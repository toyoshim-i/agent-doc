use agent_man::DocEngine;

#[test]
fn test_a3_static_entrypoints_acceptance() {
    let mut engine = DocEngine::new("kinema");

    engine.add_page(
        "docs/README.md",
        r#"---
id: readme
title: Introduction
summary: Overview of kinema
category: intro
---
# Introduction
Welcome to kinema.
"#,
    );

    engine.add_page(
        "docs/guides/migration.md",
        r#"---
id: migration
title: KiCad Project Migration
summary: Migrate KiCad project to kinema
category: getting-started
---
# Migration
Migration guide.
"#,
    );

    engine.add_page(
        "docs/guides/principles.md",
        r#"---
id: principles
title: Core Design Principles
summary: Core design principles for hardware
category: guide
---
# Principles
Principles guide.
"#,
    );

    engine.add_page(
        "docs/rules/identity-missing.md",
        r#"---
id: identity-missing
title: "Rule: identity-missing"
summary: PCB footprint timestamp mismatch
category: rule
---
# Rule
Identity missing rule.
"#,
    );

    // Set host custom categories (kinema uses "Design Guides & Best Practices")
    engine.set_categories(vec![
        agent_man::CategoryConfig::new("getting-started", "Getting Started & Migration")
            .with_aliases(["start", "migration"]),
        agent_man::CategoryConfig::new("guide", "Design Guides & Best Practices")
            .with_aliases(["guides", "best-practices"]),
        agent_man::CategoryConfig::new("rule", "Diagnostics & Rules")
            .with_aliases(["rules", "diagnostic", "diagnostics"])
            .with_id_as_title(true),
    ]);

    // 1. Acceptance condition: summary_md() matches kinema docs/SUMMARY.md structure
    let summary = engine.summary_md();
    let expected_summary = r#"# Summary

[Introduction](README.md)

# Getting Started & Migration
- [KiCad Project Migration](guides/migration.md)

# Design Guides & Best Practices
- [Core Design Principles](guides/principles.md)

# Diagnostics & Rules
- [identity-missing](rules/identity-missing.md)
"#;
    assert_eq!(summary.trim(), expected_summary.trim());

    // 2. skill_md(name, description)
    let skill = engine.skill_md(
        "kinema-pcb-design",
        "Design, modify, route, and verify printed circuit boards (PCBs) in KiCad using kinema.",
    );
    assert!(skill.contains("name: kinema-pcb-design"));
    assert!(skill.contains("kinema guide"));
    assert!(skill.contains("principles"));

    // 3. llms_txt()
    let llms = engine.llms_txt();
    assert!(llms.contains("# kinema"));
    assert!(llms.contains("KiCad Project Migration"));

    // 4. Test helper assert_or_bless
    let temp_file = std::env::temp_dir().join("agent_man_bless_test.md");
    std::fs::write(&temp_file, "old content").unwrap();

    // Without AGENT_MAN_BLESS, mismatched content fails
    std::env::remove_var("AGENT_MAN_BLESS");
    std::env::remove_var("AGENT_DOC_BLESS");
    let res = engine.assert_or_bless(&temp_file, "new content");
    assert!(res.is_err());

    // With AGENT_MAN_BLESS=1, it updates file
    std::env::set_var("AGENT_MAN_BLESS", "1");
    let res = engine.assert_or_bless(&temp_file, "new content");
    assert!(res.is_ok());
    assert_eq!(std::fs::read_to_string(&temp_file).unwrap(), "new content");
    let _ = std::fs::remove_file(temp_file);
}
