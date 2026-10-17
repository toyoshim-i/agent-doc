use agent_man::workspace::{
    preprocess_mdbook_json, AccessMode, CreatePageRequest, Workspace, WorkspaceError,
};
use agent_man::RenderMode;
use std::fs;
use std::path::Path;

#[test]
fn test_repo_docs_workspace_is_valid_and_in_sync() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let docs_root = manifest_dir.join("docs");

    let ws = Workspace::load(&docs_root, "agent-man-cli --root docs")
        .expect("Failed to load repository docs/ workspace");

    let lint = ws.lint();
    assert!(
        lint.is_clean(),
        "Repository docs/ failed workspace lint:\n{}",
        lint.format_report("agent-man-cli --root docs")
    );

    let summary_path = docs_root.join("SUMMARY.md");
    agent_man::assert_or_bless(&summary_path, &ws.summary_md())
        .expect("docs/SUMMARY.md is out of sync with docs/agent-man.json");

    // Overview should show pinned overview, normal guides, and fold record category
    let overview = ws.overview(RenderMode::Agent);
    assert!(overview.contains("## Entrypoints (Must Read) [Must Read]"));
    assert!(overview.contains("**overview** [readonly]"));
    assert!(overview.contains("**cli-workspace** [readwrite]"));
    assert!(overview.contains("## Records & History (1 entries folded)"));
    assert!(overview.contains("agent-man-cli --root docs guide overview"));

    // Reading 'embedding' should show bidirectional backlinks
    let embedding_page = ws
        .read_page("embedding", None, RenderMode::Agent)
        .expect("read embedding page");
    assert!(embedding_page.contains("Referenced by (Backlinks):"));
    assert!(embedding_page.contains("**overview**"));
    assert!(embedding_page.contains("**cli-workspace**"));
}

