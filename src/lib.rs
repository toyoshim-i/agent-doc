pub mod index;
pub mod model;
pub mod parser;
pub mod renderer;

#[cfg(feature = "build")]
pub mod build;

#[cfg(feature = "mcp")]
pub mod mcp;

pub use index::DocIndex;
pub use model::{
    default_categories, BrokenLink, CategoryConfig, CodeBlock, DocPage, DocSection, RenderMode,
    ResolvedLink, TopicSummary,
};
pub use parser::parse_markdown_page;
pub use renderer::DocRenderer;

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("Topic '{0}' not found in documentation index")]
    NotFound(String),
}

#[derive(Debug, Clone)]
pub struct DocEngine {
    pub binary_name: String,
    pub guide_subcommand: String,
    pub explain_subcommand: String,
    pub scheme: String,
    pub index: DocIndex,
    pub categories: Vec<CategoryConfig>,
    pub version: Option<String>,
    #[cfg(feature = "mcp")]
    pub tools: Vec<mcp::Tool>,
}

impl DocEngine {
    pub fn new(binary_name: impl Into<String>) -> Self {
        let b = binary_name.into();
        Self {
            scheme: b.clone(),
            binary_name: b,
            guide_subcommand: "guide".to_string(),
            explain_subcommand: "explain".to_string(),
            index: DocIndex::new(),
            categories: default_categories(),
            version: None,
            #[cfg(feature = "mcp")]
            tools: Vec::new(),
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn set_version(&mut self, version: impl Into<String>) {
        self.version = Some(version.into());
    }

    pub fn with_subcommands(
        mut self,
        guide_subcommand: impl Into<String>,
        explain_subcommand: impl Into<String>,
    ) -> Self {
        self.guide_subcommand = guide_subcommand.into();
        self.explain_subcommand = explain_subcommand.into();
        self
    }

    pub fn with_scheme(mut self, scheme: impl Into<String>) -> Self {
        self.scheme = scheme.into();
        self
    }

    pub fn set_scheme(&mut self, scheme: impl Into<String>) {
        self.scheme = scheme.into();
    }

    pub fn with_categories(mut self, categories: Vec<CategoryConfig>) -> Self {
        self.categories = categories;
        self
    }

    pub fn set_categories(&mut self, categories: Vec<CategoryConfig>) {
        self.categories = categories;
    }

    pub fn add_page(&mut self, path: &str, content: &str) {
        let page = parse_markdown_page(path, content);
        self.index.add_page(page);
    }

    pub fn add_pages<I, S1, S2>(&mut self, pages: I)
    where
        I: IntoIterator<Item = (S1, S2)>,
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        for (path, content) in pages {
            self.add_page(path.as_ref(), content.as_ref());
        }
    }

    pub fn lookup(&self, query: &str) -> Option<&DocPage> {
        self.index.lookup(query)
    }

    /// Lookup page with exact match on id or alias.
    pub fn lookup_exact(&self, query: &str) -> Option<&DocPage> {
        self.index.lookup_exact(query)
    }

    pub fn list_topics(&self, category: Option<&str>) -> Vec<TopicSummary> {
        self.index.list_topics(category)
    }

    /// Return remediation hint command or MCP tool invocation if the diagnostic code/rule exists.
    pub fn hint(&self, code: &str, mode: RenderMode) -> Option<String> {
        self.index.lookup_exact(code).map(|page| match mode {
            RenderMode::Mcp => {
                format!("Help: call tool 'doc_read {{\"id\": \"{}\"}}'", page.id)
            }
            RenderMode::Agent | RenderMode::Terminal => {
                format!(
                    "Help: run '{} {} {}'",
                    self.binary_name, self.explain_subcommand, page.id
                )
            }
            RenderMode::Json => {
                format!("doc_read {{\"id\": \"{}\"}}", page.id)
            }
        })
    }

    /// Check a list of diagnostic error codes and return any codes that lack documentation.
    pub fn missing_pages<I, S>(&self, codes: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        codes
            .into_iter()
            .filter_map(|c| {
                let s = c.as_ref();
                if self.index.lookup_exact(s).is_none() {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Explain a diagnostic rule or specific topic by exact ID/alias lookup.
    pub fn explain(&self, query: &str, mode: RenderMode) -> Result<String, DocError> {
        if let Some(page) = self.index.lookup_exact(query) {
            let renderer = DocRenderer::new(
                &self.binary_name,
                &self.guide_subcommand,
                &self.explain_subcommand,
                &self.index,
            )
            .with_scheme(&self.scheme);
            Ok(renderer.render_page(page, mode))
        } else {
            Err(DocError::NotFound(query.to_string()))
        }
    }

    pub fn guide(&self, topic_opt: Option<&str>, mode: RenderMode) -> Result<String, DocError> {
        if let Some(topic) = topic_opt {
            let candidates = self.index.find_candidates(topic);
            if candidates.is_empty() {
                Err(DocError::NotFound(topic.to_string()))
            } else if candidates.len() == 1 {
                let renderer = DocRenderer::new(
                    &self.binary_name,
                    &self.guide_subcommand,
                    &self.explain_subcommand,
                    &self.index,
                )
                .with_scheme(&self.scheme);
                Ok(renderer.render_page(candidates[0], mode))
            } else {
                let mut out = format!("Multiple topics matched '{}':\n\n", topic);
                for c in candidates {
                    let cmd = format!("{} {} {}", self.binary_name, self.guide_subcommand, c.id);
                    if mode == RenderMode::Terminal {
                        out.push_str(&format!(
                            "  * \x1b[1m{}\x1b[0m: {} \x1b[90m(`{}`)\x1b[0m\n",
                            c.id, c.summary, cmd
                        ));
                    } else {
                        out.push_str(&format!("  * **{}**: {} (`{}`)\n", c.id, c.summary, cmd));
                    }
                }
                Ok(out)
            }
        } else {
            // Render index overview
            let mut out = String::new();
            let topics = self.list_topics(None);

            if mode == RenderMode::Terminal {
                out.push_str(&format!(
                    "\x1b[1;36m=== {} Documentation & Topic Index ===\x1b[0m\n\n",
                    self.binary_name
                ));
            } else {
                out.push_str(&format!(
                    "# {} Documentation & Topic Index\n\n",
                    self.binary_name
                ));
            }

            let mut matched_indices = std::collections::HashSet::new();

            for cat in &self.categories {
                let cat_topics: Vec<&TopicSummary> = topics
                    .iter()
                    .enumerate()
                    .filter(|(idx, t)| {
                        if cat.matches(&t.category) {
                            matched_indices.insert(*idx);
                            true
                        } else {
                            false
                        }
                    })
                    .map(|(_, t)| t)
                    .collect();

                if !cat_topics.is_empty() {
                    out.push_str(&format!("## {}\n", cat.heading));
                    for t in cat_topics {
                        let subcmd = if cat.key == "rule" {
                            &self.explain_subcommand
                        } else {
                            &self.guide_subcommand
                        };
                        let cmd = format!("{} {} {}", self.binary_name, subcmd, t.id);
                        if mode == RenderMode::Terminal {
                            out.push_str(&format!(
                                "  * \x1b[1m{}\x1b[0m: {} \x1b[90m(`{}`)\x1b[0m\n",
                                t.id, t.summary, cmd
                            ));
                        } else {
                            out.push_str(&format!("  * **{}**: {} (`{}`)\n", t.id, t.summary, cmd));
                        }
                    }
                    out.push('\n');
                }
            }

            let others: Vec<&TopicSummary> = topics
                .iter()
                .enumerate()
                .filter(|(idx, _)| !matched_indices.contains(idx))
                .map(|(_, t)| t)
                .collect();

            if !others.is_empty() {
                out.push_str("## Other Topics\n");
                for o in others {
                    let cmd = format!("{} {} {}", self.binary_name, self.guide_subcommand, o.id);
                    if mode == RenderMode::Terminal {
                        out.push_str(&format!(
                            "  * \x1b[1m{}\x1b[0m: {} \x1b[90m(`{}`)\x1b[0m\n",
                            o.id, o.summary, cmd
                        ));
                    } else {
                        out.push_str(&format!("  * **{}**: {} (`{}`)\n", o.id, o.summary, cmd));
                    }
                }
                out.push('\n');
            }

            Ok(out)
        }
    }

    /// Generate mdBook `SUMMARY.md` from registered pages and category configuration.
    pub fn summary_md(&self) -> String {
        let mut out = String::from("# Summary\n\n");

        let intro_page = self.index.pages.iter().find(|p| {
            p.category.eq_ignore_ascii_case("intro")
                || p.id.eq_ignore_ascii_case("readme")
                || p.id.eq_ignore_ascii_case("introduction")
        });

        if let Some(intro) = intro_page {
            out.push_str(&format!(
                "[{}]({})\n\n",
                intro.title,
                clean_doc_path(&intro.path)
            ));
        }

        for cat in &self.categories {
            let cat_pages: Vec<&DocPage> = self
                .index
                .pages
                .iter()
                .filter(|p| {
                    if Some(p.id.as_str()) == intro_page.map(|ip| ip.id.as_str()) {
                        return false;
                    }
                    cat.matches(&p.category)
                })
                .collect();

            if !cat_pages.is_empty() {
                out.push_str(&format!("# {}\n", cat.heading));
                for p in cat_pages {
                    let label = if cat.use_id_as_title { &p.id } else { &p.title };
                    out.push_str(&format!("- [{}]({})\n", label, clean_doc_path(&p.path)));
                }
                out.push('\n');
            }
        }

        out
    }

    /// Generate Agent `SKILL.md` entrypoint.
    pub fn skill_md(&self, name: &str, description: &str) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", name));
        out.push_str(&format!("description: {}\n", description));
        out.push_str("---\n\n");

        out.push_str(&format!("# {}\n\n", self.binary_name));
        out.push_str(&format!(
            "Run `{} {}` to browse available documentation and topics.\n\n",
            self.binary_name, self.guide_subcommand
        ));

        out.push_str("## Available Topics\n\n");
        for cat in &self.categories {
            let cat_pages: Vec<&DocPage> = self
                .index
                .pages
                .iter()
                .filter(|p| cat.matches(&p.category))
                .collect();

            if !cat_pages.is_empty() {
                out.push_str(&format!("### {}\n", cat.heading));
                for p in cat_pages {
                    let subcmd = if cat.key == "rule" {
                        &self.explain_subcommand
                    } else {
                        &self.guide_subcommand
                    };
                    let cmd = format!("{} {} {}", self.binary_name, subcmd, p.id);
                    out.push_str(&format!("- **{}**: {} (`{}`)\n", p.id, p.summary, cmd));
                }
                out.push('\n');
            }
        }

        out
    }

    /// Generate `llms.txt` index.
    pub fn llms_txt(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.binary_name));

        for cat in &self.categories {
            let cat_pages: Vec<&DocPage> = self
                .index
                .pages
                .iter()
                .filter(|p| cat.matches(&p.category))
                .collect();

            if !cat_pages.is_empty() {
                out.push_str(&format!("## {}\n", cat.heading));
                for p in cat_pages {
                    out.push_str(&format!(
                        "- [{}]({}): {}\n",
                        p.title,
                        clean_doc_path(&p.path),
                        p.summary
                    ));
                }
                out.push('\n');
            }
        }

        out
    }

    /// Extract fenced code blocks for a given language tag across all registered documentation pages.
    ///
    /// The returned `line` is the 1-indexed line number in the original markdown file.
    pub fn code_blocks(&self, lang: &str) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let lang_lower = lang.trim().to_lowercase();

        for page in &self.index.pages {
            let lines: Vec<&str> = page.source_content.lines().collect();
            let mut in_fence = false;
            let mut fence_lang = String::new();
            let mut start_line = 0;
            let mut current_block = String::new();

            for (idx, line) in lines.iter().enumerate() {
                let line_num = idx + 1;
                let trimmed = line.trim();

                if trimmed.starts_with("```") {
                    if in_fence {
                        // End of fence
                        if lang_lower.is_empty() || fence_lang.to_lowercase() == lang_lower {
                            blocks.push(CodeBlock {
                                page_id: page.id.clone(),
                                path: page.path.clone(),
                                line: start_line,
                                lang: fence_lang.clone(),
                                source: current_block.clone(),
                            });
                        }
                        in_fence = false;
                        fence_lang.clear();
                        current_block.clear();
                    } else {
                        // Start of fence
                        in_fence = true;
                        start_line = line_num;
                        let info = trimmed.trim_start_matches('`').trim();
                        fence_lang = info.split_whitespace().next().unwrap_or("").to_string();
                    }
                } else if in_fence {
                    current_block.push_str(line);
                    current_block.push('\n');
                }
            }
        }
        blocks
    }

    /// Scan all documentation pages for broken relative links and anchors.
    ///
    /// The returned `line` is the 1-indexed line number in the original markdown file.
    pub fn broken_links(&self) -> Vec<BrokenLink> {
        let mut broken = Vec::new();

        for page in &self.index.pages {
            let lines: Vec<&str> = page.source_content.lines().collect();
            let mut in_fence = false;

            for (idx, line) in lines.iter().enumerate() {
                let line_num = idx + 1;
                let trimmed = line.trim();
                if trimmed.starts_with("```") {
                    in_fence = !in_fence;
                    continue;
                }
                if in_fence {
                    continue;
                }

                let chars: Vec<char> = line.chars().collect();
                let len = chars.len();
                let mut i = 0;

                while i < len {
                    if chars[i] == '[' {
                        let start_label = i + 1;
                        let mut label_end = None;
                        let mut j = start_label;
                        while j < len {
                            if chars[j] == ']' {
                                label_end = Some(j);
                                break;
                            }
                            j += 1;
                        }

                        if let Some(lend) = label_end {
                            if lend + 1 < len && chars[lend + 1] == '(' {
                                let start_target = lend + 2;
                                let mut target_end = None;
                                let mut k = start_target;
                                while k < len {
                                    if chars[k] == ')' {
                                        target_end = Some(k);
                                        break;
                                    }
                                    k += 1;
                                }

                                if let Some(tend) = target_end {
                                    let label: String = chars[start_label..lend].iter().collect();
                                    let target: String = chars[start_target..tend].iter().collect();

                                    // Skip external URLs
                                    if !target.starts_with("http://")
                                        && !target.starts_with("https://")
                                        && !target.starts_with("mailto:")
                                    {
                                        let resolved = self.index.resolve_link(&page.path, &target);
                                        let is_broken = match resolved {
                                            None => true,
                                            Some(ref res) => {
                                                if let Some(ref anchor) = res.anchor {
                                                    // Verify section anchor exists in target page
                                                    if let Some(target_page) =
                                                        self.index.lookup_exact(&res.target_id)
                                                    {
                                                        !target_page
                                                            .sections
                                                            .iter()
                                                            .any(|s| s.anchor == *anchor)
                                                    } else {
                                                        true
                                                    }
                                                } else {
                                                    false
                                                }
                                            }
                                        };

                                        if is_broken {
                                            broken.push(BrokenLink {
                                                page_id: page.id.clone(),
                                                path: page.path.clone(),
                                                line: line_num,
                                                text: label,
                                                target,
                                            });
                                        }
                                    }

                                    i = tend + 1;
                                    continue;
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }
        }

        broken
    }

    /// Compare content against an on-disk file, or update it when `AGENT_DOC_BLESS=1`.
    pub fn assert_or_bless(
        &self,
        file_path: impl AsRef<std::path::Path>,
        actual: &str,
    ) -> Result<(), String> {
        assert_or_bless(file_path, actual)
    }

    /// List all documentation pages as MCP resources (`<scheme>://doc/<id>`).
    #[cfg(feature = "mcp")]
    pub fn mcp_list_resources(&self) -> Vec<mcp::McpResource> {
        self.index
            .pages
            .iter()
            .map(|p| mcp::McpResource {
                uri: format!("{}://doc/{}", self.scheme, p.id),
                name: p.title.clone(),
                description: p.summary.clone(),
                mime_type: "text/markdown".to_string(),
            })
            .collect()
    }

    /// Read an MCP documentation resource by URI.
    #[cfg(feature = "mcp")]
    pub fn mcp_read_resource(&self, uri: &str) -> Result<mcp::McpResourceContents, DocError> {
        let prefix = format!("{}://doc/", self.scheme);
        let without_prefix = uri
            .strip_prefix(&prefix)
            .ok_or_else(|| DocError::NotFound(format!("Invalid resource URI: {uri}")))?;

        let (id, anchor) = match without_prefix.split_once('#') {
            Some((i, a)) => (i, Some(a)),
            None => (without_prefix, None),
        };

        let page = self
            .index
            .lookup_exact(id)
            .ok_or_else(|| DocError::NotFound(id.to_string()))?;

        let text = if let Some(anchor) = anchor {
            let sec = page
                .sections
                .iter()
                .find(|s| s.anchor == anchor || crate::parser::slugify(&s.heading) == anchor)
                .ok_or_else(|| DocError::NotFound(format!("Section '{anchor}' in topic '{id}'")))?;
            format!("## {}\n\n{}", sec.heading, sec.content)
        } else {
            self.explain(id, RenderMode::Mcp)?
        };

        Ok(mcp::McpResourceContents {
            uri: uri.to_string(),
            mime_type: "text/markdown".to_string(),
            text,
        })
    }

    /// Invoke an MCP tool by name. Handles built-in `doc_list` and `doc_read` tools.
    /// Invoke an MCP tool by name and return a `HandlerResult` (Ready or Pending).
    #[cfg(feature = "mcp")]
    pub fn mcp_call_tool_handler(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> mcp::HandlerResult<mcp::ToolResult> {
        match name {
            "doc_list" => {
                let topics = self.list_topics(None);
                let mut out = String::new();
                for t in topics {
                    out.push_str(&format!("- {}: {} ({})\n", t.id, t.title, t.summary));
                }
                mcp::HandlerResult::Ready(mcp::ToolResult::text(out))
            }
            "doc_read" => {
                let id = match args.get("id").and_then(|v| v.as_str()) {
                    Some(i) => i,
                    None => {
                        return mcp::HandlerResult::Ready(mcp::ToolResult::error(
                            "Missing required argument 'id'",
                        ))
                    }
                };

                let page = match self.index.lookup_exact(id) {
                    Some(p) => p,
                    None => {
                        return mcp::HandlerResult::Ready(mcp::ToolResult::error(format!(
                            "Topic '{id}' not found"
                        )))
                    }
                };

                if let Some(section) = args.get("section").and_then(|v| v.as_str()) {
                    if let Some(sec) = page.sections.iter().find(|s| {
                        s.anchor == section || crate::parser::slugify(&s.heading) == section
                    }) {
                        mcp::HandlerResult::Ready(mcp::ToolResult::text(format!(
                            "## {}\n\n{}",
                            sec.heading, sec.content
                        )))
                    } else {
                        mcp::HandlerResult::Ready(mcp::ToolResult::error(format!(
                            "Section '{section}' not found in topic '{id}'"
                        )))
                    }
                } else {
                    match self.explain(id, RenderMode::Mcp) {
                        Ok(text) => mcp::HandlerResult::Ready(mcp::ToolResult::text(text)),
                        Err(e) => mcp::HandlerResult::Ready(mcp::ToolResult::error(e.to_string())),
                    }
                }
            }
            _ => {
                if let Some(tool) = self.tools.iter().find(|t| t.name == name) {
                    if !tool.skip_validation {
                        if let Err(val_err) = tool.validate_args(&args, &self.scheme) {
                            return mcp::HandlerResult::Ready(val_err);
                        }
                    }
                    (tool.handler)(args)
                } else {
                    mcp::HandlerResult::Ready(mcp::ToolResult::error(format!(
                        "Unknown tool: '{name}'"
                    )))
                }
            }
        }
    }

    /// Invoke an MCP tool synchronously by name, blocking if pending (on supported platforms).
    #[cfg(feature = "mcp")]
    pub fn mcp_call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<mcp::ToolResult, DocError> {
        match self.mcp_call_tool_handler(name, args) {
            mcp::HandlerResult::Ready(r) => Ok(r),
            mcp::HandlerResult::Pending(p) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Ok(p.settled())
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let mut p = p;
                    if let Some(r) = p.poll_settled() {
                        Ok(r)
                    } else {
                        Ok(mcp::ToolResult::error(
                            "Tool execution pending in wasm environment",
                        ))
                    }
                }
            }
        }
    }

    /// Handle a raw JSON-RPC request string directly without network I/O.
    #[cfg(feature = "mcp")]
    pub fn handle(&self, request: &str) -> mcp::HandlerResult<Option<String>> {
        mcp::protocol::handle_json_rpc(self, request)
    }

    /// Serve MCP JSON-RPC protocol over standard input/output.
    #[cfg(feature = "mcp-stdio")]
    pub fn serve_stdio(&self) -> std::io::Result<()> {
        mcp::stdio::serve_stdio(self)
    }

    /// Serve MCP JSON-RPC protocol over custom reader and writer streams.
    #[cfg(feature = "mcp-stdio")]
    pub fn serve_stdio_io<R: std::io::BufRead, W: std::io::Write>(
        &self,
        reader: R,
        writer: W,
    ) -> std::io::Result<()> {
        mcp::stdio::serve_stdio_io(self, reader, writer)
    }

    /// Serve MCP JSON-RPC protocol over HTTP loopback, returning an `HttpServer` handle.
    #[cfg(feature = "mcp-http")]
    pub fn serve_http(
        &self,
        addr: impl std::net::ToSocketAddrs,
    ) -> std::io::Result<mcp::http::HttpServer> {
        mcp::http::serve_http(self, addr)
    }

    /// Register a custom tool for MCP execution.
    #[cfg(feature = "mcp")]
    pub fn register_tool(&mut self, tool: mcp::Tool) {
        self.tools.push(tool);
    }

    /// Builder helper to attach a tool.
    #[cfg(feature = "mcp")]
    pub fn with_tool(mut self, tool: mcp::Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Register clap subcommands as MCP tools, parsing inputs through clap's parser.
    #[cfg(all(feature = "mcp", feature = "clap"))]
    pub fn register_clap_tools<F>(&mut self, cmd: ::clap::Command, exclude: &[&str], handler: F)
    where
        F: Fn(&str, ::clap::ArgMatches) -> mcp::HandlerResult<mcp::ToolResult>
            + Send
            + Sync
            + 'static,
    {
        let tools = mcp::clap::tools_from_clap(cmd, exclude, handler);
        for tool in tools {
            self.register_tool(tool);
        }
    }

    /// List all available MCP tools including built-in doc tools and registered host tools.
    #[cfg(feature = "mcp")]
    pub fn mcp_list_tools(&self) -> Vec<mcp::McpToolDefinition> {
        let mut list = vec![
            mcp::McpToolDefinition {
                name: "doc_list".to_string(),
                description: "List all available documentation topics and summaries".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                }),
            },
            mcp::McpToolDefinition {
                name: "doc_read".to_string(),
                description:
                    "Read documentation page by ID or extract a specific section by anchor"
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Documentation page ID" },
                        "section": { "type": "string", "description": "Optional section anchor" }
                    },
                    "required": ["id"]
                }),
            },
        ];

        for t in &self.tools {
            list.push(t.to_mcp_definition(&self.scheme));
        }

        list
    }

    /// Verify tool definitions for consistency:
    /// - Checks that all `.see` references point to existing documentation page IDs.
    /// - Checks that all `.example` payloads pass argument schema validation.
    #[cfg(feature = "mcp")]
    pub fn check_tools(&self) -> Vec<String> {
        let mut issues = Vec::new();
        for tool in &self.tools {
            if let Some(ref see) = tool.see_page {
                if self.index.lookup_exact(see).is_none() {
                    issues.push(format!(
                        "Tool '{}' references nonexistent documentation page in .see('{}')",
                        tool.name, see
                    ));
                }
            }

            for (idx, example) in tool.examples.iter().enumerate() {
                if let Err(err) = tool.validate_args(example, &self.scheme) {
                    issues.push(format!(
                        "Tool '{}' example #{} failed schema validation: {:?}",
                        tool.name, idx, err
                    ));
                }
            }
        }
        issues
    }
}

/// Helper function to strip leading "docs/" prefix for mdBook relative paths.
fn clean_doc_path(path: &str) -> &str {
    path.strip_prefix("docs/").unwrap_or(path)
}

/// Compare content against an on-disk file, or update it when `AGENT_DOC_BLESS=1` or `AGENT_DOC_BLESS=true`.
pub fn assert_or_bless(file_path: impl AsRef<std::path::Path>, actual: &str) -> Result<(), String> {
    let path = file_path.as_ref();
    let bless = std::env::var("AGENT_DOC_BLESS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if bless {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, actual).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        if !path.exists() {
            return Err(format!(
                "File does not exist: {}. Set AGENT_DOC_BLESS=1 to generate.",
                path.display()
            ));
        }
        let current = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if current.trim() == actual.trim() {
            Ok(())
        } else {
            Err(format!(
                "Content mismatch for {}:\n--- On disk:\n{}\n--- Generated:\n{}\nSet AGENT_DOC_BLESS=1 to update.",
                path.display(),
                current,
                actual
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_doc_index_and_link_rewriting() {
        let mut engine = DocEngine::new("kinema");

        let guide_md = r#"---
id: identity
title: Component Identity and UUID Management
aliases: [uuid, refactoring]
summary: How to keep PCB footprint placement intact when renaming components.
---

# Component Identity

KiCad identifies parts by UUID v5.
See [Rule: identity-missing](../rules/identity-missing.md) for related diagnostics.
"#;

        let rule_md = r#"---
id: identity-missing
title: Missing or Mismatched Footprint Identity
aliases: [uuid-mismatch]
summary: PCB footprint timestamp does not match circuit UUID v5.
---

# Missing Identity

When refdes changes, check [Identity Guide](../guides/identity.md).
Also check external docs at [KiCad Docs](https://docs.kicad.org).
"#;

        let migration_md = r#"---
id: migration
title: KiCad Migration Guide
category: getting-started
summary: How to migrate an existing KiCad project safely.
---

# KiCad Migration
"#;

        engine.add_page("guides/migration.md", migration_md);
        engine.add_page("guides/identity.md", guide_md);
        engine.add_page("rules/identity-missing.md", rule_md);

        // Test lookup
        let page = engine.lookup("identity").expect("lookup identity");
        assert_eq!(page.id, "identity");

        let page_alias = engine.lookup("uuid-mismatch").expect("lookup alias");
        assert_eq!(page_alias.id, "identity-missing");

        // Test agent mode rendering with link rewriting to CLI commands
        let rendered_rule = engine
            .explain("identity-missing", RenderMode::Agent)
            .expect("render explain");

        assert!(
            rendered_rule.contains("Identity Guide (see: 'kinema guide identity')"),
            "Internal link must be rewritten to executable guide command: {}",
            rendered_rule
        );
        assert!(
            rendered_rule.contains("KiCad Docs (https://docs.kicad.org)"),
            "External link must preserve URL: {}",
            rendered_rule
        );

        // Test guide mode rendering
        let rendered_guide = engine
            .explain("identity", RenderMode::Agent)
            .expect("render guide");

        assert!(
            rendered_guide
                .contains("Rule: identity-missing (see: 'kinema explain identity-missing')"),
            "Rule link must be rewritten to executable explain command: {}",
            rendered_guide
        );

        // Test terminal mode rendering
        let rendered_term = engine
            .explain("identity-missing", RenderMode::Terminal)
            .expect("render terminal");
        assert!(
            rendered_term.contains("\x1b]8;;https://docs.kicad.org\x1b\\"),
            "OSC 8 link expected for external URL"
        );

        // Test overview listing
        let overview = engine
            .guide(None, RenderMode::Agent)
            .expect("render overview");
        assert!(overview.contains("## Getting Started & Migration"));
        assert!(overview.contains("## Guides & Best Practices"));
        assert!(overview.contains("## Diagnostics & Rules"));
        assert!(overview.contains("`kinema guide migration`"));
        assert!(overview.contains("`kinema guide identity`"));
        assert!(overview.contains("`kinema explain identity-missing`"));

        // Verify Getting Started appears before Guides & Best Practices
        let start_pos = overview.find("## Getting Started & Migration").unwrap();
        let guides_pos = overview.find("## Guides & Best Practices").unwrap();
        assert!(start_pos < guides_pos);
    }
}
