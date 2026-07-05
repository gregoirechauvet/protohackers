use std::io::{BufReader, BufWriter, Error, ErrorKind, BufRead, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use serde::{Deserialize, Serialize};
use std::os::fd::AsRawFd;

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();

    println!("Server listening...");

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        match handle_connection(stream) {
            Ok(()) => {
                println!("Client disconnected");
            }
            Err(e) => {
                eprintln!("Error handling connection: {}", e);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct InputPayload {
    method: String,
    value: f64,
}

#[derive(Serialize, Deserialize)]
struct Response {
    method: String,
    prime: bool,
}

struct ConnectionHandler {
    stream: TcpStream,
}

impl ConnectionHandler {
    fn new(stream: TcpStream) -> Self {
        println!("Connection established with client {}", stream.as_raw_fd());
        ConnectionHandler { stream }
    }

    fn handle(&mut self) -> Result<(), Error> {
        let client_id = self.stream.as_raw_fd();

        let mut buf_reader = BufReader::new(&self.stream);
        let mut line_buffer = String::new();

        loop {
            let bytes_read = buf_reader.read_line(&mut line_buffer)?;
            if bytes_read == 0 {
                break;
            }

            println!("[Client {client_id}] Received payload: {line_buffer}");

            let new_buf_reader = BufReader::new(line_buffer.as_bytes());
            let json_res = serde_json::from_reader::<_, InputPayload>(new_buf_reader);

            let data = match json_res {
                Ok(value) => value,
                Err(err) => {
                    self.stream.write_all(b"malformed")?;
                    return Err(Error::new(ErrorKind::InvalidInput, format!("JSON parsing error: {}", err)));
                }
            };

            if data.method != "isPrime" {
                self.stream.write_all(b"malformed")?;
                return Err(Error::new(ErrorKind::InvalidInput, "Method not supported"));
            }

            let response = Response {
                method: "isPrime".to_owned(),
                prime: is_prime(data.value),
            };

            let serialized_response = serde_json::to_vec(&response)?;
            let mut buf_writer = BufWriter::new(&self.stream);
            buf_writer.write_all(&serialized_response)?;
            buf_writer.write_all(b"\n")?;
            buf_writer.flush()?;
        }

        println!("[Client {client_id}] Disconnected");
        Ok(())
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        println!("Shutting down connection for client {}", self.stream.as_raw_fd());
        if let Err(e) = self.stream.shutdown(Shutdown::Both) {
            eprintln!("Error shutting down stream for client {}: {}", self.stream.as_raw_fd(), e);
        }
    }
}

fn handle_connection(stream: TcpStream) -> Result<(), Error> {
    let mut handler = ConnectionHandler::new(stream);
    handler.handle()
}

fn is_prime(value: f64) -> bool {
    let integer = value as i64;
    if integer as f64 != value || integer <= 1 {
        return false;
    }

    if integer == 2 {
        return true;
    }

    if integer % 2 == 0 {
        return false;
    }

    for i in (3..=integer.isqrt()).step_by(2) {
        if integer % i == 0 {
            return false;
        }
    }

    true
}
