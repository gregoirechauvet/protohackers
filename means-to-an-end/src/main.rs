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
        let mut buffer = [0u8; 9];
        let mut asset = Asset::new();

        loop {
            if let Err(_) = reader.read_exact(&mut buffer) {
                break;
            }

            let request = buffer[0] as char;
            let one = i32::from_be_bytes(buffer[1..5].try_into().unwrap());
            let two = i32::from_be_bytes(buffer[5..9].try_into().unwrap());

            match request {
                'I' => {
                    asset.insert_price(one, two);
                }
                'Q' => {
                    let mean = asset.query_mean(one, two);
                    let response = mean.to_be_bytes();
                    writer.write(&response)?;
                    writer.flush()?;
                }
                _ => {
                    println!(
                        "[Client {}] Unknown request type: {}",
                        self.peer_addr, request
                    );
                }
            }
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

struct PricePoint {
    timestamp: i32,
    price: i32,
}

struct Asset {
    prices: Vec<PricePoint>,
}

impl Asset {
    fn new() -> Self {
        Asset { prices: Vec::new() }
    }

    fn insert_price(&mut self, timestamp: i32, price: i32) {
        self.prices.push(PricePoint { price, timestamp });
    }

    fn query_mean(&self, from: i32, to: i32) -> i32 {
        let (sum, count) = self
            .prices
            .iter()
            .filter(|point| point.timestamp >= from && point.timestamp <= to)
            .fold((0i64, 0i64), |(sum, count), point| (sum + point.price as i64, count + 1));

        if count == 0 {
            return 0;
        }

        (sum / count) as i32
    }
}
