use crate::model::{DocPage, ResolvedLink, TopicSummary};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct DocIndex {
    pub pages: Vec<DocPage>,
    pub id_map: HashMap<String, usize>,
    pub path_map: HashMap<String, usize>,
}

impl DocIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_page(&mut self, page: DocPage) {
        let idx = self.pages.len();
        let id_lower = page.id.to_lowercase();
        self.id_map.insert(id_lower.clone(), idx);

        for alias in &page.aliases {
            self.id_map.insert(alias.to_lowercase(), idx);
        }

        let normalized_path = normalize_path_str(&page.path);
        self.path_map.insert(normalized_path.clone(), idx);

        let file_stem = Path::new(&page.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !file_stem.is_empty() {
            self.id_map.entry(file_stem).or_insert(idx);
        }

        self.pages.push(page);
    }

    /// Exact match on page id or aliases (case-insensitive).
    pub fn lookup_exact(&self, query: &str) -> Option<&DocPage> {
        let q = query.trim();
        self.pages.iter().find(|page| {
            page.id.eq_ignore_ascii_case(q)
                || page.aliases.iter().any(|a| a.eq_ignore_ascii_case(q))
        })
    }

    /// Search candidates for topic guidance: exact matches first, then substring matches.
    pub fn find_candidates(&self, query: &str) -> Vec<&DocPage> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        if let Some(page) = self.lookup_exact(&q) {
            return vec![page];
        }
        self.pages
            .iter()
            .filter(|page| {
                page.id.to_lowercase().contains(&q)
                    || page.title.to_lowercase().contains(&q)
                    || page.aliases.iter().any(|a| a.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn lookup(&self, query: &str) -> Option<&DocPage> {
        let q = query.trim().to_lowercase();

        // 1. Exact ID or alias or file stem match
        if let Some(&idx) = self.id_map.get(&q) {
            return Some(&self.pages[idx]);
        }

        // 2. Normalized path match
        let q_path = normalize_path_str(&q);
        if let Some(&idx) = self.path_map.get(&q_path) {
            return Some(&self.pages[idx]);
        }

        // 3. Prefix or contains match
        self.pages.iter().find(|page| {
            page.id.to_lowercase().contains(&q)
                || page.title.to_lowercase().contains(&q)
                || page.aliases.iter().any(|a| a.to_lowercase().contains(&q))
        })
    }

    pub fn resolve_link(&self, from_path: &str, target: &str) -> Option<ResolvedLink> {
        if target.starts_with("http://") || target.starts_with("https://") {
            return None;
        }

        let (path_part, anchor) = if let Some((p, a)) = target.split_once('#') {
            (p, Some(a.to_string()))
        } else {
            (target, None)
        };

        if path_part.is_empty() {
            // Anchor link in same page
            if let Some(&idx) = self.path_map.get(&normalize_path_str(from_path)) {
                let page = &self.pages[idx];
                return Some(ResolvedLink {
                    target_id: page.id.clone(),
                    category: page.category.clone(),
                    anchor,
                    label: String::new(),
                });
            }
            return None;
        }

        // Relative path resolution
        let from_dir = Path::new(from_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let resolved_path = normalize_path(&from_dir.join(path_part));
        let resolved_str = resolved_path.to_string_lossy().to_string();

        if let Some(&idx) = self.path_map.get(&resolved_str) {
            let page = &self.pages[idx];
            return Some(ResolvedLink {
                target_id: page.id.clone(),
                category: page.category.clone(),
                anchor,
                label: String::new(),
            });
        }

        // Fallback: check file stem
        if let Some(stem) = Path::new(path_part).file_stem().and_then(|s| s.to_str()) {
            if let Some(&idx) = self.id_map.get(&stem.to_lowercase()) {
                let page = &self.pages[idx];
                return Some(ResolvedLink {
                    target_id: page.id.clone(),
                    category: page.category.clone(),
                    anchor,
                    label: String::new(),
                });
            }
        }

        None
    }

    pub fn list_topics(&self, category_filter: Option<&str>) -> Vec<TopicSummary> {
        self.pages
            .iter()
            .filter(|p| {
                if let Some(cat) = category_filter {
                    p.category.eq_ignore_ascii_case(cat)
                } else {
                    true
                }
            })
            .map(|p| TopicSummary {
                id: p.id.clone(),
                title: p.title.clone(),
                summary: p.summary.clone(),
                category: p.category.clone(),
                aliases: p.aliases.clone(),
            })
            .collect()
    }

    /// Return all pages that contain an internal link resolving to `target_id`.
    pub fn backlinks(&self, target_id: &str) -> Vec<&DocPage> {
        let mut refs = Vec::new();
        for page in &self.pages {
            if page.id.eq_ignore_ascii_case(target_id) {
                continue;
            }
            let links = extract_markdown_links(&page.source_content);
            let links_to_target = links.into_iter().any(|(_line, _label, target)| {
                if target.starts_with("http://")
                    || target.starts_with("https://")
                    || target.starts_with("mailto:")
                {
                    return false;
                }
                if let Some(resolved) = self.resolve_link(&page.path, &target) {
                    resolved.target_id.eq_ignore_ascii_case(target_id)
                } else {
                    false
                }
            });
            if links_to_target {
                refs.push(page);
            }
        }
        refs
    }
}

/// Extract markdown links `(line_num_1_indexed, label, target)` outside fenced code blocks.
pub fn extract_markdown_links(content: &str) -> Vec<(usize, String, String)> {
    let mut links = Vec::new();
    let mut in_fence = false;

    for (idx, line) in content.lines().enumerate() {
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
                            links.push((line_num, label, target));
                            i = tend + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    links
}

fn normalize_path_str(p: &str) -> String {
    normalize_path(Path::new(p)).to_string_lossy().to_string()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::Normal(c) => {
                components.push(c);
            }
            _ => {}
        }
    }
    let mut out = PathBuf::new();
    for c in components {
        out.push(c);
    }
    out
}
