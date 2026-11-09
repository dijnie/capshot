//! A tiny HTTP server for tests: replies with canned responses and records
//! each request (method, path with query, headers, body).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub struct Mock {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
}

/// Serve `responses` in order as (status, body); then stop.
pub fn serve(responses: Vec<(u16, &'static str)>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests: Arc<Mutex<Vec<Recorded>>> = Default::default();
    let log = requests.clone();
    std::thread::spawn(move || {
        for (status, reply) in responses {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let mut headers = Vec::new();
            let mut len = 0;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            log.lock().unwrap().push(Recorded { method, path, headers, body });
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    Mock { url, requests }
}
