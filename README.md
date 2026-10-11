# agent-doc

> **Context-engineering-free, agent-native documentation and MCP integration engine**

[![CI](https://github.com/toyoshim-i/agent-doc/actions/workflows/ci.yml/badge.svg)](https://github.com/toyoshim-i/agent-doc/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/agent-doc.svg)](https://crates.io/crates/agent-doc)
[![Docs.rs](https://docs.rs/agent-doc/badge.svg)](https://docs.rs/agent-doc)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

`agent-doc` is an embedded documentation and Model Context Protocol (MCP) engine for Rust CLI tools, agent frameworks, and web runtimes. It turns a single source of Markdown documentation into an on-demand CLI knowledge system, static entry points (`SUMMARY.md`, `SKILL.md`, `llms.txt`), and production MCP servers.

---

## The Philosophy: Context-Engineering-Free

Dumping massive prompt files or skill definitions into LLM contexts causes prompt bloat, high token costs, and instruction degradation. 

`agent-doc` enables **context-engineering-free** design:
1. **Self-Evident Discovery**: CLI `--help` points agents directly to your in-binary guide (`mycli guide`).
2. **Just-in-Time Knowledge**: Agents retrieve specific topics (`mycli guide <topic>`) only when relevant, keeping working contexts clean.
3. **Automated Diagnostic Remediation**: Diagnostics emit self-documenting hints (`Help: run 'mycli explain <code>'`). Agents run the command to receive actionable root-cause analysis and code fixes.
4. **Unified MCP & CLI**: Tools and documentation are served over standard MCP (stdio, HTTP, and in-process) with identical input schemas and validation.

---

## Features

- **Compile-Time Embedding**: Automatically scans docs directories in `build.rs` with `cargo:rerun-if-changed` tracking.
- **Dynamic Link Rewriter**: Markdown relative links (`[Guide](../guides/other.md)`) are dynamically rewritten at runtime into executable CLI commands (`(see: 'mycli guide other')`), interactive OSC 8 terminal hyperlinks, or MCP tool invocations (`mycli://doc/other`, `doc_read`).
- **Render Modes**:
  - `RenderMode::Agent`: Clean plain-text with actionable CLI invocation hints, stripped of ANSI escape sequences.
  - `RenderMode::Terminal`: Bold styling and clickable OSC 8 terminal hyperlinks.
  - `RenderMode::Mcp`: Rewrites internal links to MCP resource URIs and `doc_read` tool calls.
  - `RenderMode::Json`: Structured JSON for programmatic consumption.
- **Static Artifact Generation & Blessing**:
  - Generate mdBook `SUMMARY.md`, Agent `SKILL.md`, and LLM crawler `llms.txt`.
  - Verify freshness in tests or update with `DocEngine::assert_or_bless()`.
- **First-Class MCP Integration**:
  - Expose documentation via `resources/list`, `resources/read`, and built-in `doc_list` / `doc_read` tools.
  - Register custom tools with typed schemas (`ToolBuilder`) or directly from `clap::Command`.
  - Run MCP servers via `serve_stdio()`, `serve_http(addr)`, or zero-I/O `handle(request)`.
  - Non-blocking async / pending execution model compatible with native threads and `wasm32-unknown-unknown` (WebMCP / Karakuri).
- **Documentation Linting**:
  - Extract fenced code blocks with 1-indexed source lines for compiler checking (`engine.code_blocks("rust")`).
  - Detect dead relative links and missing anchors (`engine.broken_links()`).
  - Validate tool schema examples and `.see(doc_id)` cross-references (`engine.check_tools()`).

---

## Cargo Features

| Feature | Description | Dependencies |
| :--- | :--- | :--- |
| *default* | Core parser, index, link rewriter, and static entry points | `serde`, `serde_json`, `thiserror` |
| `build` | Compile-time directory scanner and embedding helper (`agent_doc::build::embed`) | None |
| `mcp` | MCP types, tool builder, JSON Schema generator, and `DocEngine::handle()` | None |
| `mcp-stdio` | MCP standard input/output transport (`serve_stdio()`) | `mcp` |
| `mcp-http` | MCP HTTP POST loopback transport (`serve_http()`) using `std::net` | `mcp` |
| `clap` | Auto-generate MCP tools from `clap::Command` subcommands | `mcp`, `clap` |

---

## Minimal Example: Complete Integration

### 1. Compile-Time Embedding (`build.rs`)

```rust
// build.rs
fn main() {
    #[cfg(feature = "build")]
    agent_doc::build::embed("docs", "agent_doc_pages.rs");
}
```

### 2. Host Integration (`src/main.rs`)

```rust
use agent_doc::{CategoryConfig, DocEngine, RenderMode};

fn create_engine() -> DocEngine {
    let mut engine = DocEngine::new("kinema")
        .with_subcommands("guide", "explain")
        .with_scheme("kinema")
        .with_categories(vec![
            CategoryConfig::new("guide", "Guides"),
            CategoryConfig::new("rule", "Rules & Diagnostics"),
        ]);

    // Include the generated pages from build.rs
    // include!(concat!(env!("OUT_DIR"), "/agent_doc_pages.rs"));
    engine.add_page("docs/intro.md", r#"---
id: intro
title: Intro Guide
summary: Overview of kinema.
category: guide
---
# Introduction
Welcome to kinema. See [Syntax Error](rules/E001.md).
"#);

    engine.add_page("docs/rules/E001.md", r#"---
id: E001
title: Syntax Error
summary: Invalid syntax encountered.
category: rule
---
# Syntax Error E001
Check the [Intro Guide](../intro.md).
"#);

    engine
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = create_engine();

    // 1. CLI Guide and Diagnostics Explain
    println!("{}", engine.guide(None, RenderMode::Terminal)?);
    println!("{}", engine.explain("E001", RenderMode::Agent)?);
    println!("{}", engine.hint("E001")); // "Help: run 'kinema explain E001'"

    // 2. MCP Server (stdio or HTTP)
    #[cfg(feature = "mcp-stdio")]
    if std::env::args().any(|a| a == "--mcp") {
        return engine.serve_stdio().map_err(Into::into);
    }

    #[cfg(feature = "mcp-http")]
    if std::env::args().any(|a| a == "--mcp-http") {
        return engine.serve_http("127.0.0.1:8080").map_err(Into::into);
    }

    // 3. Direct JSON-RPC Dispatch (In-Process / WebMCP)
    #[cfg(feature = "mcp")]
    {
        let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        if let agent_doc::mcp::HandlerResult::Ready(Some(resp)) = engine.handle(request) {
            println!("MCP Response: {resp}");
        }
    }

    Ok(())
}
```

### 3. Static Documentation & CI Tests (`tests/doc_tests.rs`)

```rust
use agent_doc::DocEngine;
use std::path::Path;

#[test]
fn test_documentation_freshness_and_lints() {
    let engine = create_engine();

    // 1. Maintain static entrypoints (or update with AGENT_DOC_BLESS=1)
    DocEngine::assert_or_bless(Path::new("docs/SUMMARY.md"), &engine.summary_md()).unwrap();
    DocEngine::assert_or_bless(Path::new("SKILL.md"), &engine.skill_md("kinema-guide", &[])).unwrap();
    DocEngine::assert_or_bless(Path::new("llms.txt"), &engine.llms_txt("https://example.com/docs")).unwrap();

    // 2. Verify link integrity (no broken links or missing anchors)
    let broken = engine.broken_links();
    assert!(broken.is_empty(), "Found broken links: {:?}", broken);

    // 3. Lint embedded code snippets
    for block in engine.code_blocks("rust") {
        println!("Checking code block from {} line {}", block.page_id, block.line);
    }
}
```

---

## mdBook Compatibility

If your project builds a web manual using [mdBook](https://rust-lang.github.io/mdBook/), the included [`scripts/strip_frontmatter.py`](scripts/strip_frontmatter.py) preprocessor strips leading YAML frontmatter during `mdbook build`, allowing identical Markdown source files to power both the CLI/MCP binary and online documentation.

---

## License

MIT OR Apache-2.0
