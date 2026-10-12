# agent-man

> **Context-engineering-free, agent-native documentation and MCP integration engine**

[![CI](https://github.com/toyoshim-i/agent-man/actions/workflows/ci.yml/badge.svg)](https://github.com/toyoshim-i/agent-man/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/agent-man.svg)](https://crates.io/crates/agent-man)
[![Docs.rs](https://docs.rs/agent-man/badge.svg)](https://docs.rs/agent-man)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

`agent-man` is an embedded documentation and Model Context Protocol (MCP) engine for Rust CLI tools, agent frameworks, and web runtimes. It turns a single source of Markdown documentation into an on-demand CLI knowledge system, static entry points (`SUMMARY.md`, `SKILL.md`, `llms.txt`), and production MCP servers.

---

## The Philosophy: Context-Engineering-Free

Dumping massive prompt files or skill definitions into LLM contexts causes prompt bloat, high token costs, and instruction degradation. 

`agent-man` enables **context-engineering-free** design:
1. **Self-Evident Discovery**: CLI `--help` points agents directly to your in-binary guide (`mycli guide`).
2. **Just-in-Time Knowledge**: Agents retrieve specific topics (`mycli guide <topic>`) only when relevant, keeping working contexts clean.
3. **Automated Diagnostic Remediation**: Diagnostics emit self-documenting hints (`Help: run 'mycli explain <code>'` or `Help: call tool 'doc_read {"id": "<code>"}'`). Agents or MCP clients receive actionable root-cause analysis and code fixes.
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
  - Register custom tools with typed schemas (`ToolBuilder`) or directly from `clap::Command` (`register_tool` returns `Result<(), DocError>`, rejecting duplicate tool names or collisions with reserved built-in `doc_list` and `doc_read`).
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
| `build` | Compile-time directory scanner and embedding helper (`agent_man::build::embed`) | None |
| `mcp` | MCP types, tool builder, JSON Schema generator, and `DocEngine::handle()` | None |
| `mcp-stdio` | MCP standard input/output transport (`serve_stdio()`) | `mcp` |
| `mcp-http` | MCP HTTP POST loopback transport (`serve_http()`) using `std::net` | `mcp` |
| `clap` | Auto-generate MCP tools from `clap::Command` subcommands | `mcp`, `clap` |
| `webmcp` | WebMCP browser integration (`install()`, `call_tool()`) for `wasm32` | `mcp`, `wasm-bindgen`, `js-sys`, `web-sys`, `wasm-bindgen-futures` |

---

## Minimal Example: Complete Integration

### 1. Compile-Time Embedding (`build.rs`)

```rust
// build.rs
fn main() {
    #[cfg(feature = "build")]
    agent_man::build::embed("docs");
}
```

### 2. Host Integration (`src/main.rs`)

```rust
use agent_man::{CategoryConfig, DocEngine, RenderMode};

fn create_engine() -> DocEngine {
    let mut engine = DocEngine::new("kinema")
        .with_version(env!("CARGO_PKG_VERSION")) // MCP serverInfo.version (defaults to agent-man version if omitted)
        .with_subcommands("guide", "explain")
        .with_scheme("kinema")
        .with_categories(vec![
            CategoryConfig::new("guide", "Guides"),
            CategoryConfig::new("rule", "Rules & Diagnostics"),
        ]);

    // Include the generated pages from build.rs
    // include!(concat!(env!("OUT_DIR"), "/agent_man_pages.rs"));
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
    println!("{}", engine.hint("E001", RenderMode::Agent).unwrap()); // "Help: run 'kinema explain E001'"
    println!("{}", engine.hint("E001", RenderMode::Mcp).unwrap());   // "Help: call tool 'doc_read {\"id\": \"E001\"}'"

    // 2. Register tools (typed or from clap)
    #[cfg(feature = "clap")]
    engine.register_clap_tools(cli_cmd, &["mcp"], |_subcmd, matches| {
        // Receives root ArgMatches; deserialize directly into your Cli struct:
        // let cli = Cli::from_arg_matches(&matches).unwrap();
        agent_man::mcp::ToolResult::text("Executed command").into()
    })?;

    // 3. MCP Server (stdio or HTTP)
    #[cfg(feature = "mcp-stdio")]
    if std::env::args().any(|a| a == "--mcp") {
        return engine.serve_stdio().map_err(Into::into);
    }

    #[cfg(feature = "mcp-http")]
    if std::env::args().any(|a| a == "--mcp-http") {
        let server = engine.serve_http("127.0.0.1:8080")?;
        println!("Serving MCP HTTP at {}", server.local_addr());
        let _ = server.join();
        return Ok(());
    }

    // 3. Direct JSON-RPC Dispatch (In-Process / WebMCP)
    #[cfg(feature = "mcp")]
    {
        let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
        if let agent_man::mcp::HandlerResult::Ready(Some(resp)) = engine.handle(request) {
            println!("MCP Response: {resp}");
        }
    }

    Ok(())
}
```

### 3. Static Documentation & CI Tests (`tests/doc_tests.rs`)

```rust
use agent_man::DocEngine;
use std::path::Path;

#[test]
fn test_documentation_freshness_and_lints() {
    let engine = create_engine();

    // 1. Maintain static entrypoints (or update with AGENT_MAN_BLESS=1)
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

### 4. WebMCP Browser Integration (wasm32)

When running in browser environments via WebAssembly, `agent-man` provides integration with the W3C Web Model Context Protocol (`document.modelContext` / `navigator.modelContext`) and exposes tools to browser extensions or DevTools consoles.

Enable the `webmcp` feature in your WebAssembly target dependencies:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
agent-man = { version = "0.4", features = ["webmcp"] }
```

In your browser initialization:

```rust
#[cfg(all(feature = "webmcp", target_arch = "wasm32"))]
{
    use agent_man::webmcp::{install, WebMcpOptions};
    use std::rc::Rc;
    use std::time::Duration;

    let engine = Rc::new(create_engine());

    let options = WebMcpOptions {
        // 1. Expose global window object (e.g. window.__my_app_mcp)
        global_name: Some("__my_app_mcp".to_string()),
        // 2. Hook called right after tool dispatch to wake the host's render loop
        on_call: Some(Rc::new(|| {
            // e.g. event_loop_proxy.send_event(());
        })),
        // 3. Timeout for pending async operations before timing out
        timeout: Duration::from_secs(10),
    };

    let installed = install(engine, options).expect("Failed to initialize WebMCP");
    println!(
        "WebMCP available: {}, registered {} tools",
        installed.model_context, installed.registered
    );
}
```

#### Behavioral Details & Harness Testing

- **Context discovery**: `install` searches `document.modelContext` followed by `navigator.modelContext`. If neither is found or `registerTool` is missing, `installed.model_context` is `false` without failing.
- **Global object**: When `global_name` is set, `window[name]` exposes `listTools()`, `callTool(name, args)`, `listResources()`, and `readResource(uri)`.
- **AnimationFrame queue**: Pending results yield control using `requestAnimationFrame`. Note that in background tabs where the browser suspends animation frames, tool waiting also pauses until the tab is focused.
- **Verification**: In-browser execution cannot be tested in `agent-man`'s own CI environment; host applications (e.g. Karakuri's browser harness) should verify end-to-end WebMCP interactions in their browser test suites.

---

## mdBook Compatibility

If your project builds a web manual using [mdBook](https://rust-lang.github.io/mdBook/), the included [`scripts/strip_frontmatter.py`](scripts/strip_frontmatter.py) preprocessor strips leading YAML frontmatter during `mdbook build`, allowing identical Markdown source files to power both the CLI/MCP binary and online documentation.

---

## License

MIT OR Apache-2.0
