use io_uring::cqueue::Entry as CompletionEntry;
use io_uring::squeue::Entry as SubmissionEntry;
use io_uring::{IoUring, opcode, types};
use std::collections::HashMap;
use std::io::Error;
use std::net::TcpListener;
use std::os::fd::AsRawFd;

const SUBMISSION_QUEUE_SIZE: u32 = 1024; // Need to be a power of two

const WELCOME_MESSAGE: &str = "Welcome to chat! What shall I call you?\n";

fn main() -> Result<(), Error> {
    let mut chat_room = ChatRoom::new()?;
    chat_room.listen()?;

    println!("Server listening...");

    chat_room.start()
}

#[repr(u8)]
enum Op {
    Accept = 1,
    Read = 2,
    Write = 3,
    Close = 4,
}

fn make_user_data(op: Op, id: i32) -> u64 {
    ((op as u64) << 32) | id as u64
}

fn user_data_op(user_data: u64) -> Option<Op> {
    match user_data >> 32 {
        1 => Some(Op::Accept),
        2 => Some(Op::Read),
        3 => Some(Op::Write),
        4 => Some(Op::Close),
        _ => None,
    }
}

fn user_data_id(user_data: u64) -> i32 {
    user_data as i32
}

struct ChatRoom {
    ring: IoUring,
    clients: HashMap<i32, Client>,

    listener: Option<TcpListener>,
}

impl ChatRoom {
    fn new() -> Result<Self, Error> {
        let ring = IoUring::new(SUBMISSION_QUEUE_SIZE)?;

        let chat_room = ChatRoom {
            ring,
            clients: HashMap::new(),
            listener: None,
        };

        Ok(chat_room)
    }

    fn listen(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind("0.0.0.0:8080")?;
        let listener_fd = listener.as_raw_fd();

        self.listener = Some(listener);

        let accept_submission = opcode::AcceptMulti::new(types::Fd(listener_fd))
            .build()
            .user_data(make_user_data(Op::Accept, listener_fd));

        unsafe {
            if let Err(_) = self.ring.submission().push(&accept_submission) {
                return Err(Error::other("io_uring submission queue is full"));
            }
        }

        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        let mut completion_entries = Vec::<CompletionEntry>::with_capacity(256);
        let mut submission_entries = Vec::<SubmissionEntry>::with_capacity(256);

        loop {
            self.ring.submitter().submit_and_wait(1)?; // Might need to continue on libc::EINTR errors according to Gemini

            completion_entries.clear();
            submission_entries.clear();

            for entry in self.ring.completion() {
                completion_entries.push(entry);
            }

            for entry in completion_entries.drain(..) {
                let entries = match user_data_op(entry.user_data()) {
                    Some(Op::Accept) => self.handle_accept(entry),
                    Some(Op::Read) => self.handle_read(entry),
                    Some(Op::Write) => self.handle_write(entry),
                    Some(Op::Close) => self.handle_close(entry),
                    None => Err(Error::other("Unknown operation")),
                };
                submission_entries.extend(entries?);
            }

            unsafe {
                if let Err(_) = self.ring.submission().push_multiple(&submission_entries) {
                    return Err(Error::other("io_uring submission queue is full"));
                }
            }
        }
    }

    fn handle_accept(&mut self, entry: CompletionEntry) -> Result<Vec<SubmissionEntry>, Error> {
        let client_fd = entry.result();
        if client_fd < 0 {
            return Err(Error::from_raw_os_error(-client_fd));
        }

        println!("Accepting client with fd: {client_fd}");

        self.clients.insert(client_fd, Client::new(client_fd));

        let client = self.clients.get_mut(&client_fd).unwrap(); // Safe because we just inserted it

        let welcome_submission = opcode::Write::new(
            types::Fd(client_fd),
            WELCOME_MESSAGE.as_ptr(),
            WELCOME_MESSAGE.len() as _,
        )
        .build()
        .user_data(make_user_data(Op::Write, client_fd));

        let read_submission = opcode::Read::new(
            types::Fd(client_fd),
            client.buffer.as_mut_ptr(),
            client.buffer.len() as _,
        )
        .build()
        .user_data(make_user_data(Op::Read, client_fd));

        Ok(vec![welcome_submission, read_submission])
    }

