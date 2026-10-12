#[cfg(feature = "mcp")]
#[test]
fn test_b2_mcp_docs_acceptance() {
    use agent_man::DocEngine;
    use serde_json::json;

    let mut engine = DocEngine::new("kinema").with_scheme("kinema");

    engine.add_page(
        "docs/guides/principles.md",
        r#"---
id: principles
title: Core Design Principles
summary: Essential principles
category: guide
---
# Principles

Intro text.

## Rule One
Details for rule one.

## Rule Two
Details for rule two.
"#,
    );

    // 1. resources/list
    let resources = engine.mcp_list_resources();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "kinema://doc/principles");
    assert_eq!(resources[0].name, "Core Design Principles");
    assert_eq!(resources[0].description, "Essential principles");
    assert_eq!(resources[0].mime_type, "text/markdown");

    // 2. resources/read
    let read_res = engine.mcp_read_resource("kinema://doc/principles").unwrap();
    assert_eq!(read_res.uri, "kinema://doc/principles");
    assert_eq!(read_res.mime_type, "text/markdown");
    assert!(read_res.text.contains("# Core Design Principles"));

    // 3. Built-in tool: doc_list
    let doc_list_res = engine.mcp_call_tool("doc_list", json!({})).unwrap();
    assert!(!doc_list_res.is_error);
    let list_text = &doc_list_res.content[0].text;
    assert!(list_text.contains("principles"));
    assert!(list_text.contains("Core Design Principles"));

    // 4. Built-in tool: doc_read (whole page)
    let doc_read_all = engine
        .mcp_call_tool("doc_read", json!({ "id": "principles" }))
        .unwrap();
    assert!(!doc_read_all.is_error);
    assert!(doc_read_all.content[0].text.contains("Rule One"));
    assert!(doc_read_all.content[0].text.contains("Rule Two"));

    // 5. Built-in tool: doc_read (single section by anchor)
    let doc_read_section = engine
        .mcp_call_tool(
            "doc_read",
            json!({ "id": "principles", "section": "rule-one" }),
        )
        .unwrap();
    assert!(!doc_read_section.is_error);
    assert!(doc_read_section.content[0].text.contains("Rule One"));
    assert!(
        !doc_read_section.content[0].text.contains("Rule Two"),
        "Should only return requested section"
    );

    // 6. Built-in tool: doc_read with missing id or missing section returns isError
    let missing_id = engine
        .mcp_call_tool("doc_read", json!({ "id": "nonexistent" }))
        .unwrap();
    assert!(missing_id.is_error);

    let missing_section = engine
        .mcp_call_tool(
            "doc_read",
            json!({ "id": "principles", "section": "no-such-section" }),
        )
        .unwrap();
    assert!(missing_section.is_error);
}
