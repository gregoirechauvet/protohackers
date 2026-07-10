use io_uring::cqueue::Entry;
use io_uring::{IoUring, opcode, types, SubmissionQueue};
use std::collections::HashMap;
use std::io::Error;
use std::net::TcpListener;
use std::os::fd::AsRawFd;

const SUBMISSION_QUEUE_SIZE: u32 = 16; // Need to be a power of two

fn main() -> Result<(), Error> {
    let mut ring = IoUring::new(SUBMISSION_QUEUE_SIZE)?;
    let (submitter, mut submission_queue, mut completion_queue) = ring.split();

    let listener = TcpListener::bind("0.0.0.0:8080")?;
    let listener_fd = listener.as_raw_fd();

    let accept_submission = opcode::AcceptMulti::new(types::Fd(listener_fd))
        .build()
        .user_data(listener_fd as _);

    unsafe {
        if let Err(_) = submission_queue.push(&accept_submission) {
            return Err(Error::other("io_uring submission queue is full"));
        }
    }

    submission_queue.sync();

    println!("Server listening...");

    let mut clients: HashMap<i32, Client> = HashMap::new();

    loop {
        submitter.submit_and_wait(1)?; // Might need to continue on libc::EINTR errors according to Gemini
        completion_queue.sync();

        for entry in &mut completion_queue {
            match entry.user_data() {
                x if x == listener_fd as u64 => {
                    let client_fd = match handle_accept_event(entry) {
                        Ok(client_fd) => client_fd,
                        Err(_) => {
                            eprintln!("Error accepting connection");
                            continue;
                        }
                    };

                    match setup_client(&mut submission_queue, client_fd) {
                        Ok(client) => {
                            clients.insert(client_fd, client);
                        }
                        Err(_) => {
                            eprintln!("Cannot read client socket");
                            continue;
                        }
                    };
                }
                x => {
                    let client_fd = x as i32;
                    let result = entry.result();

                    if let Some(client) = clients.remove(&client_fd) {
                        if result <= 0 {
                            // TODO: Shutdown client socket
                            println!("Client disconnected or error");
                            continue;
                        }

                        let bytes_read = result as usize;
                        if let Ok(value) = str::from_utf8(&client.buffer[..bytes_read]) {
                            println!("Got message: {value}");
                        }

                        clients.insert(client_fd, client);
                    } else {
                        eprintln!("Got message from unknown client: {client_fd}");
                    }
                }
            };
        }
    }
}

fn handle_accept_event(entry: Entry) -> Result<i32, Error> {
    let client_fd = entry.result();
    if client_fd < 0 {
        return Err(Error::from_raw_os_error(client_fd));
    }

    println!("Accepting client with fd: {client_fd}");

    Ok(client_fd)
}

fn setup_client(
    submission_queue: &mut SubmissionQueue,
    client_fd: i32,
) -> Result<Client, Error> {
    let mut client = Client::new(client_fd);

    let welcome_message = b"Welcome to chat! What shall I call you?\n";
    let welcome_submission = opcode::Write::new(types::Fd(client_fd), welcome_message.as_ptr(), welcome_message.len() as _).
        build().
        user_data(client_fd as _);

    let read_submission =
        opcode::Read::new(types::Fd(client_fd), client.buffer.as_mut_ptr(), client.buffer.len() as _)
            .build()
            .user_data(client_fd as _);

    unsafe {
        if let Err(_) = submission_queue.push_multiple(&[welcome_submission, read_submission]) {
            return Err(Error::other("io_uring submission queue is full"));
        }
    }

    submission_queue.sync();

    Ok(client)
}

enum State {
    Pending,
    Joined { name: String },
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

            state: State::Pending
        }
    }

    fn join(&mut self, name: String) {
        self.state = State::Joined { name }
    }
}
