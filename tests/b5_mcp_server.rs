#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
#[test]
fn test_b5_mcp_server_acceptance() {
    use agent_doc::mcp::{HandlerResult, Tool, ToolResult};
    use agent_doc::DocEngine;
    use serde_json::json;
    use std::io::{Read, Write};
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
    engine.register_tool(hello_tool);

    // 1. Direct invocation via handle(): initialize, ping, tools/list, tools/call
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }).to_string();

    match engine.handle(&init_req) {
        HandlerResult::Ready(Some(res)) => {
            let v: serde_json::Value = serde_json::from_str(&res).unwrap();
            assert_eq!(v["id"], 1);
            assert_eq!(v["result"]["serverInfo"]["name"], "kinema");
        }
        _ => panic!("Expected Ready response for initialize"),
    }

    // Notification (no id) produces None
    let notif_req = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }).to_string();
    match engine.handle(&notif_req) {
        HandlerResult::Ready(None) => {}
        other => panic!("Expected Ready(None) for notification, got {:?}", other.is_ready()),
    }

    // Unknown method produces -32601
    let unknown_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "unknown_method"
    }).to_string();
    match engine.handle(&unknown_req) {
        HandlerResult::Ready(Some(res)) => {
            let v: serde_json::Value = serde_json::from_str(&res).unwrap();
            assert_eq!(v["error"]["code"], -32601);
        }
        _ => panic!("Expected error response"),
    }

    // 2. HTTP Server acceptance test on loopback port
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let engine_clone = engine.clone();
    let _server_handle = std::thread::spawn(move || {
        engine_clone.serve_http(addr).unwrap();
    });

    // Wait briefly for server startup
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Connect via HTTP POST
    let mut stream = TcpStream::connect(addr).unwrap();
    let post_body = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "hello",
            "arguments": {}
        }
    }).to_string();

    let request = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nOrigin: http://localhost:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        addr.port(),
        post_body.len(),
        post_body
    );

    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();

    assert!(response.contains("200 OK"), "Response: {}", response);
    assert!(response.contains("world"), "Response: {}", response);

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
    assert!(call_res["result"]["content"][0]["text"].as_str().unwrap().contains("world"));
}

#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
trait IsReadyHelper {
    fn is_ready(&self) -> bool;
}

#[cfg(all(feature = "mcp", feature = "mcp-stdio", feature = "mcp-http"))]
impl<T> IsReadyHelper for agent_doc::mcp::HandlerResult<T> {
    fn is_ready(&self) -> bool {
        matches!(self, agent_doc::mcp::HandlerResult::Ready(_))
    }
}
