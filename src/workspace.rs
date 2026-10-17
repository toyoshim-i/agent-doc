use crate::model::{BrokenLink, CategoryConfig, RenderMode};
use crate::parser::parse_markdown_page;
use crate::renderer::DocRenderer;
use crate::DocEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Standard manifest filename placed at the documentation root.
pub const MANIFEST_FILENAME: &str = "agent-man.json";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "Workspace manifest '{MANIFEST_FILENAME}' not found in '{0}'. Run '{1} init' to initialize it."
    )]
    ManifestNotFound(String, String),
    #[error("Topic '{0}' not found in workspace. Run '{1} guide' to list available topics.")]
    PageNotFound(String, String),
    #[error(
        "Page '{0}' is marked as 'readonly' in {MANIFEST_FILENAME} and cannot be modified or removed via agent tools."
    )]
    ReadOnly(String),
    #[error("Page '{0}' already exists in workspace. Use 'lines' and 'replace'/'insert' to edit content, or 'meta' to update metadata.")]
    PageAlreadyExists(String),
    #[error("Invalid line range: {0}")]
    InvalidLineRange(String),
    #[error("Invalid path '{0}': must be a relative path inside the workspace root")]
    InvalidPath(String),
    #[error(
        "Cannot remove page '{0}' because it is still referenced by: {1}. Update those links first or pass --force (force: true)."
    )]
    HasBacklinks(String, String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    #[serde(alias = "read-only", alias = "ro")]
    ReadOnly,
    #[default]
    #[serde(alias = "read-write", alias = "rw")]
    ReadWrite,
}

impl AccessMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessMode::ReadOnly => "readonly",
            AccessMode::ReadWrite => "readwrite",
        }
    }
}

