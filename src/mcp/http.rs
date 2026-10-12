use crate::mcp::types::HandlerResult;
use crate::DocEngine;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_BODY_SIZE: usize = 4 * 1024 * 1024; // 4 MB
pub const MAX_HEADER_LINE: usize = 64 * 1024; // 64 KB
pub const MAX_HEADERS_TOTAL: usize = 128 * 1024; // 128 KB
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// Active HTTP server instance.
pub struct HttpServer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl HttpServer {
    /// Return the local socket address the server is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Trigger graceful shutdown of the server accept loop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Wait for the server accept thread to exit.
    pub fn join(mut self) -> std::thread::Result<()> {
        if let Some(h) = self.join_handle.take() {
            h.join()
        } else {
            Ok(())
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Run HTTP MCP server bound to the specified socket address.
///
/// Binds directly to the provided address (it is the caller's responsibility to
/// specify a loopback address if local-only access is desired). Returns an
/// `HttpServer` handle immediately without blocking the caller.
pub fn serve_http(engine: &DocEngine, addr: impl ToSocketAddrs) -> io::Result<HttpServer> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    let bound = listener.local_addr()?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_thread = shutdown.clone();
    let engine_clone = engine.clone();

    let join_handle = std::thread::Builder::new()
        .name("agent-man-http".into())
        .spawn(move || {
            while !shutdown_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let eng = engine_clone.clone();
                        std::thread::spawn(move || {
                            let _ = handle_http_connection(eng, stream);
                        });
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        })?;

    Ok(HttpServer {
        addr: bound,
        shutdown,
        join_handle: Some(join_handle),
    })
}

fn read_capped(
    reader: &mut impl BufRead,
    into: &mut String,
    max_bytes: usize,
) -> io::Result<usize> {
    let mut taken = Read::by_ref(reader).take(max_bytes as u64);
    let read = taken.read_line(into)?;
    if read >= max_bytes && !into.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "header line exceeds maximum length",
        ));
    }
    Ok(read)
}

fn handle_http_connection(engine: DocEngine, stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(IDLE_TIMEOUT)).ok();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    loop {
        let mut request_line = String::new();
        match read_capped(&mut reader, &mut request_line, MAX_HEADER_LINE) {
            Ok(0) => break, // EOF: client hung up
            Ok(_) => {}
            Err(_) => {
                let _ = respond(
                    &mut writer,
                    400,
                    "text/plain",
                    b"request line too long",
                    true,
                );
                break;
            }
        }

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.is_empty() {
            break;
        }
        let method = parts[0];
        let target = if parts.len() > 1 { parts[1] } else { "" };
        let path = target.split(['?', '#']).next().unwrap_or("");

        // Parse headers
        let mut content_length: Option<usize> = None;
        let mut origin: Option<String> = None;
        let mut connection_close = false;
        let mut malformed_header = false;
        let mut bad_length_msg: Option<String> = None;
        let mut total_header_bytes = 0;

        loop {
            let mut header_line = String::new();
            let n = match read_capped(&mut reader, &mut header_line, MAX_HEADER_LINE) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => {
                    let _ = respond(
                        &mut writer,
                        400,
                        "text/plain",
                        b"header line too long",
                        true,
                    );
                    return Ok(());
                }
            };
            total_header_bytes += n;
            if total_header_bytes > MAX_HEADERS_TOTAL {
                respond(&mut writer, 431, "text/plain", b"headers too large", true)?;
                return Ok(());
            }

            let trimmed = header_line.trim_end();
            if trimmed.is_empty() {
                break; // End of headers
            }

            let Some((k, v)) = trimmed.split_once(':') else {
                malformed_header = true;
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "content-length" => match val.parse::<usize>() {
                    Ok(l) => content_length = Some(l),
                    Err(_) => bad_length_msg = Some("content-length is not a number".to_string()),
                },
                "origin" => origin = Some(val.to_string()),
                "connection" if val.eq_ignore_ascii_case("close") => {
                    connection_close = true;
                }
                _ => {}
            }
        }

        if malformed_header {
            respond(&mut writer, 400, "text/plain", b"malformed header", true)?;
            break;
        }

        if let Some(msg) = bad_length_msg {
            respond(&mut writer, 400, "text/plain", msg.as_bytes(), true)?;
            break;
        }

        // 1. Origin verification
        if let Some(ref o) = origin {
            if !is_local_origin(o) {
                respond(
                    &mut writer,
                    403,
                    "text/plain",
                    b"this server does not answer cross-origin requests",
                    true,
                )?;
                break;
            }
        }

        // 2. Path routing: Only "/" is served.
        // Unknown paths (such as OAuth discovery /.well-known/...) return 404 with JSON body.
        if path != "/" {
            respond(
                &mut writer,
                404,
                "application/json",
                br#"{"error":"no such path: this server serves MCP at / and nothing else"}"#,
                connection_close,
            )?;
            if connection_close {
                break;
            } else {
                continue;
            }
        }

        // 3. Method verification: Only POST is allowed at "/".
        // Non-POST requests return 405 Method Not Allowed with JSON body.
        if method != "POST" {
            respond(
                &mut writer,
                405,
                "application/json",
                br#"{"error":"this endpoint answers POST only: there is no SSE stream here"}"#,
                connection_close,
            )?;
            if connection_close {
                break;
            } else {
                continue;
            }
        }

        // 4. Content-Length verification
        let Some(length) = content_length else {
            respond(
                &mut writer,
                411,
                "text/plain",
                b"a POST needs a content-length",
                connection_close,
            )?;
            if connection_close {
                break;
            } else {
                continue;
            }
        };

        // 5. Body size cap verification
        if length > MAX_BODY_SIZE {
            respond(
                &mut writer,
                413,
                "text/plain",
                b"that body is too large",
                true,
            )?;
            break;
        }

        // 6. Read body
        let mut body_bytes = vec![0u8; length];
        reader.read_exact(&mut body_bytes)?;
        let body_str = String::from_utf8_lossy(&body_bytes);

        // 7. Dispatch request
        let res = engine.handle(&body_str);
        let output = match res {
            HandlerResult::Ready(opt) => opt,
            HandlerResult::Pending(p) => p.settled(),
        };

        match output {
            None => {
                // Notification: 202 Accepted
                respond(&mut writer, 202, "text/plain", b"", connection_close)?;
            }
            Some(resp_json) => {
                respond(
                    &mut writer,
                    200,
                    "application/json",
                    resp_json.as_bytes(),
                    connection_close,
                )?;
            }
        }

        if connection_close {
            break;
        }
    }

    Ok(())
}

fn respond(
    writer: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    connection_close: bool,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let conn = if connection_close {
        "close"
    } else {
        "keep-alive"
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: {conn}\r\n\r\n",
        body.len()
    );
    writer.write_all(head.as_bytes())?;
    writer.write_all(body)?;
    writer.flush()
}

/// Verify that origin is a trusted local address to prevent cross-origin DNS rebinding attacks.
pub fn is_local_origin(origin: &str) -> bool {
    let without_proto = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(origin);
    let host = without_proto.split(':').next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}
