# Embedding Guide

`agent-man` can be embedded directly into any Rust CLI application, language server, or WebAssembly runtime. Read the [agent-man Overview](../overview.md) for the high-level architecture.

## 1. Compile-Time Embedding (`build.rs`)

Enable the `build` feature in `[build-dependencies]` and scan your `docs/` directory at compile time:

```rust
fn main() {
    #[cfg(feature = "build")]
    agent_man::build::embed("docs").expect("Failed to embed docs");
}
```

## 2. Initializing `DocEngine`

Construct a `DocEngine` with your binary name, subcommands, and embedded pages:

```rust
use agent_man::{DocEngine, RenderMode};

pub fn build_engine() -> DocEngine {
    let mut engine = DocEngine::new("mycli")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_subcommands("guide", "explain");

    // Load compile-time embedded pages:
    // let pages: &[(&str, &str)] = &include!(concat!(env!("OUT_DIR"), "/agent_man_pages.rs"));
    // engine.add_pages(pages.iter().copied());
    engine
}
```

## 3. MCP & Multimodal Tool Results

With `mcp`, `mcp-stdio`, `mcp-http`, or `webmcp` enabled, `DocEngine` serves `resources/list`, `resources/read`, `doc_list`, `doc_read`, and custom tools registered via `ToolBuilder` or `register_clap_tools`.

Tools can return text, structured JSON, or base64-encoded images (see [ADR: Multimodal Image Content in v0.5.0](../records/v0.5.0-image-content.md)).

## 4. Static Entrypoints & CI Checks

In your test suite, use `DocEngine` to verify link integrity and bless static entrypoints:

- `engine.broken_links()` checks all relative links and `#section` anchors.
- `engine.summary_md()`, `engine.skill_md(...)`, and `engine.llms_txt()` generate static artifacts verified via `agent_man::assert_or_bless`.

For filesystem-backed dynamic documentation without writing a custom Rust host binary, see [CLI & Workspace Guide](cli-workspace.md).