impl std::fmt::Display for AccessMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CategoryVisibility {
    /// Highlighted at the very top of the overview (e.g., must-read entrypoints).
    Pinned,
    /// Displayed normally under its category heading.
    #[default]
    Normal,
    /// Collapsed in the top-level overview so historical logs/records do not bloat context.
    Folded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCategory {
    pub key: String,
    pub heading: String,
    #[serde(default)]
    pub visibility: CategoryVisibility,
    #[serde(default)]
    pub default_access: AccessMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageMeta {
    pub id: String,
    pub path: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    #[serde(default)]
    pub access: AccessMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_workspace_categories")]
    pub categories: Vec<WorkspaceCategory>,
    #[serde(default)]
    pub pages: Vec<PageMeta>,
}

pub fn default_workspace_categories() -> Vec<WorkspaceCategory> {
    vec![
        WorkspaceCategory {
            key: "entrypoint".to_string(),
            heading: "Entrypoints (Must Read)".to_string(),
            visibility: CategoryVisibility::Pinned,
            default_access: AccessMode::ReadOnly,
        },
        WorkspaceCategory {
            key: "guide".to_string(),
            heading: "Guides & Architecture".to_string(),
            visibility: CategoryVisibility::Normal,
            default_access: AccessMode::ReadWrite,
        },
        WorkspaceCategory {
            key: "rule".to_string(),
            heading: "Diagnostics & Rules".to_string(),
            visibility: CategoryVisibility::Normal,
            default_access: AccessMode::ReadOnly,
        },
        WorkspaceCategory {
            key: "record".to_string(),
            heading: "Records & History".to_string(),
            visibility: CategoryVisibility::Folded,
            default_access: AccessMode::ReadWrite,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitReport {
    pub manifest_path: String,
    pub added_pages: Vec<String>,
    pub total_pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub category: Option<String>,
    pub path: Option<String>,
    pub aliases: Vec<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintReport {
    pub missing_files: Vec<String>,
    pub unregistered_files: Vec<String>,
    pub broken_links: Vec<BrokenLink>,
    pub orphan_pages: Vec<String>,
}

impl LintReport {
    pub fn is_clean(&self) -> bool {
        self.missing_files.is_empty()
            && self.unregistered_files.is_empty()
            && self.broken_links.is_empty()
            && self.orphan_pages.is_empty()
    }

    pub fn format_report(&self, command_prefix: &str) -> String {
        if self.is_clean() {
            return "Workspace lint passed: 0 broken links, 0 missing files, 0 unregistered files, 0 orphan pages.".to_string();
        }

        let mut out = String::from("Workspace Lint Report:\n");

        if !self.missing_files.is_empty() {
            out.push_str("\nMissing files on disk (registered in agent-man.json):\n");
            for f in &self.missing_files {
                out.push_str(&format!("  - {f}\n"));
            }
        }

        if !self.unregistered_files.is_empty() {
            out.push_str(&format!(
                "\nUnregistered .md files on disk (run '{command_prefix} init' to register):\n"
            ));
            for f in &self.unregistered_files {
                out.push_str(&format!("  - {f}\n"));
            }
        }

        if !self.broken_links.is_empty() {
            out.push_str("\nBroken internal links:\n");
            for b in &self.broken_links {
                out.push_str(&format!(
                    "  - {} ({}:{}): '[{}]({})'\n",
                    b.page_id, b.path, b.line, b.text, b.target
                ));
            }
        }

        if !self.orphan_pages.is_empty() {
            out.push_str("\nOrphan pages (not pinned and not linked from any other page):\n");
            for id in &self.orphan_pages {
                out.push_str(&format!(
                    "  - {id} (Hint: add a relative link to this page from an entrypoint or parent guide)\n"
                ));
            }
        }

        out
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub command_prefix: String,
    pub manifest: WorkspaceManifest,
    pub engine: DocEngine,
    pub missing_files: Vec<String>,
}

impl Workspace {
    /// Initialize or synchronize `agent-man.json` by scanning `.md` files in `root`.
    pub fn init(
        root: impl AsRef<Path>,
        name_opt: Option<&str>,
        description_opt: Option<&str>,
    ) -> Result<InitReport, WorkspaceError> {
        let root_path = root.as_ref();
        fs::create_dir_all(root_path)?;

        let manifest_path = root_path.join(MANIFEST_FILENAME);
        let mut manifest = if manifest_path.exists() {
            let raw = fs::read_to_string(&manifest_path)?;
            let mut m: WorkspaceManifest = serde_json::from_str(&raw)?;
            if let Some(n) = name_opt {
                m.name = n.to_string();
            }
            if let Some(d) = description_opt {
                m.description = d.to_string();
            }
            m
        } else {
            let default_name = name_opt
                .map(|s| s.to_string())
                .or_else(|| {
                    root_path
                        .canonicalize()
                        .ok()
                        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                })
                .unwrap_or_else(|| "Documentation".to_string());

            WorkspaceManifest {
                name: default_name,
                description: description_opt.unwrap_or("").to_string(),
                categories: default_workspace_categories(),
                pages: Vec::new(),
            }
        };

        let mut md_files = Vec::new();
        collect_workspace_md_files(root_path, root_path, &mut md_files)?;
        md_files.sort();

        let existing_paths: HashSet<String> = manifest
            .pages
            .iter()
            .map(|p| normalize_rel_path(&p.path))
            .collect();
        let mut existing_ids: HashSet<String> =
            manifest.pages.iter().map(|p| p.id.to_lowercase()).collect();

        let mut added_pages = Vec::new();

        for rel_path in md_files {
            let norm = normalize_rel_path(&rel_path);
            if existing_paths.contains(&norm) {
                continue;
            }

            let full_path = root_path.join(&norm);
            let content = fs::read_to_string(&full_path)?;
            let parsed = parse_markdown_page(&norm, &content);

            let category = infer_category_for_init(&norm, &parsed.category);
            let access = manifest
                .categories
                .iter()
                .find(|c| c.key.eq_ignore_ascii_case(&category))
                .map(|c| c.default_access)
                .unwrap_or(AccessMode::ReadWrite);

            let mut candidate_id = parsed.id.clone();
            let mut suffix = 2;
            while existing_ids.contains(&candidate_id.to_lowercase()) {
                candidate_id = format!("{}-{}", parsed.id, suffix);
                suffix += 1;
            }
            existing_ids.insert(candidate_id.to_lowercase());

            let summary = if parsed.summary.trim().is_empty() {
                format!("Documentation page for {}", parsed.title)
            } else {
                parsed.summary
            };

            manifest.pages.push(PageMeta {
                id: candidate_id.clone(),
                path: norm,
                title: parsed.title,
                summary,
                category,
                access,
                aliases: parsed.aliases,
            });
            added_pages.push(candidate_id);
        }

        let json_str = serde_json::to_string_pretty(&manifest)? + "\n";
        fs::write(&manifest_path, json_str)?;

        let total_pages = manifest.pages.len();
        let ws = Self::load(root_path, "agent-man-cli")?;
        ws.sync_summary_if_present()?;

        Ok(InitReport {
            manifest_path: manifest_path.to_string_lossy().to_string(),
            added_pages,
            total_pages,
        })
    }

    /// Load an existing workspace from `root`.
    pub fn load(
        root: impl AsRef<Path>,
        command_prefix: impl Into<String>,
    ) -> Result<Self, WorkspaceError> {
        let root_path = root.as_ref().to_path_buf();
        let prefix = command_prefix.into();
        let manifest_path = root_path.join(MANIFEST_FILENAME);
        if !manifest_path.exists() {
            return Err(WorkspaceError::ManifestNotFound(
                root_path.display().to_string(),
                prefix,
            ));
        }

        let raw = fs::read_to_string(&manifest_path)?;
        let manifest: WorkspaceManifest = serde_json::from_str(&raw)?;

        let categories: Vec<CategoryConfig> = manifest
            .categories
            .iter()
            .map(|c| CategoryConfig::new(&c.key, &c.heading))
            .collect();

        let mut engine = DocEngine::new(&prefix)
            .with_subcommands("guide", "explain")
            .with_scheme("agent-man")
            .with_categories(categories);

        let mut missing_files = Vec::new();

        for meta in &manifest.pages {
            let norm_path = normalize_rel_path(&meta.path);
            let full_path = root_path.join(&norm_path);
            if !full_path.exists() {
                missing_files.push(norm_path);
                continue;
            }
            let content = fs::read_to_string(&full_path)?;
            let mut page = parse_markdown_page(&norm_path, &content);
            page.id = meta.id.clone();
            page.path = norm_path;
            if !meta.title.trim().is_empty() {
                page.title = meta.title.clone();
            }
            if !meta.summary.trim().is_empty() {
                page.summary = meta.summary.clone();
            }
            if !meta.category.trim().is_empty() {
                page.category = meta.category.clone();
            }
            if !meta.aliases.is_empty() {
                for alias in &meta.aliases {
                    if !page
                        .aliases
                        .iter()
                        .any(|a| a.eq_ignore_ascii_case(alias))
                    {
                        page.aliases.push(alias.clone());
                    }
                }
            }
            engine.index.add_page(page);
        }

        Ok(Self {
            root: root_path,
            command_prefix: prefix,
            manifest,
            engine,
            missing_files,
        })
    }

    /// Save the current manifest to `<root>/agent-man.json` and reload the workspace index.
    pub fn save_manifest_and_reload(&mut self) -> Result<(), WorkspaceError> {
        let manifest_path = self.root.join(MANIFEST_FILENAME);
        let json_str = serde_json::to_string_pretty(&self.manifest)? + "\n";
        fs::write(&manifest_path, json_str)?;
        self.reload()?;
        self.sync_summary_if_present()?;
        Ok(())
    }

    /// Reload workspace state from disk.
    pub fn reload(&mut self) -> Result<(), WorkspaceError> {
        let reloaded = Self::load(&self.root, self.command_prefix.clone())?;
        *self = reloaded;
        Ok(())
    }

    pub fn page_meta(&self, id_or_alias: &str) -> Option<&PageMeta> {
        let page = self.engine.index.lookup_exact(id_or_alias)?;
        self.manifest
            .pages
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(&page.id))
    }

    pub fn page_meta_mut(&mut self, id_or_alias: &str) -> Option<&mut PageMeta> {
        let target_id = self
            .engine
            .index
            .lookup_exact(id_or_alias)
            .map(|p| p.id.clone())
            .unwrap_or_else(|| id_or_alias.to_string());
        self.manifest
            .pages
            .iter_mut()
            .find(|m| m.id.eq_ignore_ascii_case(&target_id))
    }

    /// Auto-generate the top-level workspace overview document from `agent-man.json`.
    pub fn overview(&self, mode: RenderMode) -> String {
        if mode == RenderMode::Json {
            return serde_json::to_string_pretty(&self.manifest).unwrap_or_default();
        }

        let mut out = String::new();
        if mode == RenderMode::Terminal {
            out.push_str(&format!("\x1b[1;36m# {}\x1b[0m\n", self.manifest.name));
        } else {
            out.push_str(&format!("# {}\n", self.manifest.name));
        }

        if !self.manifest.description.trim().is_empty() {
            out.push_str(&format!("\n{}\n", self.manifest.description.trim()));
        }
        out.push('\n');

        let mut matched_ids = HashSet::new();

        // 1. Pinned categories first, then Normal categories, then Folded categories
        for vis_pass in [
            CategoryVisibility::Pinned,
            CategoryVisibility::Normal,
            CategoryVisibility::Folded,
        ] {
            for cat in &self.manifest.categories {
                if cat.visibility != vis_pass {
                    continue;
                }
                let pages: Vec<&PageMeta> = self
                    .manifest
                    .pages
                    .iter()
                    .filter(|p| p.category.eq_ignore_ascii_case(&cat.key))
                    .collect();

                if pages.is_empty() {
                    continue;
                }

                for p in &pages {
                    matched_ids.insert(p.id.to_lowercase());
                }

                match cat.visibility {
                    CategoryVisibility::Pinned | CategoryVisibility::Normal => {
                        let badge = if cat.visibility == CategoryVisibility::Pinned {
                            " [Must Read]"
                        } else {
                            ""
                        };
                        out.push_str(&format!("## {}{}\n", cat.heading, badge));
                        for p in pages {
                            out.push_str(&self.format_overview_entry(p, mode));
                        }
                        out.push('\n');
                    }
                    CategoryVisibility::Folded => {
                        out.push_str(&format!(
                            "## {} ({} entries folded)\n",
                            cat.heading,
                            pages.len()
                        ));
                        if mode == RenderMode::Mcp {
                            out.push_str(&format!(
                                "  * Historical/secondary entries are hidden by default. Call `doc_list {{\"category\": \"{}\"}}` to expand.\n\n",
                                cat.key
                            ));
                        } else {
                            out.push_str(&format!(
                                "  * Historical/secondary entries are hidden by default. Run `{} list --category {}` to expand.\n\n",
                                self.command_prefix, cat.key
                            ));
                        }
                    }
                }
            }
        }

        // Uncategorized pages
        let others: Vec<&PageMeta> = self
            .manifest
            .pages
            .iter()
            .filter(|p| !matched_ids.contains(&p.id.to_lowercase()))
            .collect();
        if !others.is_empty() {
            out.push_str("## Other Topics\n");
            for p in others {
                out.push_str(&self.format_overview_entry(p, mode));
            }
            out.push('\n');
        }

        // Quick health warning if broken links or missing files exist
        let broken_count = self.engine.broken_links().len();
        if !self.missing_files.is_empty() || broken_count > 0 {
            out.push_str(&format!(
                "> Notice: Workspace has {} missing file(s) and {} broken link(s). {}\n\n",
                self.missing_files.len(),
                broken_count,
                if mode == RenderMode::Mcp {
                    "Call tool `doc_lint {}` for details.".to_string()
                } else {
                    format!("Run `{} lint` for details.", self.command_prefix)
                }
            ));
        }

        // Affordance footer
        out.push_str("---\n### Navigation & Editing Affordances\n");
        if mode == RenderMode::Mcp {
            out.push_str(
                "- **Read topic**: `doc_read {\"id\": \"<id>\"}`\n\
                 - **List folded category**: `doc_list {\"category\": \"<category>\"}`\n\
                 - **Inspect raw lines (before editing)**: `doc_lines {\"id\": \"<id>\"}`\n\
                 - **Partial edit (`[readwrite]` pages)**: `doc_insert`, `doc_replace`, `doc_delete_lines`\n\
                 - **Manage pages**: `doc_create`, `doc_meta`, `doc_remove`, `doc_lint`\n",
            );
        } else {
            let pfx = &self.command_prefix;
            out.push_str(&format!(
                "- **Read topic**: `{pfx} guide <id>`\n\
                 - **List category / folded records**: `{pfx} list --category <key>`\n\
                 - **Inspect line numbers**: `{pfx} lines <id> [--start <N> --end <M>]`\n\
                 - **Edit `[readwrite]` page**:\n\
                   - `{pfx} insert <id> [--line <N>] --content \"<text>\"`\n\
                   - `{pfx} replace <id> --start <N> --end <M> --content \"<text>\"`\n\
                   - `{pfx} delete-lines <id> --start <N> --end <M>`\n\
                 - **Create / maintain pages**: `{pfx} create ...`, `{pfx} meta ...`, `{pfx} remove <id>`, `{pfx} lint`\n"
            ));
        }

        out
    }

    fn format_overview_entry(&self, p: &PageMeta, mode: RenderMode) -> String {
        match mode {
            RenderMode::Mcp => {
                format!(
                    "  * **{}** (`[{}]`): {} — {} (tool: `doc_read {{\"id\": \"{}\"}}`)\n",
                    p.id, p.access, p.title, p.summary, p.id
                )
            }
            RenderMode::Terminal => {
                let cmd = format!("{} guide {}", self.command_prefix, p.id);
                format!(
                    "  * \x1b[1m{}\x1b[0m [{}]: {} — {} \x1b[90m(`{}`)\x1b[0m\n",
                    p.id, p.access, p.title, p.summary, cmd
                )
            }
            _ => {
                let cmd = format!("{} guide {}", self.command_prefix, p.id);
                format!(
                    "  * **{}** [{}]: {} — {} (`{}`)\n",
                    p.id, p.access, p.title, p.summary, cmd
                )
            }
        }
    }

    /// List pages, optionally filtered by `category` or expanding folded categories.
    pub fn list_pages(
        &self,
        category: Option<&str>,
        include_folded: bool,
        mode: RenderMode,
    ) -> String {
        if category.is_none() && !include_folded {
            return self.overview(mode);
        }

        let pages: Vec<&PageMeta> = self
            .manifest
            .pages
            .iter()
            .filter(|p| {
                if let Some(cat) = category {
                    p.category.eq_ignore_ascii_case(cat)
                } else {
                    true
                }
            })
            .collect();

        if mode == RenderMode::Json {
            return serde_json::to_string_pretty(&pages).unwrap_or_default();
        }

        if pages.is_empty() {
            return match category {
                Some(c) => format!("No pages found in category '{c}'.\n"),
                None => "No pages registered in workspace.\n".to_string(),
            };
        }

        let mut out = match category {
            Some(c) => format!("# Topics in category '{c}'\n\n"),
            None => format!("# All Topics in {}\n\n", self.manifest.name),
        };

        for p in pages {
            out.push_str(&self.format_overview_entry(p, mode));
        }

        out
    }

    /// Read a documentation page (or section) with metadata header, backlinks, and edit affordances.
    pub fn read_page(
        &self,
        query: &str,
        section: Option<&str>,
        mode: RenderMode,
    ) -> Result<String, WorkspaceError> {
        let candidates = self.engine.index.find_candidates(query);
        if candidates.is_empty() {
            return Err(WorkspaceError::PageNotFound(
                query.to_string(),
                self.command_prefix.clone(),
            ));
        }

        if candidates.len() > 1 {
            let mut out = format!("Multiple topics matched '{query}':\n\n");
            for c in candidates {
                let cmd = format!("{} guide {}", self.command_prefix, c.id);
                out.push_str(&format!("  * **{}**: {} (`{}`)\n", c.id, c.summary, cmd));
            }
            return Ok(out);
        }

        let page = candidates[0];
        let meta = self.page_meta(&page.id);
        let access = meta.map(|m| m.access).unwrap_or(AccessMode::ReadWrite);

        if mode == RenderMode::Json {
            let backlinks: Vec<String> = self
                .engine
                .index
                .backlinks(&page.id)
                .into_iter()
                .map(|p| p.id.clone())
                .collect();
            let val = serde_json::json!({
                "id": page.id,
                "path": page.path,
                "title": page.title,
                "summary": page.summary,
                "category": page.category,
                "access": access,
                "aliases": page.aliases,
                "backlinks": backlinks,
                "sections": page.sections,
                "raw_content": page.raw_content,
            });
            return Ok(serde_json::to_string_pretty(&val)?);
        }

        let renderer = DocRenderer::new(
            &self.command_prefix,
            &self.engine.guide_subcommand,
            &self.engine.explain_subcommand,
            &self.engine.index,
        )
        .with_scheme(&self.engine.scheme);

        let body = if let Some(sec_query) = section {
            let sec = page
                .sections
                .iter()
                .find(|s| {
                    s.anchor.eq_ignore_ascii_case(sec_query)
                        || crate::parser::slugify(&s.heading).eq_ignore_ascii_case(sec_query)
                })
                .ok_or_else(|| {
                    WorkspaceError::Other(format!(
                        "Section '{sec_query}' not found in topic '{}'",
                        page.id
                    ))
                })?;
            let rewritten = renderer.rewrite_links(&page.path, &sec.content, mode);
            format!("## {}\n\n{}\n", sec.heading, rewritten)
        } else {
            renderer.render_page(page, mode)
        };

        let mut out = format!(
            "[id: {} | category: {} | access: {} | path: {}]\n\n{}",
            page.id, page.category, access, page.path, body
        );

        // Append bidirectional backlinks
        let backlinks = self.engine.index.backlinks(&page.id);
        if !backlinks.is_empty() {
            out.push_str("\n---\nReferenced by (Backlinks):\n");
            for b in backlinks {
                if mode == RenderMode::Mcp {
                    out.push_str(&format!(
                        "- **{}** ({}) — tool: `doc_read {{\"id\": \"{}\"}}`\n",
                        b.id, b.title, b.id
                    ));
                } else {
                    out.push_str(&format!(
                        "- **{}** ({}) — see: '{} guide {}'\n",
                        b.id, b.title, self.command_prefix, b.id
                    ));
                }
            }
        }

        // Append edit affordance hint
        out.push_str("\n---\n");
        match access {
            AccessMode::ReadWrite => {
                if mode == RenderMode::Mcp {
                    out.push_str(&format!(
                        "Affordance: Page '{}' is [readwrite]. To view line numbers or edit, call `doc_lines {{\"id\": \"{}\"}}`.\n",
                        page.id, page.id
                    ));
                } else {
                    out.push_str(&format!(
                        "Affordance: Page '{}' is [readwrite]. To view line numbers or edit, run '{} lines {}'.\n",
                        page.id, self.command_prefix, page.id
                    ));
                }
            }
            AccessMode::ReadOnly => {
                out.push_str(&format!(
                    "Affordance: Page '{}' is [readonly] (protected by {}).\n",
                    page.id, MANIFEST_FILENAME
                ));
            }
        }

        Ok(out)
    }

    /// Return raw file content with 1-indexed line numbers for inspection before editing.
    pub fn lines(
        &self,
        id: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
    ) -> Result<String, WorkspaceError> {
        let page = self.engine.index.lookup_exact(id).ok_or_else(|| {
            WorkspaceError::PageNotFound(id.to_string(), self.command_prefix.clone())
        })?;
        let access = self
            .page_meta(&page.id)
            .map(|m| m.access)
            .unwrap_or(AccessMode::ReadWrite);

        let lines: Vec<&str> = page.source_content.lines().collect();
        let total = lines.len();

        if total == 0 {
            return Ok(format!(
                "[id: {} | path: {} | access: {} | total_lines: 0]\n(empty file)\n",
                page.id, page.path, access
            ));
        }

        let start = start_line.unwrap_or(1).max(1);
        let end = end_line.unwrap_or(total).min(total);
        if start > end || start > total {
            return Err(WorkspaceError::InvalidLineRange(format!(
                "Requested range {start}..={end} is outside valid lines 1..={total}"
            )));
        }

        let mut out = format!(
            "[id: {} | path: {} | access: {} | lines: {}..={} of {}]\n",
            page.id, page.path, access, start, end, total
        );

        for (idx, line) in lines.iter().enumerate().take(end).skip(start - 1) {
            let line_num = idx + 1;
            out.push_str(&format!("{line_num:>4} | {line}\n"));
        }

        if access == AccessMode::ReadWrite {
            out.push_str(&format!(
                "\n---\nAffordance: Use 1-indexed line numbers above with `insert {}`, `replace {}`, or `delete-lines {}`.\n",
                page.id, page.id, page.id
            ));
        }

        Ok(out)
    }

    /// Insert `content` before `line` (1-indexed), or append at the end if `line` is `None`.
    pub fn insert(
        &mut self,
        id: &str,
        line: Option<usize>,
        content: &str,
    ) -> Result<String, WorkspaceError> {
        let (page_id, rel_path, source) = self.require_writable_page(id)?;
        let mut lines: Vec<String> = source.lines().map(String::from).collect();
        let old_total = lines.len();

        let insert_idx = match line {
            Some(l) => {
                if l == 0 || l > old_total + 1 {
                    return Err(WorkspaceError::InvalidLineRange(format!(
                        "Insert line {l} is out of bounds (valid: 1..={})",
                        old_total + 1
                    )));
                }
                l - 1
            }
            None => old_total,
        };

        let new_lines: Vec<String> = split_preserve_lines(content);
        let inserted_count = new_lines.len();

        lines.splice(insert_idx..insert_idx, new_lines);
        self.write_page_lines(&rel_path, &lines)?;

        let start_changed = insert_idx + 1;
        let end_changed = insert_idx + inserted_count.max(1);
        Ok(self.format_edit_feedback(
            &page_id,
            &format!(
                "Inserted {} line(s) at line {}",
                inserted_count, start_changed
            ),
            old_total,
            lines.len(),
            start_changed,
            end_changed,
        ))
    }

    /// Replace 1-indexed inclusive line range `start_line..=end_line` with `content`.
    pub fn replace(
        &mut self,
        id: &str,
        start_line: usize,
        end_line: usize,
        content: &str,
    ) -> Result<String, WorkspaceError> {
        let (page_id, rel_path, source) = self.require_writable_page(id)?;
        let mut lines: Vec<String> = source.lines().map(String::from).collect();
        let old_total = lines.len();

        if start_line == 0 || start_line > end_line || end_line > old_total {
            return Err(WorkspaceError::InvalidLineRange(format!(
                "Replace range {start_line}..={end_line} is out of bounds (valid: 1..={old_total})"
            )));
        }

        let new_lines: Vec<String> = split_preserve_lines(content);
        let replacement_count = new_lines.len();

        lines.splice((start_line - 1)..end_line, new_lines);
        self.write_page_lines(&rel_path, &lines)?;

        let end_changed = if replacement_count == 0 {
            start_line
        } else {
            start_line + replacement_count - 1
        };

        Ok(self.format_edit_feedback(
            &page_id,
            &format!(
                "Replaced lines {start_line}..={end_line} with {replacement_count} line(s)"
            ),
            old_total,
            lines.len(),
            start_line,
            end_changed,
        ))
    }

    /// Delete 1-indexed inclusive line range `start_line..=end_line`.
    pub fn delete_lines(
        &mut self,
        id: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<String, WorkspaceError> {
        let (page_id, rel_path, source) = self.require_writable_page(id)?;
        let mut lines: Vec<String> = source.lines().map(String::from).collect();
        let old_total = lines.len();

        if start_line == 0 || start_line > end_line || end_line > old_total {
            return Err(WorkspaceError::InvalidLineRange(format!(
                "Delete range {start_line}..={end_line} is out of bounds (valid: 1..={old_total})"
            )));
        }

        let removed_count = end_line - start_line + 1;
        lines.drain((start_line - 1)..end_line);
        self.write_page_lines(&rel_path, &lines)?;

        let focus_line = start_line.min(lines.len().max(1));
        Ok(self.format_edit_feedback(
            &page_id,
            &format!("Deleted {removed_count} line(s) ({start_line}..={end_line})"),
            old_total,
            lines.len(),
            focus_line,
            focus_line,
        ))
    }

    /// Create a new documentation page on disk and register it in `agent-man.json` as `readwrite`.
    pub fn create_page(&mut self, req: CreatePageRequest) -> Result<String, WorkspaceError> {
        let trimmed_id = req.id.trim().to_string();
        if trimmed_id.is_empty() {
            return Err(WorkspaceError::Other("Page 'id' cannot be empty".into()));
        }

        if self.engine.index.lookup_exact(&trimmed_id).is_some()
            || self
                .manifest
                .pages
                .iter()
                .any(|p| p.id.eq_ignore_ascii_case(&trimmed_id))
        {
            return Err(WorkspaceError::PageAlreadyExists(trimmed_id));
        }

        let category = req
            .category
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("guide")
            .to_string();

        let rel_path = match req.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(p) => validate_relative_doc_path(p)?,
            None => default_path_for_category(&category, &trimmed_id),
        };

        let full_path = self.root.join(&rel_path);
        if full_path.exists() {
            return Err(WorkspaceError::Other(format!(
                "File '{}' already exists on disk. Run '{} init' to register existing files.",
                rel_path, self.command_prefix
            )));
        }

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut body = req.content.trim_end().to_string();
        if !body.lines().any(|l| l.trim().starts_with("# ")) {
            if body.is_empty() {
                body = format!("# {}\n\n{}", req.title.trim(), req.summary.trim());
            } else {
                body = format!("# {}\n\n{}", req.title.trim(), body);
            }
        }
        body.push('\n');

        fs::write(&full_path, &body)?;

        self.manifest.pages.push(PageMeta {
            id: trimmed_id.clone(),
            path: rel_path.clone(),
            title: req.title.trim().to_string(),
            summary: req.summary.trim().to_string(),
            category: category.clone(),
            access: AccessMode::ReadWrite,
            aliases: req.aliases,
        });

        self.save_manifest_and_reload()?;

        let total_lines = body.lines().count();
        Ok(self.format_edit_feedback(
            &trimmed_id,
            &format!("Created new [readwrite] page '{trimmed_id}' at '{rel_path}'"),
            0,
            total_lines,
            1,
            total_lines.min(10),
        ))
    }

    /// Remove a `readwrite` page from disk and `agent-man.json`.
    pub fn remove_page(&mut self, id: &str, force: bool) -> Result<String, WorkspaceError> {
        let (page_id, rel_path, _) = self.require_writable_page(id)?;

        let backlinks: Vec<String> = self
            .engine
            .index
            .backlinks(&page_id)
            .into_iter()
            .map(|p| format!("'{}' ({})", p.id, p.path))
            .collect();

        if !backlinks.is_empty() && !force {
            return Err(WorkspaceError::HasBacklinks(
                page_id,
                backlinks.join(", "),
            ));
        }

        let full_path = self.root.join(&rel_path);
        if full_path.exists() {
            fs::remove_file(&full_path)?;
        }

        self.manifest
            .pages
            .retain(|p| !p.id.eq_ignore_ascii_case(&page_id));
        self.save_manifest_and_reload()?;

        Ok(format!(
            "Removed page '{page_id}' ({rel_path}) and updated {MANIFEST_FILENAME}."
        ))
    }

    /// Update metadata (`title`, `summary`, `category`, `aliases`) for a `readwrite` page.
    /// Note: `access` (`readonly` / `readwrite`) is intentionally locked and cannot be modified here.
    pub fn update_meta(
        &mut self,
        id: &str,
        title: Option<String>,
        summary: Option<String>,
        category: Option<String>,
        aliases: Option<Vec<String>>,
    ) -> Result<String, WorkspaceError> {
        let (page_id, _, _) = self.require_writable_page(id)?;
        let prefix = self.command_prefix.clone();
        let meta = self
            .page_meta_mut(&page_id)
            .ok_or_else(|| WorkspaceError::PageNotFound(page_id.clone(), prefix))?;

        if let Some(t) = title {
            if !t.trim().is_empty() {
                meta.title = t.trim().to_string();
            }
        }
        if let Some(s) = summary {
            if !s.trim().is_empty() {
                meta.summary = s.trim().to_string();
            }
        }
        if let Some(c) = category {
            if !c.trim().is_empty() {
                meta.category = c.trim().to_string();
            }
        }
        if let Some(a) = aliases {
            meta.aliases = a;
        }

        let updated_summary = format!(
            "Updated metadata for '{}' (title: '{}', category: '{}', summary: '{}').",
            meta.id, meta.title, meta.category, meta.summary
        );

        self.save_manifest_and_reload()?;
        Ok(updated_summary)
    }

    /// Run workspace integrity checks: missing files, unregistered `.md` files, broken links, and orphan pages.
    pub fn lint(&self) -> LintReport {
        let mut md_files = Vec::new();
        let _ = collect_workspace_md_files(&self.root, &self.root, &mut md_files);
        md_files.sort();

        let registered_paths: HashSet<String> = self
            .manifest
            .pages
            .iter()
            .map(|p| normalize_rel_path(&p.path))
            .collect();

        let unregistered_files: Vec<String> = md_files
            .into_iter()
            .filter(|f| !registered_paths.contains(&normalize_rel_path(f)))
            .collect();

        let broken_links = self.engine.broken_links();

        let pinned_keys: HashSet<String> = self
            .manifest
            .categories
            .iter()
            .filter(|c| c.visibility == CategoryVisibility::Pinned)
            .map(|c| c.key.to_lowercase())
            .collect();

        let mut orphan_pages = Vec::new();
        if self.manifest.pages.len() > 1 {
            for meta in &self.manifest.pages {
                if pinned_keys.contains(&meta.category.to_lowercase()) {
                    continue;
                }
                if self.engine.index.backlinks(&meta.id).is_empty() {
                    orphan_pages.push(meta.id.clone());
                }
            }
        }

        LintReport {
            missing_files: self.missing_files.clone(),
            unregistered_files,
            broken_links,
            orphan_pages,
        }
    }

    /// Generate mdBook `SUMMARY.md` content from `agent-man.json`.
    pub fn summary_md(&self) -> String {
        let mut out = String::from("# Summary\n\n");
        let mut matched_ids = HashSet::new();

        for vis in [
            CategoryVisibility::Pinned,
            CategoryVisibility::Normal,
            CategoryVisibility::Folded,
        ] {
            for cat in &self.manifest.categories {
                if cat.visibility != vis {
                    continue;
                }
                let pages: Vec<&PageMeta> = self
                    .manifest
                    .pages
                    .iter()
                    .filter(|p| p.category.eq_ignore_ascii_case(&cat.key))
                    .collect();

                if pages.is_empty() {
                    continue;
                }

                out.push_str(&format!("# {}\n\n", cat.heading));
                for p in pages {
                    matched_ids.insert(p.id.to_lowercase());
                    out.push_str(&format!(
                        "- [{}]({})\n",
                        p.title,
                        normalize_rel_path(&p.path)
                    ));
                }
                out.push('\n');
            }
        }

        let others: Vec<&PageMeta> = self
            .manifest
            .pages
            .iter()
            .filter(|p| !matched_ids.contains(&p.id.to_lowercase()))
            .collect();
        if !others.is_empty() {
            out.push_str("# Other Topics\n\n");
            for p in others {
                out.push_str(&format!(
                    "- [{}]({})\n",
                    p.title,
                    normalize_rel_path(&p.path)
                ));
            }
            out.push('\n');
        }

        out
    }

    /// Write `<root>/SUMMARY.md` for mdBook / GitHub Pages.
    pub fn write_summary_md(&self) -> Result<PathBuf, WorkspaceError> {
        let summary_path = self.root.join("SUMMARY.md");
        fs::write(&summary_path, self.summary_md())?;
        Ok(summary_path)
    }

    fn sync_summary_if_present(&self) -> Result<(), WorkspaceError> {
        let summary_path = self.root.join("SUMMARY.md");
        if summary_path.exists() {
            fs::write(&summary_path, self.summary_md())?;
        }
        Ok(())
    }

    fn require_writable_page(
        &self,
        id: &str,
    ) -> Result<(String, String, String), WorkspaceError> {
        let page = self.engine.index.lookup_exact(id).ok_or_else(|| {
            WorkspaceError::PageNotFound(id.to_string(), self.command_prefix.clone())
        })?;
        let meta = self.page_meta(&page.id).ok_or_else(|| {
            WorkspaceError::PageNotFound(id.to_string(), self.command_prefix.clone())
        })?;

        if meta.access == AccessMode::ReadOnly {
            return Err(WorkspaceError::ReadOnly(page.id.clone()));
        }

        Ok((
            page.id.clone(),
            meta.path.clone(),
            page.source_content.clone(),
        ))
    }

    fn write_page_lines(&mut self, rel_path: &str, lines: &[String]) -> Result<(), WorkspaceError> {
        let full_path = self.root.join(rel_path);
        let mut content = lines.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        fs::write(&full_path, content)?;
        self.reload()?;
        Ok(())
    }

    fn format_edit_feedback(
        &self,
        page_id: &str,
        action_summary: &str,
        old_total: usize,
        new_total: usize,
        focus_start: usize,
        focus_end: usize,
    ) -> String {
        let mut out = format!(
            "{action_summary} (lines: {old_total} -> {new_total}).\n"
        );

        if let Some(page) = self.engine.index.lookup_exact(page_id) {
            let lines: Vec<&str> = page.source_content.lines().collect();
            let total = lines.len();
            if total > 0 {
                let ctx_start = focus_start.saturating_sub(2).max(1).min(total);
                let ctx_end = (focus_end + 2).min(total).max(ctx_start);
                out.push_str(&format!(
                    "\nUpdated line context ({}..={} of {}):\n",
                    ctx_start, ctx_end, total
                ));
                for (idx, line) in lines
                    .iter()
                    .enumerate()
                    .take(ctx_end)
                    .skip(ctx_start - 1)
                {
                    let line_num = idx + 1;
                    out.push_str(&format!("{line_num:>4} | {line}\n"));
                }
            }

            // Check broken links specifically on this page
            let page_broken: Vec<BrokenLink> = self
                .engine
                .broken_links()
                .into_iter()
                .filter(|b| b.page_id.eq_ignore_ascii_case(page_id))
                .collect();

            if !page_broken.is_empty() {
                out.push_str("\nAffordance Warning — Broken links detected in this page:\n");
                for b in page_broken {
                    out.push_str(&format!(
                        "  - Line {}: '[{}]({})' does not resolve to any registered page or anchor.\n",
                        b.line, b.text, b.target
                    ));
                }
            }

            // Check if page is an orphan
            let is_pinned = self
                .manifest
                .categories
                .iter()
                .any(|c| {
                    c.key.eq_ignore_ascii_case(&page.category)
                        && c.visibility == CategoryVisibility::Pinned
                });
            if !is_pinned
                && self.manifest.pages.len() > 1
                && self.engine.index.backlinks(&page.id).is_empty()
            {
                out.push_str(&format!(
                    "\nAffordance Hint: Page '{}' has no incoming links from other pages yet. Consider adding a relative link `[{}]({})` from a related guide or entrypoint.\n",
                    page.id, page.title, page.path
                ));
            }
        }

        out
    }
}

/// Process mdBook preprocessor JSON payload from `stdin`:
/// strips leading YAML frontmatter (`--- ... ---`) from every chapter.
pub fn preprocess_mdbook_json(raw_input: &str) -> Result<String, WorkspaceError> {
    let mut data: serde_json::Value = serde_json::from_str(raw_input)?;
    let book_obj = if let Some(arr) = data.as_array_mut() {
        if arr.len() >= 2 {
            &mut arr[1]
        } else {
            return Err(WorkspaceError::Other(
                "Invalid mdBook preprocessor array input".into(),
            ));
        }
    } else {
        &mut data
    };

    let key = if book_obj.get("items").is_some() {
        "items"
    } else {
        "sections"
    };
    if let Some(items) = book_obj.get_mut(key).and_then(|v| v.as_array_mut()) {
        for item in items {
            strip_frontmatter_in_mdbook_item(item);
        }
    }

    Ok(serde_json::to_string(book_obj)?)
}

fn strip_frontmatter_in_mdbook_item(item: &mut serde_json::Value) {
    if let Some(chapter) = item.get_mut("Chapter") {
        if let Some(content_val) = chapter.get_mut("content") {
            if let Some(content_str) = content_val.as_str() {
                *content_val = serde_json::Value::String(strip_yaml_frontmatter(content_str));
            }
        }
        if let Some(subs) = chapter.get_mut("sub_items").and_then(|v| v.as_array_mut()) {
            for sub in subs {
                strip_frontmatter_in_mdbook_item(sub);
            }
        }
    }
}

/// Strip leading YAML frontmatter (`---\n...\n---\n`) from a Markdown string.
pub fn strip_yaml_frontmatter(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first().map(|l| l.trim()) == Some("---") {
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                let rest = lines[i + 1..].join("\n");
                return rest.trim_start_matches('\n').to_string() + "\n";
            }
        }
    }
    content.to_string()
}

fn split_preserve_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(String::from).collect()
    }
}

fn collect_workspace_md_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    if !current.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            collect_workspace_md_files(root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if matches!(
                name_str.as_ref(),
                "SUMMARY.md" | "SKILL.md" | "llms.txt"
            ) {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(normalize_rel_path(&rel.to_string_lossy()));
            }
        }
    }
    Ok(())
}