    fn handle_read(&mut self, entry: CompletionEntry) -> Result<Vec<SubmissionEntry>, Error> {
        let client_fd = user_data_id(entry.user_data());
        let result = entry.result();

        let client = match self.clients.get_mut(&client_fd) {
            Some(client) => client,
            None => {
                return Err(Error::other("Got message from unknown client in chat room"));
            }
        };

        if result <= 0 {
            println!("Disconnecting client {}...", client.client_fd);
            client.disconnect();
            return Ok(vec![self.close_client(client_fd)]);
        }

        let bytes_read = result as usize;
        let message = match str::from_utf8(&client.buffer[..bytes_read]) {
            Err(_) => {
                println!("Got invalid UTF-8 message from client {}", client.client_fd);
                return Err(Error::other("Got invalid UTF-8 message from client"));
            }
            Ok(value) => {
                println!("Got message: {value}");
                value
            }
        };

        let stuff = match &client.state {
            State::Joined { name } => {
                format!("[{name}]: {message}\n")
            }
            State::Pending => {
                // client.join(message);
                format!("* {message} joined the room\n")
            }
            State::Disconnected => {
                format!("* {message} has left the room\n")
            }
        };

        Ok(self.broadcast(stuff, client_fd))
    }

    fn handle_write(&mut self, entry: CompletionEntry) -> Result<Vec<SubmissionEntry>, Error> {
        let client_fd = user_data_id(entry.user_data());
        let result = entry.result();
        if result < 0 {
            eprintln!("Error writing to client {client_fd}");
            // Probably disconnect client
            return Err(Error::from_raw_os_error(-result));
        }
        Ok(vec![])
    }

    fn handle_close(&mut self, entry: CompletionEntry) -> Result<Vec<SubmissionEntry>, Error> {
        let client_fd = user_data_id(entry.user_data());
        let result = entry.result();
        if result < 0 {
            eprintln!("Error closing client {client_fd}");
            return Err(Error::from_raw_os_error(-result));
        }

        println!("Client {client_fd} disconnected");
        self.clients.remove(&client_fd);

        Ok(vec![])
    }

    fn broadcast(&self, message: String, sender: i32) -> Vec<SubmissionEntry> {
        self.clients
            .iter()
            .filter(|(client_fd, client)| {
                if **client_fd == sender {
                    return false;
                }

                if let State::Joined { name: _ } = &client.state {
                    return true;
                }
                false
            })
            .map(|(client_fd, _)| {
                opcode::Write::new(types::Fd(*client_fd), message.as_ptr(), message.len() as _)
                    .build()
                    .user_data(make_user_data(Op::Write, *client_fd))
            })
            .collect::<Vec<_>>()
    }

    fn close_client(&self, client_fd: i32) -> SubmissionEntry {
        opcode::Close::new(types::Fd(client_fd))
            .build()
            .user_data(make_user_data(Op::Close, client_fd))
    }
}

enum State {
    Pending,
    Joined { name: String },
    Disconnected,
}

struct Client {
    client_fd: i32,
    buffer: Vec<u8>, // Use Vec to allocate memory on the heap and keep stable memory pointer

    state: State,
}

impl Client {
    fn new(client_fd: i32) -> Self {
        Client {
            client_fd,
            buffer: vec![0u8; 4096],

            state: State::Pending,
        }
    }

    fn join(&mut self, name: &str) {
        self.state = State::Joined {
            name: String::from(name),
        }
    }

    fn disconnect(&mut self) {
        self.state = State::Disconnected;
    }
}
