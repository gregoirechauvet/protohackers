mod arena;

use crate::arena::{Arena, Index};
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
    chat_room.listen("0.0.0.0:8080")?;

    println!("Server listening...");

    chat_room.start()
}

enum Op {
    Accept,
    Read { client_fd: i32, buffer: Vec<u8> },
    Write { client_fd: i32, buffer: String },
    Close { client_fd: i32 },
}

struct ChatRoom {
    ring: IoUring,
    listener: Option<TcpListener>,

    ops: Arena<Op>,
    clients: HashMap<i32, Client>,
}

impl ChatRoom {
    fn new() -> Result<Self, Error> {
        let ring = IoUring::new(SUBMISSION_QUEUE_SIZE)?;

        let chat_room = ChatRoom {
            ring,
            listener: None,

            ops: Arena::new(),
            clients: HashMap::new(),
        };

        Ok(chat_room)
    }

    fn listen(&mut self, addr: &str) -> Result<(), Error> {
        let listener = TcpListener::bind(addr)?;
        let listener_fd = listener.as_raw_fd();

        self.listener = Some(listener);

        let accept_op = self.ops.insert(Op::Accept);
        let accept_submission = opcode::AcceptMulti::new(types::Fd(listener_fd))
            .build()
            .user_data(accept_op.into_user_data());

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
                let index = Index::from_user_data(entry.user_data());

                let entries = match self.ops.get(index) {
                    Some(Op::Accept) => {
                        self.handle_accept(entry)
                    },
                    _ => {
                        match self.ops.remove(index) {
                            Some(Op::Accept) => self.handle_accept(entry),
                            Some(Op::Read { client_fd, buffer }) => {
                                self.handle_read(entry, client_fd, buffer)
                            }
                            Some(Op::Write {
                                     client_fd,
                                     buffer: _,
                                 }) => self.handle_write(entry, client_fd),
                            Some(Op::Close { client_fd }) => self.handle_close(entry, client_fd),
                            None => {
                                eprintln!("Got completion entry for unknown operation");
                                continue;
                            }
                        }
                    }
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

        self.clients.insert(client_fd, Client::new());

        let index = self.ops.insert(Op::Write {
            client_fd,
            buffer: WELCOME_MESSAGE.to_owned(),
        });
        let write_ref = self.ops.get_mut(index);

        let welcome_submission = match write_ref {
            Some(Op::Write {
                client_fd: _,
                buffer,
            }) => opcode::Write::new(types::Fd(client_fd), buffer.as_ptr(), buffer.len() as _)
                .build()
                .user_data(index.into_user_data()),
            _ => unreachable!("Got invalid write reference"),
        };

        let index = self.ops.insert(Op::Read {
            client_fd,
            buffer: vec![0; 1024],
        });
        let read_ref = self.ops.get_mut(index);

        let read_submission = match read_ref {
            Some(Op::Read {
                client_fd: _,
                buffer,
            }) => opcode::Read::new(types::Fd(client_fd), buffer.as_mut_ptr(), buffer.len() as _)
                .build()
                .user_data(index.into_user_data()),
            _ => unreachable!("Got invalid read reference"),
        };

        Ok(vec![welcome_submission, read_submission])
    }

    fn handle_read(
        &mut self,
        entry: CompletionEntry,
        client_fd: i32,
        buffer: Vec<u8>,
    ) -> Result<Vec<SubmissionEntry>, Error> {
        let result = entry.result();

        if result <= 0 {
            // TODO
            // println!("Disconnecting client {}...", client.client_fd);
            // client.disconnect();
            // return Ok(vec![client.close()]);
            return Ok(vec![]);
        }

        let client = match self.clients.get_mut(&client_fd) {
            Some(client) => client,
            None => {
                eprintln!("Got message from unknown client in chat room");
                return Ok(vec![]);
            }
        };

        let bytes_read = result as usize;
        client.incoming.extend_from_slice(&buffer[..bytes_read]);

        let index = self.ops.insert(Op::Read {
            client_fd,
            buffer, // Reuse buffer
        });

        let read_submission = match self.ops.get_mut(index) {
            Some(Op::Read {
                client_fd: _,
                buffer,
            }) => {
                opcode::Read::new(types::Fd(client_fd), buffer.as_mut_ptr(), buffer.len() as _)
                    .build()
                    .user_data(index.into_user_data())
            }
            _ => unreachable!("Got invalid read reference"),
        };

        let line_break_position = client.incoming.iter().position(|&byte| byte == b'\n');
        let line_break = match line_break_position {
            Some(value) => value,
            None => {
                return Ok(vec![read_submission]);
            }
        };

        let frame = client.incoming.drain(..=line_break).collect::<Vec<_>>();
        let mut message = match String::from_utf8(frame) {
            Ok(value) => value,
            Err(_) => {
                eprintln!("Got invalid UTF-8 message from client");
                // Disconnect client
                return Ok(vec![]);
            }
        };

        message = message.trim_end().to_string();
        if message.is_empty() {
            // TODO
            return Ok(vec![]);
        }

        let broadcast_message = match &client.state {
            State::Joined { name } => {
                format!("[{name}]: {message}\n")
            }
            State::Pending => {
                client.join(message.clone());
                format!("* {message} joined the room\n")
            }
        };

        let mut broadcast_submissions = self.broadcast(broadcast_message, client_fd);
        broadcast_submissions.push(read_submission);

        Ok(broadcast_submissions)
    }

    fn welcome(&mut self, welcome_fd: i32) -> SubmissionEntry {
        let names = self
            .clients
            .iter()
            .filter_map(|(&fd, client)| {
                if fd == welcome_fd {
                    return None;
                }

                match &client.state {
                    State::Joined { name } => Some(name.clone()),
                    _ => None,
                }
            })
            .collect::<Vec<String>>()
            .join(", ");

        let message = format!("* The room contains: {names}\n");

        let index = self.ops.insert(Op::Write {
            client_fd: welcome_fd,
            buffer: message,
        });

        match self.ops.get_mut(index) {
            Some(Op::Write {
                client_fd: _,
                buffer,
            }) => opcode::Write::new(types::Fd(welcome_fd), buffer.as_ptr(), buffer.len() as _)
                .build()
                .user_data(index.into_user_data()),
            _ => unreachable!("Got invalid write reference"),
        }
    }

    fn handle_write(
        &mut self,
        entry: CompletionEntry,
        client_fd: i32,
    ) -> Result<Vec<SubmissionEntry>, Error> {
        let result = entry.result();
        if result < 0 {
            eprintln!("Error writing to client {client_fd}");
            // Probably disconnect client
        }

        Ok(vec![])
    }

    fn handle_close(
        &mut self,
        entry: CompletionEntry,
        client_fd: i32,
    ) -> Result<Vec<SubmissionEntry>, Error> {
        println!("Client {client_fd} disconnected");

        let result = entry.result();
        if result < 0 {
            eprintln!(
                "Error closing client {client_fd}: {}",
                Error::from_raw_os_error(-result)
            );
            return Ok(vec![]);
        }

        let client = match self.clients.get(&client_fd) {
            Some(client) => client,
            None => {
                eprintln!("Got close for unknown client {client_fd}");
                return Ok(vec![]);
            }
        };

        let message = match &client.state {
            State::Joined { name } => {
                format!("* {name} has left the room")
            }
            _ => return Ok(vec![]),
        };

        self.clients.remove(&client_fd);

        Ok(self.broadcast(message, client_fd))
    }

    fn broadcast(&mut self, message: String, sender: i32) -> Vec<SubmissionEntry> {
        let recipient_fds: Vec<i32> = self
            .clients
            .iter()
            .filter(|&(client_fd, client)| {
                if *client_fd == sender {
                    return false;
                }

                if let State::Joined { .. } = client.state {
                    return true;
                }
                false
            })
            .map(|(&client_fd, _)| client_fd)
            .collect();

        recipient_fds
            .iter()
            .map(|&client_fd| {
                let index = self.ops.insert(Op::Write {
                    client_fd,
                    buffer: message.clone(),
                });

                match self.ops.get_mut(index) {
                    Some(Op::Write {
                        client_fd: _,
                        buffer,
                    }) => {
                        opcode::Write::new(types::Fd(client_fd), buffer.as_ptr(), buffer.len() as _)
                            .build()
                            .user_data(index.into_user_data())
                    }
                    _ => unreachable!("Got invalid write reference"),
                }
            })
            .collect()
    }
}

enum State {
    Pending,
    Joined { name: String },
}

struct Client {
    state: State,

    incoming: Vec<u8>,
}

impl Client {
    fn new() -> Self {
        Client {
            state: State::Pending,

            incoming: Vec::new(),
        }
    }

    fn join(&mut self, name: String) {
        self.state = State::Joined { name }
    }
}