fn normalize_rel_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn validate_relative_doc_path(path: &str) -> Result<String, WorkspaceError> {
    let norm = normalize_rel_path(path);
    let p = Path::new(&norm);
    if p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(WorkspaceError::InvalidPath(path.to_string()));
    }
    let with_ext = if norm.ends_with(".md") {
        norm
    } else {
        format!("{norm}.md")
    };
    Ok(with_ext)
}

fn default_path_for_category(category: &str, id: &str) -> String {
    match category.to_lowercase().as_str() {
        "entrypoint" | "intro" => format!("{id}.md"),
        "record" | "records" | "log" | "logs" => format!("records/{id}.md"),
        "rule" | "rules" => format!("rules/{id}.md"),
        "guide" | "guides" => format!("guides/{id}.md"),
        other => format!("{other}/{id}.md"),
    }
}

fn infer_category_for_init(rel_path: &str, parsed_category: &str) -> String {
    let lower_path = rel_path.to_lowercase();
    if lower_path == "overview.md"
        || lower_path == "readme.md"
        || lower_path == "index.md"
        || lower_path.starts_with("entrypoints/")
    {
        return "entrypoint".to_string();
    }
    if lower_path.starts_with("records/")
        || lower_path.starts_with("logs/")
        || lower_path.starts_with("adr/")
        || lower_path.starts_with("history/")
    {
        return "record".to_string();
    }
    if lower_path.starts_with("rules/") {
        return "rule".to_string();
    }
    if parsed_category != "general" && !parsed_category.is_empty() {
        return parsed_category.to_string();
    }
    "guide".to_string()
}