#[test]
fn test_workspace_init_and_partial_editing_lifecycle() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "agent_man_ws_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(tmp_dir.join("guides")).unwrap();
    fs::create_dir_all(tmp_dir.join("records")).unwrap();

    fs::write(
        tmp_dir.join("overview.md"),
        "# System Overview\n\nCore architecture.\nSee [Setup Guide](guides/setup.md) and [Log 01](records/log01.md).\n",
    )
    .unwrap();
    fs::write(
        tmp_dir.join("guides/setup.md"),
        "# Setup Guide\n\nLine 3 content.\nLine 4 content.\nLine 5 content.\n",
    )
    .unwrap();
    fs::write(
        tmp_dir.join("records/log01.md"),
        "# Log 01\n\nHistorical note.\n",
    )
    .unwrap();

    // 1. Initialize workspace
    let init_report = Workspace::init(&tmp_dir, Some("Test Docs"), Some("Test description"))
        .expect("Workspace::init");
    assert_eq!(init_report.total_pages, 3);

    let prefix = format!("agent-man-cli --root {}", tmp_dir.display());
    let mut ws = Workspace::load(&tmp_dir, &prefix).expect("Workspace::load");
    ws.write_summary_md().unwrap();

    // Verify inferred access modes
    assert_eq!(
        ws.page_meta("overview").unwrap().access,
        AccessMode::ReadOnly
    );
    assert_eq!(ws.page_meta("setup").unwrap().access, AccessMode::ReadWrite);

    // 2. Readonly enforcement: editing or removing 'overview' must fail
    let ro_err = ws.insert("overview", Some(1), "Hack").unwrap_err();
    assert!(matches!(ro_err, WorkspaceError::ReadOnly(_)));
    let ro_meta_err = ws
        .update_meta("overview", Some("New".into()), None, None, None)
        .unwrap_err();
    assert!(matches!(ro_meta_err, WorkspaceError::ReadOnly(_)));
    let ro_rm_err = ws.remove_page("overview", true).unwrap_err();
    assert!(matches!(ro_rm_err, WorkspaceError::ReadOnly(_)));

    // 3. Inspect raw lines on 'setup'
    let lines_out = ws.lines("setup", Some(1), Some(5)).unwrap();
    assert!(lines_out.contains("   1 | # Setup Guide"));
    assert!(lines_out.contains("   3 | Line 3 content."));

    // 4. Partial edit: replace lines 3..=4
    let rep_feedback = ws
        .replace(
            "setup",
            3,
            4,
            "Replaced line A.\nReplaced line B.\nReplaced line C.",
        )
        .unwrap();
    assert!(rep_feedback.contains("Replaced lines 3..=4 with 3 line(s) (lines: 5 -> 6)"));
    assert!(rep_feedback.contains("   3 | Replaced line A."));
    assert!(rep_feedback.contains("   5 | Replaced line C."));

    // 5. Partial edit: insert at EOF (line: None) with a broken link to trigger affordance warning
    let ins_feedback = ws
        .insert("setup", None, "See [Broken](missing-target.md).")
        .unwrap();
    assert!(ins_feedback.contains("Affordance Warning — Broken links detected"));
    assert!(ins_feedback.contains("missing-target.md"));

    // 6. Partial edit: delete the broken link line (line 7)
    let del_feedback = ws.delete_lines("setup", 7, 7).unwrap();
    assert!(!del_feedback.contains("Broken links detected"));
    assert!(ws.lint().is_clean());

    // 7. Create a new page and check orphan hint
    let create_feedback = ws
        .create_page(CreatePageRequest {
            id: "new-topic".into(),
            title: "New Topic".into(),
            summary: "Newly created topic".into(),
            category: Some("guide".into()),
            path: None,
            aliases: vec!["nt".into()],
            content: "Body linking to [Overview](../overview.md).".into(),
        })
        .unwrap();
    assert!(create_feedback.contains("Affordance Hint: Page 'new-topic' has no incoming links"));

    // SUMMARY.md is automatically synced with new-topic
    let summary_text = fs::read_to_string(tmp_dir.join("SUMMARY.md")).unwrap();
    assert!(summary_text.contains("[New Topic](guides/new-topic.md)"));

    // 8. Backlink protection on remove: 'setup' is referenced by 'overview'
    let rm_blocked = ws.remove_page("setup", false).unwrap_err();
    assert!(matches!(rm_blocked, WorkspaceError::HasBacklinks(_, _)));

    // Unreferenced 'new-topic' can be removed without force
    ws.remove_page("new-topic", false).unwrap();
    assert!(ws.page_meta("new-topic").is_none());

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[cfg(feature = "mcp")]
#[test]
fn test_workspace_mcp_editing_proxy_tools() {
    use agent_man::workspace::build_workspace_mcp_engine;
    use serde_json::json;

    let tmp_dir = std::env::temp_dir().join(format!(
        "agent_man_mcp_ws_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    fs::write(
        tmp_dir.join("overview.md"),
        "# Overview\n\nEntrypoint doc.\n",
    )
    .unwrap();
    Workspace::init(&tmp_dir, Some("MCP Docs"), None).unwrap();

    let engine = build_workspace_mcp_engine(&tmp_dir, "agent-man-cli --root tmp").unwrap();

    // 1. Create a new page via MCP doc_create
    let create_res = engine
        .mcp_call_tool(
            "doc_create",
            json!({
                "id": "api-guide",
                "title": "API Guide",
                "summary": "REST and RPC endpoints",
                "category": "guide",
                "content": "Initial line.\nSecond line."
            }),
        )
        .unwrap();
    assert!(!create_res.is_error, "{}", create_res.text_content());

    // 2. Immediately read via MCP doc_read (dynamic reload)
    let read_res = engine
        .mcp_call_tool("doc_read", json!({ "id": "api-guide" }))
        .unwrap();
    assert!(!read_res.is_error);
    assert!(read_res.text_content().contains("Initial line."));

    // 3. Inspect lines via MCP doc_lines
    let lines_res = engine
        .mcp_call_tool("doc_lines", json!({ "id": "api-guide" }))
        .unwrap();
    assert!(!lines_res.is_error);
    assert!(lines_res.text_content().contains("   1 | # API Guide"));

    // 4. Replace line 3 via MCP doc_replace
    let replace_res = engine
        .mcp_call_tool(
            "doc_replace",
            json!({
                "id": "api-guide",
                "start_line": 3,
                "end_line": 3,
                "content": "Updated first paragraph."
            }),
        )
        .unwrap();
    assert!(!replace_res.is_error);
    assert!(replace_res
        .text_content()
        .contains("Updated first paragraph."));

    // 5. Verify readonly page cannot be edited via MCP
    let ro_res = engine
        .mcp_call_tool(
            "doc_insert",
            json!({
                "id": "overview",
                "line": 1,
                "content": "Should be rejected"
            }),
        )
        .unwrap();
    assert!(ro_res.is_error);
    assert!(ro_res.text_content().contains("readonly"));

    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_mdbook_preprocessor_strips_frontmatter() {
    let input = r#"[
        {},
        {
            "sections": [
                {
                    "Chapter": {
                        "name": "Intro",
                        "content": "---\nid: intro\ntitle: Intro\n---\n# Intro\n\nHello world.\n",
                        "sub_items": []
                    }
                }
            ]
        }
    ]"#;

    let output = preprocess_mdbook_json(input).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let chapter_content = parsed["sections"][0]["Chapter"]["content"]
        .as_str()
        .unwrap();
    assert_eq!(chapter_content, "# Intro\n\nHello world.\n");
}
