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
}