/// Build an MCP-enabled `DocEngine` backed dynamically by a filesystem `Workspace`.
#[cfg(feature = "mcp")]
pub fn build_workspace_mcp_engine(
    root: impl AsRef<Path>,
    command_prefix: impl Into<String>,
) -> Result<DocEngine, WorkspaceError> {
    use crate::mcp::{ParamType, Tool, ToolResult};

    let root_path = root.as_ref().to_path_buf();
    let prefix = command_prefix.into();
    let ws = Workspace::load(&root_path, &prefix)?;

    let mut engine = ws.engine.clone();
    engine.workspace_root = Some(root_path.clone());
    engine.workspace_prefix = Some(prefix.clone());

    // 1. doc_lines
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_lines",
            "View raw markdown lines with 1-indexed line numbers before making line edits",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param(
            "start_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "Optional 1-indexed start line",
            false,
        )
        .param(
            "end_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "Optional 1-indexed end line (inclusive)",
            false,
        )
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let start = args["start_line"].as_u64().map(|n| n as usize);
            let end = args["end_line"].as_u64().map(|n| n as usize);
            match Workspace::load(&r, &p).and_then(|w| w.lines(id, start, end)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 2. doc_insert
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_insert",
            "Insert new line(s) into a [readwrite] page before a 1-indexed line number (or append at the end if 'line' is omitted)",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param(
            "line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "1-indexed line number to insert before (omit to append to end of file)",
            false,
        )
        .param(
            "content",
            ParamType::String,
            "Markdown text to insert (can span multiple lines)",
            true,
        )
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let line = args["line"].as_u64().map(|n| n as usize);
            let content = args["content"].as_str().unwrap_or("");
            match Workspace::load(&r, &p).and_then(|mut w| w.insert(id, line, content)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 3. doc_replace
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_replace",
            "Replace a 1-indexed inclusive line range (start_line..=end_line) in a [readwrite] page",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param(
            "start_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "1-indexed start line (inclusive)",
            true,
        )
        .param(
            "end_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "1-indexed end line (inclusive)",
            true,
        )
        .param(
            "content",
            ParamType::String,
            "Replacement markdown text",
            true,
        )
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let start = args["start_line"].as_u64().unwrap_or(0) as usize;
            let end = args["end_line"].as_u64().unwrap_or(0) as usize;
            let content = args["content"].as_str().unwrap_or("");
            match Workspace::load(&r, &p).and_then(|mut w| w.replace(id, start, end, content)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 4. doc_delete_lines
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_delete_lines",
            "Delete a 1-indexed inclusive line range (start_line..=end_line) from a [readwrite] page",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param(
            "start_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "1-indexed start line (inclusive)",
            true,
        )
        .param(
            "end_line",
            ParamType::Integer {
                min: Some(1),
                max: None,
            },
            "1-indexed end line (inclusive)",
            true,
        )
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let start = args["start_line"].as_u64().unwrap_or(0) as usize;
            let end = args["end_line"].as_u64().unwrap_or(0) as usize;
            match Workspace::load(&r, &p).and_then(|mut w| w.delete_lines(id, start, end)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 5. doc_create
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_create",
            "Create a new [readwrite] documentation page and register it in agent-man.json",
        )
        .param("id", ParamType::String, "Unique page identifier (e.g. 'auth-flow')", true)
        .param("title", ParamType::String, "Display title of the page", true)
        .param("summary", ParamType::String, "One-line summary of the page", true)
        .param(
            "category",
            ParamType::String,
            "Category key (e.g. 'guide', 'record'); defaults to 'guide'",
            false,
        )
        .param(
            "path",
            ParamType::String,
            "Optional relative file path (defaults to '<category>/<id>.md')",
            false,
        )
        .param(
            "aliases",
            ParamType::StringArray,
            "Optional lookup aliases",
            false,
        )
        .param("content", ParamType::String, "Initial markdown body content", true)
        .handler(move |args| {
            let req = CreatePageRequest {
                id: args["id"].as_str().unwrap_or("").to_string(),
                title: args["title"].as_str().unwrap_or("").to_string(),
                summary: args["summary"].as_str().unwrap_or("").to_string(),
                category: args["category"].as_str().map(String::from),
                path: args["path"].as_str().map(String::from),
                aliases: args["aliases"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                content: args["content"].as_str().unwrap_or("").to_string(),
            };
            match Workspace::load(&r, &p).and_then(|mut w| w.create_page(req)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 6. doc_meta
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_meta",
            "Update metadata (title, summary, category, aliases) of a [readwrite] page in agent-man.json",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param("title", ParamType::String, "New display title", false)
        .param("summary", ParamType::String, "New one-line summary", false)
        .param("category", ParamType::String, "New category key", false)
        .param("aliases", ParamType::StringArray, "New list of aliases", false)
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let title = args["title"].as_str().map(String::from);
            let summary = args["summary"].as_str().map(String::from);
            let category = args["category"].as_str().map(String::from);
            let aliases = args["aliases"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });
            match Workspace::load(&r, &p)
                .and_then(|mut w| w.update_meta(id, title, summary, category, aliases))
            {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 7. doc_remove
    {
        let r = root_path.clone();
        let p = prefix.clone();
        let tool = Tool::builder(
            "doc_remove",
            "Remove a [readwrite] page from disk and agent-man.json (refuses if backlinks exist unless force is true)",
        )
        .param("id", ParamType::String, "Documentation page ID", true)
        .param(
            "force",
            ParamType::Boolean,
            "Force removal even if other pages link to this page",
            false,
        )
        .handler(move |args| {
            let id = args["id"].as_str().unwrap_or("");
            let force = args["force"].as_bool().unwrap_or(false);
            match Workspace::load(&r, &p).and_then(|mut w| w.remove_page(id, force)) {
                Ok(text) => ToolResult::text(text).into(),
                Err(e) => ToolResult::error(e.to_string()).into(),
            }
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    // 8. doc_lint
    {
        let r = root_path;
        let p = prefix;
        let tool = Tool::builder(
            "doc_lint",
            "Check workspace for broken links, missing files, unregistered .md files, and orphan pages",
        )
        .handler(move |_args| match Workspace::load(&r, &p) {
            Ok(w) => {
                let report = w.lint();
                ToolResult::text(report.format_report(&w.command_prefix)).into()
            }
            Err(e) => ToolResult::error(e.to_string()).into(),
        })
        .build();
        let _ = engine.register_tool(tool);
    }

    Ok(engine)
}
