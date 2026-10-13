#[cfg(feature = "mcp")]
#[test]
fn test_image_content_serialization_and_methods() {
    use agent_man::mcp::{ToolContent, ToolResult};

    // 1. ToolContent::image strips "data:" URL prefix if present
    let img_raw = ToolContent::image("aGVsbG8=", "image/png");
    let img_with_prefix = ToolContent::image("data:image/png;base64,aGVsbG8=", "image/png");
    assert_eq!(img_raw, img_with_prefix);

    // 2. ToolContent as_text returns None for Image and Some for Text
    assert_eq!(img_raw.as_text(), None);
    let txt = ToolContent::text("hello");
    assert_eq!(txt.as_text(), Some("hello"));

    // 3. ToolContent serialization matches MCP specification
    let json_val = serde_json::to_value(&img_raw).unwrap();
    assert_eq!(json_val["type"], "image");
    assert_eq!(json_val["data"], "aGVsbG8=");
    assert_eq!(json_val["mimeType"], "image/png");

    // 4. ToolResult constructors and text_content()
    let res = ToolResult::text_and_image("Generated diagram", "aGVsbG8=", "image/png");
    assert_eq!(res.text_content(), "Generated diagram");
    assert_eq!(res.content.len(), 2);
    assert!(!res.is_error);

    let res_json = serde_json::to_value(&res).unwrap();
    assert_eq!(res_json["content"][0]["type"], "text");
    assert_eq!(res_json["content"][0]["text"], "Generated diagram");
    assert_eq!(res_json["content"][1]["type"], "image");
    assert_eq!(res_json["content"][1]["data"], "aGVsbG8=");
    assert_eq!(res_json["content"][1]["mimeType"], "image/png");

    // 5. Multiple text elements in text_content()
    let multi = ToolResult::new(vec![
        ToolContent::text("Line 1"),
        ToolContent::image("abc", "image/jpeg"),
        ToolContent::text("Line 2"),
    ]);
    assert_eq!(multi.text_content(), "Line 1\nLine 2");
}

#[cfg(feature = "mcp")]
#[test]
fn test_image_tool_execution_via_handle() {
    use agent_man::mcp::{HandlerResult, Tool, ToolResult};
    use agent_man::DocEngine;
    use serde_json::json;

    let mut engine = DocEngine::new("kinema");
    let render_tool = Tool::builder("render_circuit", "Render circuit diagram to PNG")
        .handler(|_| {
            ToolResult::text_and_image(
                "Circuit rendered successfully",
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
                "image/png",
            )
            .into()
        })
        .build();
    engine.register_tool(render_tool).unwrap();

    let rpc_call = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "render_circuit",
            "arguments": {}
        }
    })
    .to_string();

    let resp = match engine.handle(&rpc_call) {
        HandlerResult::Ready(Some(r)) => r,
        _ => panic!("Expected ready response"),
    };

    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["id"], 1);
    let contents = &v["result"]["content"];
    assert_eq!(contents.as_array().unwrap().len(), 2);
    assert_eq!(contents[0]["type"], "text");
    assert_eq!(contents[0]["text"], "Circuit rendered successfully");
    assert_eq!(contents[1]["type"], "image");
    assert_eq!(contents[1]["mimeType"], "image/png");
    assert!(contents[1]["data"].as_str().unwrap().starts_with("iVBORw"));
}

#[cfg(all(feature = "mcp-http", not(target_arch = "wasm32")))]
#[test]
fn test_large_image_over_http() {
    use agent_man::mcp::{Tool, ToolResult};
    use agent_man::DocEngine;
    use serde_json::json;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpStream;

    let mut engine = DocEngine::new("kinema");
    // Generate ~2MB base64 data
    let large_b64 = "A".repeat(2 * 1024 * 1024);
    let expected_len = large_b64.len();

    let large_tool = Tool::builder("get_large_render", "Return large 2MB image")
        .handler(move |_| ToolResult::image(&large_b64, "image/png").into())
        .build();
    engine.register_tool(large_tool).unwrap();

    let server = engine.serve_http("127.0.0.1:0").unwrap();
    let addr = server.local_addr();

    let mut stream = TcpStream::connect(addr).unwrap();
    let rpc_body = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": { "name": "get_large_render", "arguments": {} }
    })
    .to_string();

    let req = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        addr,
        rpc_body.len(),
        rpc_body
    );
    stream.write_all(req.as_bytes()).unwrap();

    let mut reader = std::io::BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(status_line.contains("200 OK"), "Got: {status_line}");

    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
        if let Some(val) = line.to_lowercase().strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap();
        }
    }
    assert!(content_length > expected_len);

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();

    let resp_val: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp_val["result"]["content"][0]["type"], "image");
    assert_eq!(
        resp_val["result"]["content"][0]["data"]
            .as_str()
            .unwrap()
            .len(),
        expected_len
    );
    assert_eq!(resp_val["result"]["content"][0]["mimeType"], "image/png");
}
