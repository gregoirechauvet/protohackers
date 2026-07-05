use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Error, ErrorKind, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread;

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();

    println!("Server listening...");

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        thread::spawn(|| match handle_connection(stream) {
            Ok(()) => {
                println!("Client disconnected");
            }
            Err(e) => {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}

#[derive(Serialize, Deserialize)]
struct InputPayload {
    method: String,
    number: f64,
}

#[derive(Serialize, Deserialize)]
struct Response {
    method: String,
    prime: bool,
}

struct ConnectionHandler {
    stream: TcpStream,
    peer_addr: SocketAddr,
}

impl ConnectionHandler {
    fn new(stream: TcpStream) -> Result<Self, Error> {
        let peer_addr = stream.peer_addr()?;
        println!("Connection established with client {}", peer_addr);
        Ok(ConnectionHandler { stream, peer_addr })
    }

    fn handle(&mut self) -> Result<(), Error> {
        let mut reader = BufReader::new(&self.stream);
        let mut writer = BufWriter::new(&self.stream);
        let mut line_buffer = String::new();

        loop {
            line_buffer.clear();
            let bytes_read = reader.read_line(&mut line_buffer)?;
            if bytes_read == 0 {
                break;
            }

            println!(
                "[Client {}] Received payload: {}",
                self.peer_addr,
                line_buffer.trim()
            );

            let data: InputPayload = match serde_json::from_str(&line_buffer) {
                Ok(value) => value,
                Err(err) => {
                    writer.write_all(b"malformed\n")?;
                    writer.flush()?;
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("JSON parsing error: {}", err),
                    ));
                }
            };

            if data.method != "isPrime" {
                writer.write_all(b"malformed\n")?;
                writer.flush()?;
                return Err(Error::new(ErrorKind::InvalidInput, "Method not supported"));
            }

            let response = Response {
                method: "isPrime".to_owned(),
                prime: is_prime(data.number),
            };

            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }

        println!("[Client {}] Disconnected", self.peer_addr);
        Ok(())
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        println!("Shutting down connection for client {}", self.peer_addr);
        if let Err(e) = self.stream.shutdown(Shutdown::Both) {
            eprintln!("Error shutting down stream for {}: {}", self.peer_addr, e);
        }
    }
}

fn handle_connection(stream: TcpStream) -> Result<(), Error> {
    ConnectionHandler::new(stream)?.handle()
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
