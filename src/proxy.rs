use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const WORKERS: usize = 8;

type Routes = Arc<HashMap<String, String>>;

// ── Thread pool ───────────────────────────────────────────────────────────────

struct ThreadPool {
    sender: mpsc::Sender<(TcpStream, Routes)>,
}

impl ThreadPool {
    fn new(size: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<(TcpStream, Routes)>();
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..size {
            let rx = Arc::clone(&receiver);
            thread::spawn(move || loop {
                let (stream, routes) = rx.lock().unwrap().recv().unwrap();
                handle(stream, routes);
            });
        }
        ThreadPool { sender }
    }

    fn execute(&self, stream: TcpStream, routes: Routes) {
        self.sender.send((stream, routes)).unwrap();
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn load_routes(path: &str) -> HashMap<String, String> {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Could not read routes file: {path}"));

    let mut routes = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut parts = line.split_whitespace();
        let host    = parts.next().expect("missing host in route").to_lowercase();
        let backend = parts.next().expect("missing backend in route").to_string();
        println!("  route: {host} → {backend}");
        routes.insert(host, backend);
    }
    routes
}

pub fn run(bind_addr: &str, routes: HashMap<String, String>) {
    let routes = Arc::new(routes);
    let pool   = ThreadPool::new(WORKERS);
    let listener = TcpListener::bind(bind_addr)
        .unwrap_or_else(|e| panic!("Proxy failed to bind {bind_addr}: {e}"));
    println!("Proxy listening on http://{bind_addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => pool.execute(s, Arc::clone(&routes)),
            Err(e) => eprintln!("proxy accept error: {e}"),
        }
    }
}

// ── Connection handler ────────────────────────────────────────────────────────

fn handle(mut client: TcpStream, routes: Routes) {
    let client_ip = client.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Read raw request headers byte-by-byte until \r\n\r\n
    let header_buf = match read_headers(&client) {
        Some(b) => b,
        None    => return,
    };

    let headers_str = String::from_utf8_lossy(&header_buf);

    // Parse Host header (strip port if present)
    let host = extract_header(&headers_str, "host")
        .map(|h| h.split(':').next().unwrap_or(h).trim().to_lowercase())
        .unwrap_or_default();

    // Parse Content-Length for request bodies (POST etc.)
    let content_length: usize = extract_header(&headers_str, "content-length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);

    // Look up backend
    let backend_addr = match routes.get(&host) {
        Some(addr) => addr.clone(),
        None => {
            error_response(&mut client, 502, &format!("No route configured for: {host}"));
            return;
        }
    };

    // Connect to backend
    let mut backend = match TcpStream::connect(&backend_addr) {
        Ok(s)  => s,
        Err(e) => {
            error_response(&mut client, 502, &format!("Backend unavailable: {e}"));
            return;
        }
    };

    // Forward headers to backend, injecting X-Real-IP before the blank line
    let inject = format!("X-Real-IP: {client_ip}\r\n");
    let _ = backend.write_all(&header_buf[..header_buf.len() - 2]); // strip final \r\n
    let _ = backend.write_all(inject.as_bytes());
    let _ = backend.write_all(b"\r\n");                              // restore blank line

    // Forward request body if present (e.g. POST data)
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        if client.read_exact(&mut body).is_ok() {
            let _ = backend.write_all(&body);
        }
    }

    // Stream backend response back to client
    pipe(&mut backend, &mut client);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Read HTTP headers byte-by-byte until \r\n\r\n.
// Safe to use with TcpStream because all reads advance the same OS socket.
fn read_headers(stream: &TcpStream) -> Option<Vec<u8>> {
    let mut buf  = Vec::new();
    let mut byte = [0u8; 1];
    let mut reader: &TcpStream = stream;
    loop {
        match reader.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") { return Some(buf); }
                if buf.len() > 64 * 1024     { return None; } // 64 KB header limit
            }
        }
    }
}

fn extract_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}:");
    headers
        .lines()
        .find(|l| l.to_lowercase().starts_with(&prefix))
        .map(|l| l[prefix.len()..].trim())
}

fn pipe(from: &mut TcpStream, to: &mut TcpStream) {
    let mut buf = [0u8; 8192];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => { if to.write_all(&buf[..n]).is_err() { break; } }
        }
    }
}

fn error_response(stream: &mut TcpStream, code: u16, msg: &str) {
    let response = format!(
        "HTTP/1.1 {code} Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{msg}",
        msg.len()
    );
    let _ = stream.write_all(response.as_bytes());
}
