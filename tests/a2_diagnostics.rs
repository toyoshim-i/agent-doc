use agent_doc::{DocEngine, DocError, RenderMode};

#[test]
fn test_a2_diagnostics_acceptance() {
    let mut engine = DocEngine::new("kinema");

    engine.add_page(
        "docs/rules/E001.md",
        r#"---
id: E001
title: Unused Pin
summary: Pin is unconnected
category: rule
aliases: ["unused-pin", "pin-unused"]
---
# E001: Unused Pin
Fix the pin connection.
"#,
    );

    engine.add_page(
        "docs/rules/E002.md",
        r#"---
id: E002
title: Short Circuit
summary: VCC connected to GND
category: rule
---
# E002: Short Circuit
Separate nets.
"#,
    );

    // 1. engine.hint() returns helpful command / tool guidance for existing code/alias
    let hint_agent = engine.hint("E001", RenderMode::Agent);
    assert_eq!(
        hint_agent,
        Some("Help: run 'kinema explain E001'".to_string())
    );

    let hint_terminal = engine.hint("E001", RenderMode::Terminal);
    assert_eq!(
        hint_terminal,
        Some("Help: run 'kinema explain E001'".to_string())
    );

    let hint_mcp = engine.hint("E001", RenderMode::Mcp);
    assert_eq!(
        hint_mcp,
        Some("Help: call tool 'doc_read {\"id\": \"E001\"}'".to_string())
    );

    let hint_alias = engine.hint("unused-pin", RenderMode::Agent);
    assert_eq!(
        hint_alias,
        Some("Help: run 'kinema explain E001'".to_string())
    );

    let hint_alias_mcp = engine.hint("unused-pin", RenderMode::Mcp);
    assert_eq!(
        hint_alias_mcp,
        Some("Help: call tool 'doc_read {\"id\": \"E001\"}'".to_string())
    );

    let hint_json = engine.hint("E001", RenderMode::Json);
    assert_eq!(hint_json, Some("doc_read {\"id\": \"E001\"}".to_string()));

    // Missing code returns None
    assert_eq!(engine.hint("E999", RenderMode::Mcp), None);

    // 2. Acceptance condition: Non-existing code with partial match in another page
    // E001 matches "E00", but explain for "E00" or "E0" should return NotFound, not E001!
    let explain_partial = engine.explain("E00", RenderMode::Agent);
    match explain_partial {
        Err(DocError::NotFound(missing)) => assert_eq!(missing, "E00"),
        other => panic!("Expected NotFound(\"E00\"), got: {:?}", other),
    }

    // 3. engine.missing_pages()
    let reported = engine.missing_pages(["E001", "E002", "E003", "E004"]);
    assert_eq!(reported, vec!["E003".to_string(), "E004".to_string()]);

    // 4. guide() topic search with partial match and multiple candidates
    let guide_multiple = engine.guide(Some("pin"), RenderMode::Agent).unwrap();
    // Partial match on "pin" matches E001, but if multiple topics match, returns list of candidates
    assert!(guide_multiple.contains("E001"));
}
