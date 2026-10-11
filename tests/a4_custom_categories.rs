use agent_doc::{CategoryConfig, DocEngine, RenderMode};

#[test]
fn test_a4_custom_categories_acceptance() {
    let mut engine = DocEngine::new("mytool");

    engine.add_page(
        "docs/g.md",
        r#"---
id: g1
title: Guide One
summary: First guide
category: guide
---
# Guide
"#,
    );

    engine.add_page(
        "docs/r.md",
        r#"---
id: R001
title: Rule One
summary: First rule
category: rule
---
# Rule
"#,
    );

    // 1. Default categories order: Guides first, Rules second
    let default_overview = engine.guide(None, RenderMode::Agent).unwrap();
    let guide_pos = default_overview.find("## Guides & Best Practices").unwrap();
    let rule_pos = default_overview.find("## Diagnostics & Rules").unwrap();
    assert!(guide_pos < rule_pos);

    // 2. Custom categories: Rule first, custom heading
    engine.set_categories(vec![
        CategoryConfig::new("rule", "Custom Diagnostics").with_id_as_title(true),
        CategoryConfig::new("guide", "Custom Tutorials"),
    ]);

    let custom_overview = engine.guide(None, RenderMode::Agent).unwrap();
    let custom_rule_pos = custom_overview.find("## Custom Diagnostics").unwrap();
    let custom_guide_pos = custom_overview.find("## Custom Tutorials").unwrap();
    assert!(custom_rule_pos < custom_guide_pos);

    let summary = engine.summary_md();
    let summary_rule_pos = summary.find("# Custom Diagnostics").unwrap();
    let summary_guide_pos = summary.find("# Custom Tutorials").unwrap();
    assert!(summary_rule_pos < summary_guide_pos);
}
