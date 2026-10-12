use agent_man::{DocEngine, RenderMode};

#[test]
fn test_b1_render_mcp_acceptance() {
    let mut engine = DocEngine::new("kinema").with_scheme("kinema");

    engine.add_page(
        "docs/guides/principles.md",
        r#"---
id: principles
title: Principles
category: guide
---
# Principles
Check [Migration Guide](migration.md) and [Rule](rules/identity.md#problem).
"#,
    );

    engine.add_page(
        "docs/guides/migration.md",
        r#"---
id: migration
title: Migration
category: guide
---
# Migration
"#,
    );

    engine.add_page(
        "docs/rules/identity.md",
        r#"---
id: identity
title: Identity
category: rule
---
# Identity
## Problem
Problem description.
"#,
    );

    let rendered = engine.explain("principles", RenderMode::Mcp).unwrap();

    // Resource URI and doc_read tool call are present for link without anchor
    assert!(
        rendered.contains("resource: 'kinema://doc/migration'"),
        "Must contain resource URI: {}",
        rendered
    );
    assert!(
        rendered.contains("doc_read"),
        "Must contain doc_read call hint: {}",
        rendered
    );

    // Resource URI with anchor and section parameter
    assert!(
        rendered.contains("resource: 'kinema://doc/identity#problem'"),
        "Must contain resource URI with anchor: {}",
        rendered
    );
    assert!(
        rendered.contains("section"),
        "Must contain section parameter for anchor link: {}",
        rendered
    );

    // Verify H1 deduplication when body starts with H1
    let mcp_h1_count = rendered.matches("# Principles").count();
    assert_eq!(
        mcp_h1_count, 1,
        "RenderMode::Mcp must not duplicate H1 heading: {}",
        rendered
    );

    let agent_rendered = engine.explain("principles", RenderMode::Agent).unwrap();
    let agent_h1_count = agent_rendered.matches("# Principles").count();
    assert_eq!(
        agent_h1_count, 1,
        "RenderMode::Agent must not duplicate H1 heading: {}",
        agent_rendered
    );

    // Verify title is prepended when body does not start with H1
    engine.add_page(
        "docs/rules/no_h1.md",
        r#"---
id: no-h1
title: Without Heading
summary: A page without H1
category: rule
---
Body text without an H1 heading.
"#,
    );
    let rendered_no_h1_mcp = engine.explain("no-h1", RenderMode::Mcp).unwrap();
    assert!(
        rendered_no_h1_mcp.starts_with("# Without Heading"),
        "Must prepend title when body has no H1: {}",
        rendered_no_h1_mcp
    );
    let rendered_no_h1_agent = engine.explain("no-h1", RenderMode::Agent).unwrap();
    assert!(
        rendered_no_h1_agent.starts_with("# Without Heading"),
        "Must prepend title when body has no H1: {}",
        rendered_no_h1_agent
    );
}
