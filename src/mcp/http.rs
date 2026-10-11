use crate::mcp::types::HandlerResult;
use crate::DocEngine;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const MAX_BODY_SIZE: usize = 4 * 1024 * 1024; // 4 MB

/// Run HTTP MCP server bound to the specified loopback socket address.
pub fn serve_http(engine: &DocEngine, addr: impl ToSocketAddrs) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let engine_clone = engine.clone();
                std::thread::spawn(move || {
                    let _ = handle_http_connection(engine_clone, stream);
                });
            }
            Err(e) => {
                eprintln!("agent-doc http accept error: {e}");
            }
        }
    }
    Ok(())
}

fn handle_http_connection(engine: DocEngine, mut stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;

    loop {
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        match reader.read_line(&mut request_line) {
            Ok(0) => break, // Connection closed by client
            Ok(_) => {}
            Err(_) => break, // Timeout or error
        }

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            break;
        }

        let method = parts[0];
        let _path = parts[1];

        // Parse headers
        let mut content_length = None;
        let mut origin = None;
        let mut connection_close = false;

        loop {
            let mut header_line = String::new();
            if reader.read_line(&mut header_line)? == 0 {
                break;
            }
            let trimmed = header_line.trim();
            if trimmed.is_empty() {
                break; // End of headers
            }

            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().to_lowercase();
                let val = v.trim();
                match key.as_str() {
                    "content-length" => {
                        content_length = val.parse::<usize>().ok();
                    }
                    "origin" => {
                        origin = Some(val.to_string());
                    }
                    "connection" if val.eq_ignore_ascii_case("close") => {
                        connection_close = true;
                    }
                    _ => {}
                }
            }
        }

        // Method verification
        if method != "POST" {
            let resp =
                "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(resp.as_bytes())?;
            break;
        }

        // Local origin verification
        if let Some(ref o) = origin {
            if !is_local_origin(o) {
                let resp =
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(resp.as_bytes())?;
                break;
            }
        }

        // Content-Length verification
        let len = match content_length {
            Some(l) => l,
            None => {
                let resp = "HTTP/1.1 411 Length Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                stream.write_all(resp.as_bytes())?;
                break;
            }
        };

        // Size cap verification
        if len > MAX_BODY_SIZE {
            let resp =
                "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(resp.as_bytes())?;
            break;
        }

        // Read request body
        let mut body_bytes = vec![0u8; len];
        reader.read_exact(&mut body_bytes)?;
        let body_str = String::from_utf8_lossy(&body_bytes);

        // Dispatch request
        let res = engine.handle(&body_str);
        let output = match res {
            HandlerResult::Ready(opt) => opt,
            HandlerResult::Pending(p) => p.settled(),
        };

        match output {
            None => {
                // Notification: return 204 No Content
                let conn_header = if connection_close {
                    "close"
                } else {
                    "keep-alive"
                };
                let resp = format!(
                    "HTTP/1.1 204 No Content\r\nConnection: {}\r\n\r\n",
                    conn_header
                );
                stream.write_all(resp.as_bytes())?;
            }
            Some(resp_json) => {
                let bytes = resp_json.as_bytes();
                let conn_header = if connection_close {
                    "close"
                } else {
                    "keep-alive"
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n{}",
                    bytes.len(),
                    conn_header,
                    resp_json
                );
                stream.write_all(resp.as_bytes())?;
            }
        }

        stream.flush()?;
        if connection_close {
            break;
        }
    }

    Ok(())
}

/// Verify that origin is a trusted local address to prevent DNS rebinding attacks.
pub fn is_local_origin(origin: &str) -> bool {
    let without_proto = match origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    {
        Some(s) => s,
        None => return false,
    };

    let host = without_proto.split(':').next().unwrap_or("");
    host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1"
}
