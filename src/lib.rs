pub mod index;
pub mod model;
pub mod parser;
pub mod renderer;

#[cfg(feature = "build")]
pub mod build;

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
        }
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

    /// Return remediation hint command if the diagnostic code/rule exists.
    pub fn hint(&self, code: &str) -> Option<String> {
        self.index.lookup_exact(code).map(|page| {
            format!(
                "Help: run '{} {} {}'",
                self.binary_name, self.explain_subcommand, page.id
            )
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
                    let label = if cat.use_id_as_title {
                        &p.id
                    } else {
                        &p.title
                    };
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
                                    let target: String =
                                        chars[start_target..tend].iter().collect();

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
}

/// Helper function to strip leading "docs/" prefix for mdBook relative paths.
fn clean_doc_path(path: &str) -> &str {
    path.strip_prefix("docs/").unwrap_or(path)
}

/// Compare content against an on-disk file, or update it when `AGENT_DOC_BLESS=1` or `AGENT_DOC_BLESS=true`.
pub fn assert_or_bless(
    file_path: impl AsRef<std::path::Path>,
    actual: &str,
) -> Result<(), String> {
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
            rendered_guide.contains("Rule: identity-missing (see: 'kinema explain identity-missing')"),
            "Rule link must be rewritten to executable explain command: {}",
            rendered_guide
        );

        // Test terminal mode rendering
        let rendered_term = engine
            .explain("identity-missing", RenderMode::Terminal)
            .expect("render terminal");
        assert!(rendered_term.contains("\x1b]8;;https://docs.kicad.org\x1b\\"), "OSC 8 link expected for external URL");

        // Test overview listing
        let overview = engine.guide(None, RenderMode::Agent).expect("render overview");
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
