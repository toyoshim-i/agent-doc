use agent_doc::DocEngine;

#[test]
fn test_a5_codeblocks_and_links_acceptance() {
    let mut engine = DocEngine::new("kinema");

    let doc_content = r#"---
id: intro
title: Introduction
category: guide
---
# Introduction

Here is some verilog code:
```verilog
module top;
endmodule
```

Check out [Working Link](guide.md) and [Broken Link](nonexistent.md).
Check internal anchor [Missing Section](#nosuchsection).
"#;

    let guide_content = r#"---
id: guide
title: Guide
category: guide
---
# Guide
Content here.
"#;

    engine.add_page("docs/intro.md", doc_content);
    engine.add_page("docs/guide.md", guide_content);

    // 1. Acceptance condition: code_blocks extracts verilog with correct 1-indexed line number
    // Line 1: ---
    // Line 2: id: intro
    // Line 3: title: Introduction
    // Line 4: category: guide
    // Line 5: ---
    // Line 6: # Introduction
    // Line 7:
    // Line 8: Here is some verilog code:
    // Line 9: ```verilog
    let blocks = engine.code_blocks("verilog");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].page_id, "intro");
    assert_eq!(blocks[0].path, "docs/intro.md");
    assert_eq!(blocks[0].line, 9, "Code block line must match line 9 in original file");
    assert_eq!(blocks[0].source.trim(), "module top;\nendmodule");

    // 2. Acceptance condition: broken_links detects broken relative links with correct line number
    // Line 14: Check out [Working Link](guide.md) and [Broken Link](nonexistent.md).
    let broken = engine.broken_links();
    let nonexistent = broken.iter().find(|b| b.target == "nonexistent.md");
    assert!(nonexistent.is_some(), "nonexistent.md must be detected as broken");
    let nb = nonexistent.unwrap();
    assert_eq!(nb.page_id, "intro");
    assert_eq!(nb.line, 14, "Broken link line must match line 14 in original file");
}
