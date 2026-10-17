# CLI & Workspace Guide

`agent-man-cli` is a standalone documentation browser, MCP server, and affordance-driven editing proxy included with `agent-man` (enabled via the `cli` feature).

For embedded library usage in custom binaries, see the [Embedding Guide](embedding.md) or return to the [agent-man Overview](../overview.md).

## 1. Zero-Rule Onboarding

To let coding agents browse and maintain a project's documentation without long instructions in `CLAUDE.md` or `AGENTS.md`, add a single line to your project's top document:

```text
Documentation is managed via agent-man. Run `agent-man-cli --root docs guide` to browse and edit.
```

Every rendered link, backlink, and diagnostic hint automatically includes `agent-man-cli --root docs`, guiding the agent through self-evident tool affordances.

## 2. Workspace Manifest (`agent-man.json`)

At the root of the documentation directory (`--root`), `agent-man.json` acts as the authoritative registry for categories, page metadata, visibility, and write permissions:

- **Category Visibility**:
  - `pinned`: Highlighted at the very top of the overview (`Must Read` entrypoints).
  - `normal`: Standard browsable guides, specifications, and rules.
  - `folded`: Collapsed in the default overview (e.g. `record` for ADRs and logs) to keep agent contexts clean. Expand on demand with `agent-man-cli --root docs list --category record`.
- **Access Control (`readonly` vs `readwrite`)**:
  - `readonly` pages are protected by `agent-man-cli`: line edits, metadata mutations, and deletions are rejected.
  - `readwrite` pages can be inspected and updated via partial line-range operations.
  - The `access` attribute itself cannot be modified via agent editing commands or MCP tools.

Run `agent-man-cli --root docs init` on any existing Markdown folder to generate or sync `agent-man.json`.

## 3. Bidirectional Links & Line-Range Editing

When reading any topic (`agent-man-cli --root docs guide <id>` or MCP `doc_read`), `agent-man-cli` automatically computes and displays incoming **Backlinks (`Referenced by`)** across the workspace.

To edit a `[readwrite]` page safely without rewriting the entire file:

1. **Inspect 1-indexed line numbers**:
   ```sh
   agent-man-cli --root docs lines <id> [--start 1 --end 40]
   ```
2. **Apply partial line mutations**:
   - **Insert** before line `N` (or append at EOF when `--line` is omitted):
     ```sh
     agent-man-cli --root docs insert <id> --line 15 --content "New paragraph."
     ```
   - **Replace** inclusive line range `start..=end`:
     ```sh
     agent-man-cli --root docs replace <id> --start 10 --end 12 --content "Updated section."
     ```
   - **Delete** inclusive line range `start..=end`:
     ```sh
     agent-man-cli --root docs delete-lines <id> --start 20 --end 22
     ```

Each edit immediately returns the updated surrounding lines with their new 1-indexed line numbers and warns if any broken links or orphan pages were introduced.

## 4. MCP Server Mode

Start an MCP server backed dynamically by the filesystem workspace:

```sh
# Over stdio:
agent-man-cli --root docs --mcp

# Over HTTP loopback:
agent-man-cli --root docs --mcp-http 127.0.0.1:8080
```

Exposes `doc_list`, `doc_read`, `doc_lines`, `doc_insert`, `doc_replace`, `doc_delete_lines`, `doc_create`, `doc_meta`, `doc_remove`, and `doc_lint`.

## 5. Publishing to GitHub Pages (mdBook)

`agent-man-cli` includes built-in support for [mdBook](https://rust-lang.github.io/mdBook/):

- Generate or update `<root>/SUMMARY.md` from `agent-man.json`:
  ```sh
  agent-man-cli --root docs summary --write
  ```
  (Note: `SUMMARY.md` is also automatically kept in sync whenever pages are added, removed, or updated when `<root>/SUMMARY.md` exists.)
- Strip YAML frontmatter during `mdbook build` using either `agent-man-cli mdbook-preprocessor` or `python3 scripts/strip_frontmatter.py` in `book.toml`.
