# agent-man Overview

`agent-man` is a **context-engineering-free** documentation and Model Context Protocol (MCP) engine for Rust applications and local Markdown knowledge bases.

Instead of stuffing massive instruction files into an LLM's initial prompt, `agent-man` turns documentation into a self-discoverable, hyperlinked system where agents retrieve only what they need and follow built-in tool affordances.

## Two Ways to Use `agent-man`

1. **Embedded Library (`agent-man` crate)**
   Compile Markdown documentation directly into your Rust binary (`build.rs`) and expose `guide`, `explain`, and MCP servers (`stdio`, `HTTP`, `WebMCP`) with zero runtime file dependencies.
   See [Embedding Guide](guides/embedding.md).

2. **Dynamic Workspace & Editing Proxy (`agent-man-cli`)**
   Point `agent-man-cli --root <DIR>` at any directory of Markdown files managed by an `agent-man.json` manifest. Browse bidirectional links, separate active guides from folded historical records, and let agents safely maintain `[readwrite]` documentation via line-numbered partial edits.
   See [CLI & Workspace Guide](guides/cli-workspace.md).

## Architectural Records

Historical design decisions are stored in the `record` category so they stay folded out of the default overview while remaining reachable via links or explicit category queries:

- [ADR: Multimodal Image Content in v0.5.0](records/v0.5.0-image-content.md)
