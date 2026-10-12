use crate::mcp::types::{HandlerResult, PendingResult, ToolResult};
use crate::DocEngine;
use serde_json::json;

pub fn handle_json_rpc(engine: &DocEngine, request_str: &str) -> HandlerResult<Option<String>> {
    let req: serde_json::Value = match serde_json::from_str(request_str) {
        Ok(v) => v,
        Err(_) => {
            let err_res = json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": {
                    "code": -32700,
                    "message": "Parse error"
                }
            });
            return HandlerResult::Ready(Some(err_res.to_string()));
        }
    };

    let id = match req.get("id") {
        Some(id_val) => id_val.clone(),
        None => {
            // Notification: must not send any response
            return HandlerResult::Ready(None);
        }
    };

    let method = match req.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => {
            let err = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "Invalid Request: missing method"
                }
            });
            return HandlerResult::Ready(Some(err.to_string()));
        }
    };

    match method {
        "initialize" => {
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {},
                        "resources": {}
                    },
                    "serverInfo": {
                        "name": engine.binary_name,
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            });
            HandlerResult::Ready(Some(res.to_string()))
        }
        "ping" => {
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            });
            HandlerResult::Ready(Some(res.to_string()))
        }
        "tools/list" => {
            let tools = engine.mcp_list_tools();
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tools
                }
            });
            HandlerResult::Ready(Some(res.to_string()))
        }
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let name = match params.get("name").and_then(|n| n.as_str()) {
                Some(n) => n,
                None => {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32602,
                            "message": "Invalid params: missing tool name"
                        }
                    });
                    return HandlerResult::Ready(Some(err.to_string()));
                }
            };
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let id_clone = id.clone();
            match engine.mcp_call_tool_handler(name, args) {
                HandlerResult::Ready(tool_res) => {
                    let res = json!({
                        "jsonrpc": "2.0",
                        "id": id_clone,
                        "result": tool_res
                    });
                    HandlerResult::Ready(Some(res.to_string()))
                }
                HandlerResult::Pending(p) => {
                    struct PendingRpcCall {
                        id: serde_json::Value,
                        inner: Box<dyn PendingResult<ToolResult>>,
                    }

                    impl PendingResult<Option<String>> for PendingRpcCall {
                        fn poll_settled(&mut self) -> Option<Option<String>> {
                            self.inner.poll_settled().map(|tool_res| {
                                let res = json!({
                                    "jsonrpc": "2.0",
                                    "id": self.id,
                                    "result": tool_res
                                });
                                Some(res.to_string())
                            })
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        fn settled(self: Box<Self>) -> Option<String> {
                            let tool_res = self.inner.settled();
                            let res = json!({
                                "jsonrpc": "2.0",
                                "id": self.id,
                                "result": tool_res
                            });
                            Some(res.to_string())
                        }
                    }

                    HandlerResult::Pending(Box::new(PendingRpcCall {
                        id: id_clone,
                        inner: p,
                    }))
                }
            }
        }
        "resources/list" => {
            let resources = engine.mcp_list_resources();
            let res = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "resources": resources
                }
            });
            HandlerResult::Ready(Some(res.to_string()))
        }
        "resources/read" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let uri = match params.get("uri").and_then(|u| u.as_str()) {
                Some(u) => u,
                None => {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32602,
                            "message": "Invalid params: missing resource uri"
                        }
                    });
                    return HandlerResult::Ready(Some(err.to_string()));
                }
            };

            match engine.mcp_read_resource(uri) {
                Ok(content) => {
                    let res = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "contents": [content]
                        }
                    });
                    HandlerResult::Ready(Some(res.to_string()))
                }
                Err(e) => {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32602,
                            "message": e.to_string()
                        }
                    });
                    HandlerResult::Ready(Some(err.to_string()))
                }
            }
        }
        _ => {
            let err = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: '{method}'")
                }
            });
            HandlerResult::Ready(Some(err.to_string()))
        }
    }
}
