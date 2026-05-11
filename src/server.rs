use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use crate::logger;

const WWW_ROOT: &str = "/var/www";
const WORKERS: usize = 8;

#[derive(Clone, Copy)]
pub struct Config {
    pub production: bool,
    pub php: bool,
}

struct ThreadPool {
    sender: mpsc::Sender<(TcpStream, Config)>,
}

impl ThreadPool {
    fn new(size: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<(TcpStream, Config)>();
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..size {
            let rx = Arc::clone(&receiver);
            thread::spawn(move || loop {
                let (stream, config) = rx.lock().unwrap().recv().unwrap();
                handle(stream, config);
            });
        }
        ThreadPool { sender }
    }

    fn execute(&self, stream: TcpStream, config: Config) {
        self.sender.send((stream, config)).unwrap();
    }
}

pub fn serve(bind_addr: &str, config: Config) {
    let listener = TcpListener::bind(bind_addr).unwrap();
    let pool = ThreadPool::new(WORKERS);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => pool.execute(s, config),
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

fn handle(mut stream: TcpStream, config: Config) {
    let ip = stream.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let request_line = {
        let reader = BufReader::new(&stream);
        match reader.lines().next() {
            Some(Ok(line)) => line,
            _ => return,
        }
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let uri    = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        respond(&mut stream, 405, "Method Not Allowed", "text/plain", b"Method Not Allowed");
        logger::log(&ip, method, uri, 405);
        return;
    }

    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    let rel = path.trim_start_matches('/');
    let mut file_path = PathBuf::from(WWW_ROOT).join(rel);

    if file_path.is_dir() {
        file_path.push(if config.php { "index.php" } else { "index.html" });
    }

    if file_path.components().any(|c| c == Component::ParentDir) {
        respond(&mut stream, 403, "Forbidden", "text/plain", b"Forbidden");
        logger::log(&ip, method, path, 403);
        return;
    }

    let is_php = file_path.extension().and_then(|e| e.to_str()) == Some("php");

    let status = if is_php && config.php {
        run_php(&mut stream, &file_path, method, query, config.production)
    } else if is_php {
        respond(&mut stream, 404, "Not Found", "text/plain", b"Not Found");
        404
    } else {
        serve_file(&mut stream, &file_path, method, config.production)
    };

    logger::log(&ip, method, path, status);
}

fn serve_file(stream: &mut TcpStream, file_path: &PathBuf, method: &str, production: bool) -> u16 {
    match fs::read(file_path) {
        Ok(bytes) => {
            let mime = mime_for(file_path);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(header.as_bytes());
            if method != "HEAD" {
                let _ = stream.write_all(&bytes);
            }
            200
        }
        Err(_) => {
            let msg = if production { b"Not Found".as_ref() } else { b"404 Not Found" };
            respond(stream, 404, "Not Found", "text/plain", msg);
            404
        }
    }
}

fn run_php(stream: &mut TcpStream, file_path: &PathBuf, method: &str, query: &str, production: bool) -> u16 {
    let result = std::process::Command::new("php")
        .arg(file_path)
        .env("REQUEST_METHOD", method)
        .env("QUERY_STRING", query)
        .env("SCRIPT_FILENAME", file_path)
        .env("DOCUMENT_ROOT", WWW_ROOT)
        .output();

    match result {
        Ok(out) if out.status.success() => {
            respond(stream, 200, "OK", "text/html; charset=utf-8", &out.stdout);
            200
        }
        Ok(out) => {
            let body: &[u8] = if production {
                b"An error occurred"
            } else if out.stderr.is_empty() {
                b"PHP error"
            } else {
                &out.stderr
            };
            respond(stream, 500, "Internal Server Error", "text/plain", body);
            500
        }
        Err(_) => {
            let msg = if production { b"An error occurred".as_ref() } else { b"PHP is not installed" };
            respond(stream, 500, "Internal Server Error", "text/plain", msg);
            500
        }
    }
}

fn respond(stream: &mut TcpStream, code: u16, reason: &str, mime: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

fn mime_for(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css")                => "text/css",
        Some("js")                 => "application/javascript",
        Some("json")               => "application/json",
        Some("png")                => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif")                => "image/gif",
        Some("svg")                => "image/svg+xml",
        Some("ico")                => "image/x-icon",
        Some("txt")                => "text/plain",
        _                          => "application/octet-stream",
    }
}
