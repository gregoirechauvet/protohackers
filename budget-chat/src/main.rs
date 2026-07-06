use std::io::{BufReader, BufWriter, Error, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread;

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();

    println!("Server listening...");

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to accept connection: {e}");
                continue;
            }
        };

        thread::spawn(|| {
            let _ = handle_connection(stream);
        });
    }
}

struct ConnectionHandler {
    stream: TcpStream,
    peer_addr: SocketAddr,
}

impl ConnectionHandler {
    fn new(stream: TcpStream) -> Result<Self, Error> {
        let peer_addr = stream.peer_addr()?;
        println!("[Client {peer_addr}] Connected");
        Ok(ConnectionHandler { stream, peer_addr })
    }

    fn handle(&mut self) -> Result<(), Error> {
        let mut reader = BufReader::new(&self.stream);
        let mut writer = BufWriter::new(&self.stream);

        loop {
            // if let Err(_) = reader.read_line(&mut buffer) {
            //     break;
            // }
        }

        Ok(())
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        println!("[Client {}] Shutting down", self.peer_addr);
        if let Err(e) = self.stream.shutdown(Shutdown::Both) {
            eprintln!("[Client {}] Error shutting down: {}", self.peer_addr, e);
        }
    }
}

fn handle_connection(stream: TcpStream) -> Result<(), Error> {
    ConnectionHandler::new(stream)?.handle()
}
