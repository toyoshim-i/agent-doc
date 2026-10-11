#[cfg(feature = "build")]
#[test]
fn test_a1_embed_directory_acceptance() {
    use std::fs;

    let temp_dir = std::env::temp_dir().join("agent_doc_test_a1");
    let docs_dir = temp_dir.join("docs");
    let out_dir = temp_dir.join("out");

    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&docs_dir).unwrap();
    fs::create_dir_all(&out_dir).unwrap();

    // 1. Initial page
    let page1_path = docs_dir.join("guide.md");
    fs::write(
        &page1_path,
        r#"---
id: guide
title: User Guide
summary: General user guide
category: guide
---
# User Guide
Welcome.
"#,
    ).unwrap();

    // Run build embed helper with custom OUT_DIR
    std::env::set_var("OUT_DIR", &out_dir);
    agent_doc::build::embed(&docs_dir).unwrap();

    let output_file = out_dir.join("agent_doc_pages.rs");
    assert!(output_file.exists(), "Embedded source file must be generated");

    // In a real build.rs, the generated file produces a &[(&str, &str)] slice expression.
    // For this dynamic test, we verify the generated code content and also test adding a second file.
    let code_v1 = fs::read_to_string(&output_file).unwrap();
    assert!(code_v1.contains("guide.md"));
    assert!(code_v1.contains("include_str!"));

    // 2. Add a second page without modifying host code
    let page2_path = docs_dir.join("rule.md");
    fs::write(
        &page2_path,
        r#"---
id: E001
title: Error 001
summary: First error rule
category: rule
---
# E001
Details of E001.
"#,
    ).unwrap();

    // Re-run embed
    agent_doc::build::embed(&docs_dir).unwrap();

    let code_v2 = fs::read_to_string(&output_file).unwrap();
    assert!(code_v2.contains("guide.md"));
    assert!(code_v2.contains("rule.md"));

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}
