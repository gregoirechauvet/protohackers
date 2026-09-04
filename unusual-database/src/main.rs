use std::collections::HashMap;
use std::io::Error;
use std::net::UdpSocket;

const MAX_PAYLOAD_SIZE: usize = 1000;
const VERSION_MESSAGE: &str = "version=Samy's database - 1.0.0";

fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:8080")?;

    let mut database = HashMap::<String, String>::new();
    let mut receive_buffer = [0; MAX_PAYLOAD_SIZE];

    loop {
        let (size_read, client_addr) = match socket.recv_from(&mut receive_buffer) {
            Ok(size) => size,
            Err(_) => continue
        };
        println!("Received message from {client_addr}");

        let value = match str::from_utf8(&receive_buffer[..size_read]) {
            Ok(v) => v,
            Err(_) => continue
        };
        println!("[{client_addr}] {value}");

        if value == "version" {
            _ = socket.send_to(VERSION_MESSAGE.as_bytes(), client_addr);
            continue
        }

        match value.split_once('=') {
            Some((key, val)) => {
                database.insert(key.to_owned(), val.to_owned());
            },
            None => {
                let content = match database.get(value) {
                    Some(val) => val,
                    None => "",
                };

                let response = format!("{value}={content}");
                println!("Responding: {response}");
                _ = socket.send_to(response.as_bytes(), client_addr);

            }
        }
    }
}
