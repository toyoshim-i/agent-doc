#[cfg(feature = "mcp")]
#[test]
fn test_b3_tools_registration_acceptance() {
    use agent_man::mcp::{ParamType, Tool};
    use agent_man::DocEngine;
    use serde_json::json;

    let mut engine = DocEngine::new("kinema").with_scheme("kinema");

    engine.add_page(
        "docs/rules/pin-count.md",
        r#"---
id: pin-count
title: Pin Count Rule
summary: Guidelines on footprint pin counts
category: rule
---
# Pin Count
Details.
"#,
    );

    // Register a tool with builder
    let tool = Tool::builder("configure_pin", "Configure pin properties")
        .param("pin_id", ParamType::String, "Pin identifier", true)
        .param(
            "mode",
            ParamType::Enum(vec!["input".into(), "output".into()]),
            "Pin direction",
            true,
        )
        .param(
            "pull",
            ParamType::Integer {
                min: Some(0),
                max: Some(2),
            },
            "Pull mode (0: none, 1: up, 2: down)",
            false,
        )
        .param("tags", ParamType::StringArray, "Optional tags", false)
        .see("pin-count")
        .example(json!({
            "pin_id": "P1",
            "mode": "input",
            "pull": 1
        }))
        .handler(|args| {
            let pin_id = args["pin_id"].as_str().unwrap_or("");
            agent_man::mcp::ToolResult::text(format!("Configured pin {pin_id}")).into()
        })
        .build();

    engine.register_tool(tool).unwrap();

    // 1. tools/list schema generation
    let tools = engine.mcp_list_tools();
    let configured_pin_tool = tools.iter().find(|t| t.name == "configure_pin").unwrap();
    assert!(configured_pin_tool
        .description
        .contains("Configure pin properties"));
    // Description references see page
    assert!(configured_pin_tool.description.contains("pin-count"));
    let schema = &configured_pin_tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["pin_id"]["type"], "string");
    assert_eq!(schema["properties"]["pull"]["minimum"], 0);
    assert_eq!(schema["properties"]["pull"]["maximum"], 2);

    // 2. Acceptance condition: valid args invocation succeeds
    let valid_call = engine
        .mcp_call_tool(
            "configure_pin",
            json!({ "pin_id": "P1", "mode": "input", "pull": 1 }),
        )
        .unwrap();
    assert!(!valid_call.is_error);
    assert!(valid_call.text_content().contains("Configured pin P1"));

    // 2b. Acceptance condition: misspelled/unknown optional argument is rejected
    let unknown_arg_call = engine
        .mcp_call_tool(
            "configure_pin",
            json!({ "pin_id": "P1", "mode": "input", "pulll": 1 }),
        )
        .unwrap();
    assert!(
        unknown_arg_call.is_error,
        "Unknown/misspelled argument must be rejected with isError"
    );
    let unknown_err_msg = unknown_arg_call.text_content();
    assert!(
        unknown_err_msg.contains("pulll"),
        "Must specify unknown argument: {}",
        unknown_err_msg
    );
    assert!(
        unknown_err_msg.contains("pin_id") && unknown_err_msg.contains("pull"),
        "Must list declared arguments: {}",
        unknown_err_msg
    );
    assert!(
        unknown_err_msg.contains("pin-count"),
        "Must reference .see page: {}",
        unknown_err_msg
    );

    // 3. Acceptance condition: validation error returns which arg failed and .see reference
    let invalid_call = engine
        .mcp_call_tool(
            "configure_pin",
            json!({ "pin_id": "P1", "mode": "invalid_mode" }),
        )
        .unwrap();
    assert!(invalid_call.is_error, "Should fail validation");
    let err_msg = invalid_call.text_content();
    assert!(
        err_msg.contains("mode"),
        "Must specify which arg failed: {}",
        err_msg
    );
    assert!(
        err_msg.contains("pin-count"),
        "Must point to .see page: {}",
        err_msg
    );

    // 4. Acceptance condition: check_tools detects broken .see references and invalid examples
    let tool_broken_see = Tool::builder("bad_tool", "Bad tool")
        .see("nonexistent-page-id")
        .handler(|_| agent_man::mcp::ToolResult::text("ok").into())
        .build();
    engine.register_tool(tool_broken_see).unwrap();

    let issues = engine.check_tools();
    assert!(!issues.is_empty(), "Must detect nonexistent .see reference");
    assert!(issues.iter().any(|i| i.contains("nonexistent-page-id")));

    // 5. Acceptance condition: ToolResult::error_structured serializes with both isError: true and structuredContent
    let err_structured = agent_man::mcp::ToolResult::error_structured(
        "Refused due to rule violation",
        json!({
            "code": "REFUSAL_UNWIRED",
            "message": "Unwired net found"
        }),
    );
    let serialized = serde_json::to_value(&err_structured).unwrap();
    assert_eq!(serialized["isError"], true);
    assert_eq!(serialized["structuredContent"]["code"], "REFUSAL_UNWIRED");
    assert_eq!(
        serialized["content"][0]["text"],
        "Refused due to rule violation"
    );

    // 6. Acceptance condition: registering 'doc_read' or a duplicate tool name is detected and rejected
    let reserved_tool = Tool::builder("doc_read", "Reserved tool name")
        .handler(|_| agent_man::mcp::ToolResult::text("ok").into())
        .build();
    let res_reserved = engine.register_tool(reserved_tool);
    assert!(
        res_reserved.is_err(),
        "Registering tool named 'doc_read' must be rejected"
    );

    let dup_tool = Tool::builder("configure_pin", "Duplicate tool name")
        .handler(|_| agent_man::mcp::ToolResult::text("ok").into())
        .build();
    let res_dup = engine.register_tool(dup_tool);
    assert!(
        res_dup.is_err(),
        "Registering duplicate tool name 'configure_pin' must be rejected"
    );
}
