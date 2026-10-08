# agent-doc

> **Context-engineering-free, agent-native documentation and explain engine for command-line tools**

[![CI](https://github.com/toyoshim-i/agent-doc/actions/workflows/ci.yml/badge.svg)](https://github.com/toyoshim-i/agent-doc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/agent-doc.svg)](https://crates.io/crates/agent-doc)
[![Docs.rs](https://docs.rs/agent-doc/badge.svg)](https://docs.rs/agent-doc)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

`agent-doc` is an embedded documentation engine for Rust CLI tools designed for the agentic coding era. It turns standard Markdown documentation hierarchies into an on-demand, self-documenting CLI knowledge system.

---

## The Philosophy: Context-Engineering-Free

Dumping massive prompt files or skill definitions into LLM contexts causes prompt bloat, high token costs, and instruction degradation. 

`agent-doc` enables **context-engineering-free** design:
1. **Self-Evident Discovery**: CLI `--help` points agents directly to your in-binary guide (`mycli guide`).
2. **Just-in-Time Knowledge**: Agents retrieve specific topics (`mycli guide topic`) only when relevant, keeping working contexts clean.
3. **Automated Diagnostic Remediation**: Diagnostics emit self-documenting hints (`Help: run 'mycli explain <code>'`). Agents run the command to receive actionable root-cause analysis and code fixes.

---

## Features

- **Dynamic Link Rewriter**: Markdown relative links (`[Guide](../guides/other.md)`) are dynamically rewritten at runtime into executable CLI commands (`(see: 'mycli guide other')`), interactive OSC 8 terminal hyperlinks, or clean markdown.
- **Triple Output Modes**:
  - `RenderMode::Agent`: Clean plain-text with actionable CLI invocation hints, stripped of terminal ANSI escape sequences.
  - `RenderMode::Terminal`: Bold styling and clickable OSC 8 terminal hyperlinks.
  - `RenderMode::Json`: Structured JSON for programmatic consumption.
- **Frontmatter Metadata Parsing**: Extracts `id`, `title`, `summary`, `category`, and `aliases` from standard YAML frontmatter without external regex dependencies.
- **Zero Heavy Dependencies**: Depends only on `serde`, `serde_json`, and `thiserror`. Compiles in under 0.5s.

---

## Quick Start

Add `agent-doc` to your `Cargo.toml`:

```toml
[dependencies]
agent-doc = "0.1"
```

### Basic Usage

```rust
use agent_doc::{DocEngine, RenderMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = DocEngine::new("mycli")
        .with_subcommands("guide", "explain");

    // Embed markdown files at compile-time or load at runtime
    engine.add_page("guides/intro.md", r#"---
id: intro
title: Introduction Guide
summary: Getting started with mycli.
---

# Introduction
See [Rule: syntax-error](../rules/syntax-error.md) for error troubleshooting.
"#);

    engine.add_page("rules/syntax-error.md", r#"---
id: syntax-error
title: Invalid Syntax
category: rule
summary: Syntax validation failure.
---

# Syntax Error
Refer back to the [Intro Guide](../guides/intro.md).
"#);

    // 1. Render documentation index overview
    println!("{}", engine.guide(None, RenderMode::Terminal)?);

    // 2. Render topic with links dynamically rewritten to CLI commands
    // In Agent mode, the internal link to intro.md becomes:
    // "Intro Guide (see: 'mycli guide intro')"
    println!("{}", engine.explain("syntax-error", RenderMode::Agent)?);

    Ok(())
}
```

---

## mdBook Compatibility

If your project also builds a web manual using [mdBook](https://rust-lang.github.io/mdBook/), the included [`scripts/strip_frontmatter.py`](scripts/strip_frontmatter.py) preprocessor strips leading YAML frontmatter so the exact same Markdown files render cleanly on GitHub Pages.

---

## License

MIT OR Apache-2.0
