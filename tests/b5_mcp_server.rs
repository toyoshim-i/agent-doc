#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
#[test]
fn test_b5_mcp_server_acceptance() {
    use agent_man::mcp::{HandlerResult, Tool, ToolResult};
    use agent_man::DocEngine;
    use serde_json::json;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpStream;

    let mut engine = DocEngine::new("kinema").with_scheme("kinema");

    engine.add_page(
        "docs/intro.md",
        r#"---
id: intro
title: Intro
summary: Welcome
category: guide
---
# Intro
"#,
    );

    let hello_tool = Tool::builder("hello", "Say hello")
        .handler(|_| ToolResult::text("world").into())
        .build();
    engine.register_tool(hello_tool).unwrap();

    // 1. Direct invocation via handle(): initialize, ping, tools/list, tools/call
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    })
    .to_string();

    match engine.handle(&init_req) {
        HandlerResult::Ready(Some(res)) => {
            let v: serde_json::Value = serde_json::from_str(&res).unwrap();
            assert_eq!(v["id"], 1);
            assert_eq!(v["result"]["serverInfo"]["name"], "kinema");
        }
        _ => panic!("Expected Ready response for initialize"),
    }

    // Engine configured with with_version returns host version
    let engine_with_version = engine.clone().with_version("9.9.9");
    match engine_with_version.handle(&init_req) {
        HandlerResult::Ready(Some(res)) => {
            let v: serde_json::Value = serde_json::from_str(&res).unwrap();
            assert_eq!(v["result"]["serverInfo"]["version"], "9.9.9");
        }
        _ => panic!("Expected Ready response for initialize"),
    }

    // Notification (no id) produces None
    let notif_req = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
    .to_string();
    match engine.handle(&notif_req) {
        HandlerResult::Ready(None) => {}
        other => panic!(
            "Expected Ready(None) for notification, got {:?}",
            other.is_ready()
        ),
    }

    // Unknown method produces -32601
    let unknown_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "unknown_method"
    })
    .to_string();
    match engine.handle(&unknown_req) {
        HandlerResult::Ready(Some(res)) => {
            let v: serde_json::Value = serde_json::from_str(&res).unwrap();
            assert_eq!(v["error"]["code"], -32601);
        }
        _ => panic!("Expected error response"),
    }

    // 2. HTTP Server acceptance test on loopback port
    let server = engine.serve_http("127.0.0.1:0").unwrap();
    let addr = server.local_addr();

    // 2a. Connect via HTTP POST (keep-alive)
    let mut stream = TcpStream::connect(addr).unwrap();
    let post_body = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "hello",
            "arguments": {}
        }
    })
    .to_string();

    let request = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nOrigin: http://localhost:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
        addr,
        addr.port(),
        post_body.len(),
        post_body
    );

    stream.write_all(request.as_bytes()).unwrap();

    let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(status_line.contains("200 OK"), "Status: {}", status_line);

    // 2b. Test pipelined requests: send 2 requests in a single write on the same stream
    let req1 = json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "ping"
    })
    .to_string();
    let req2 = json!({
        "jsonrpc": "2.0",
        "id": 102,
        "method": "tools/call",
        "params": { "name": "hello", "arguments": {} }
    })
    .to_string();

    let pipelined = format!(
        "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}POST / HTTP/1.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        req1.len(),
        req1,
        req2.len(),
        req2
    );
    let mut pipe_stream = TcpStream::connect(addr).unwrap();
    pipe_stream.write_all(pipelined.as_bytes()).unwrap();
    let mut pipe_response = String::new();
    pipe_stream.read_to_string(&mut pipe_response).unwrap();

    // Both responses must be returned!
    assert!(
        pipe_response.contains("\"id\":101"),
        "Missing response 101: {}",
        pipe_response
    );
    assert!(
        pipe_response.contains("\"id\":102"),
        "Missing response 102: {}",
        pipe_response
    );

    // 2c. Unserved paths return 404 with JSON body (Karakuri protocol_and_http.rs:352)
    let mut path_stream = TcpStream::connect(addr).unwrap();
    path_stream.write_all(b"GET /.well-known/oauth-protected-resource HTTP/1.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    let mut path_resp = String::new();
    path_stream.read_to_string(&mut path_resp).unwrap();
    assert!(path_resp.contains("404 Not Found"), "Resp: {}", path_resp);
    let json_part = path_resp.split("\r\n\r\n").nth(1).unwrap();
    let v: serde_json::Value = serde_json::from_str(json_part).expect("404 body must be JSON");
    assert!(v.get("error").is_some());

    // 2d. Non-POST on "/" returns 405 with JSON body
    let mut method_stream = TcpStream::connect(addr).unwrap();
    method_stream
        .write_all(b"GET / HTTP/1.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut method_resp = String::new();
    method_stream.read_to_string(&mut method_resp).unwrap();
    assert!(
        method_resp.contains("405 Method Not Allowed"),
        "Resp: {}",
        method_resp
    );
    let method_json = method_resp.split("\r\n\r\n").nth(1).unwrap();
    let mv: serde_json::Value = serde_json::from_str(method_json).expect("405 body must be JSON");
    assert!(mv.get("error").is_some());

    // 2e. Enormous Content-Length returns 413 without crashing
    let mut big_stream = TcpStream::connect(addr).unwrap();
    big_stream
        .write_all(b"POST / HTTP/1.1\r\nContent-Length: 1000000000\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut big_resp = String::new();
    big_stream.read_to_string(&mut big_resp).unwrap();
    assert!(
        big_resp.contains("413 Payload Too Large"),
        "Resp: {}",
        big_resp
    );

    // 2f. Cross-origin request returns 403
    let mut evil_stream = TcpStream::connect(addr).unwrap();
    evil_stream.write_all(b"POST / HTTP/1.1\r\nOrigin: https://evil.example\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
    let mut evil_resp = String::new();
    evil_stream.read_to_string(&mut evil_resp).unwrap();
    assert!(evil_resp.contains("403 Forbidden"), "Resp: {}", evil_resp);

    // 2g. Non-numeric Content-Length returns 400
    let mut bad_len_stream = TcpStream::connect(addr).unwrap();
    bad_len_stream
        .write_all(b"POST / HTTP/1.1\r\nContent-Length: 12x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut bad_len_resp = String::new();
    bad_len_stream.read_to_string(&mut bad_len_resp).unwrap();
    assert!(
        bad_len_resp.contains("400 Bad Request"),
        "Resp: {}",
        bad_len_resp
    );
    assert!(bad_len_resp.contains("not a number"));

    // 2h. Server shutdown
    server.shutdown();

    // 3. Stdio test via serve_stdio_io
    let stdio_input = format!(
        "{}\n{}\n",
        json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "ping"
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 101,
            "method": "tools/call",
            "params": {
                "name": "hello",
                "arguments": {}
            }
        })
    );
    let mut stdio_output = Vec::new();
    engine
        .serve_stdio_io(stdio_input.as_bytes(), &mut stdio_output)
        .unwrap();

    let output_str = String::from_utf8(stdio_output).unwrap();
    let lines: Vec<&str> = output_str.lines().collect();
    assert_eq!(lines.len(), 2);
    let ping_res: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(ping_res["id"], 100);
    assert_eq!(ping_res["result"], json!({}));

    let call_res: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(call_res["id"], 101);
    assert!(call_res["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("world"));

    // 4. Karakuri-style non-Sync PendingResult with mpsc::Receiver
    struct KarakuriAsyncTool {
        rx: std::sync::mpsc::Receiver<String>,
    }

    impl agent_man::mcp::PendingResult<agent_man::mcp::ToolResult> for KarakuriAsyncTool {
        fn poll_settled(&mut self) -> Option<agent_man::mcp::ToolResult> {
            match self.rx.try_recv() {
                Ok(msg) => Some(agent_man::mcp::ToolResult::text(msg)),
                Err(_) => None,
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        fn settled(self: Box<Self>) -> agent_man::mcp::ToolResult {
            let msg = self.rx.recv().unwrap();
            agent_man::mcp::ToolResult::text(msg)
        }
    }

    let async_tool = Tool::builder("async_op", "Async operation")
        .handler(move |_| {
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send("finished!".to_string()).unwrap();
            HandlerResult::Pending(Box::new(KarakuriAsyncTool { rx }))
        })
        .build();

    // Verify it compiles, registers, and dispatches
    let mut async_engine = DocEngine::new("test");
    async_engine.register_tool(async_tool).unwrap();

    let rpc_call = json!({
        "jsonrpc": "2.0",
        "id": 999,
        "method": "tools/call",
        "params": {
            "name": "async_op",
            "arguments": {}
        }
    })
    .to_string();

    match async_engine.handle(&rpc_call) {
        HandlerResult::Pending(mut p) => {
            let res = p.poll_settled().unwrap().unwrap();
            assert!(res.contains("finished!"));
        }
        _ => panic!("Expected Pending handler result"),
    }
}

#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
trait IsReadyHelper {
    fn is_ready(&self) -> bool;
}

#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
impl<T> IsReadyHelper for agent_man::mcp::HandlerResult<T> {
    fn is_ready(&self) -> bool {
        matches!(self, agent_man::mcp::HandlerResult::Ready(_))
    }
}

#[cfg(all(feature = "mcp-http", not(target_arch = "wasm32")))]
#[test]
fn test_http_delayed_request_50ms() {
    use agent_man::DocEngine;
    use serde_json::json;
    use std::io::{BufRead, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let engine = DocEngine::new("kinema");
    let server = engine.serve_http("127.0.0.1:0").unwrap();
    let addr = server.local_addr();

    // 1. Connect, wait 50ms before sending request, expect 200 OK
    let mut stream = TcpStream::connect(addr).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "ping"
    })
    .to_string();
    let req = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).unwrap();

    let mut reader = std::io::BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(
        status_line.contains("200 OK"),
        "Expected 200 OK after 50ms delay, got: {}",
        status_line
    );
}

#[cfg(all(feature = "mcp-http", not(target_arch = "wasm32")))]
#[test]
fn test_http_sequential_request_200ms() {
    use agent_man::DocEngine;
    use serde_json::json;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let engine = DocEngine::new("kinema");
    let server = engine.serve_http("127.0.0.1:0").unwrap();
    let addr = server.local_addr();

    // 2. Same connection: send request 1, wait 200ms, send request 2, expect 200 OK
    let mut stream = TcpStream::connect(addr).unwrap();
    let body1 = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "ping"
    })
    .to_string();
    let req1 = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
        addr,
        body1.len(),
        body1
    );
    stream.write_all(req1.as_bytes()).unwrap();

    let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
    let mut status1 = String::new();
    reader.read_line(&mut status1).unwrap();
    assert!(
        status1.contains("200 OK"),
        "Request 1 expected 200 OK, got: {}",
        status1
    );

    // Consume headers and body of response 1
    let mut content_len = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        if let Some(val) = line.to_lowercase().strip_prefix("content-length:") {
            content_len = val.trim().parse().unwrap_or(0);
        }
    }
    let mut body_buf = vec![0u8; content_len];
    reader.read_exact(&mut body_buf).unwrap();

    // Wait 200ms
    std::thread::sleep(Duration::from_millis(200));

    // Send request 2
    let body2 = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping"
    })
    .to_string();
    let req2 = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        body2.len(),
        body2
    );
    stream.write_all(req2.as_bytes()).unwrap();

    let mut status2 = String::new();
    reader.read_line(&mut status2).unwrap();
    assert!(
        status2.contains("200 OK"),
        "Request 2 expected 200 OK after 200ms gap, got: {}",
        status2
    );
}
