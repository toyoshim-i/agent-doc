use crate::index::DocIndex;
use crate::model::{DocPage, RenderMode, ResolvedLink};

pub struct DocRenderer<'a> {
    pub binary_name: &'a str,
    pub guide_subcommand: &'a str,
    pub explain_subcommand: &'a str,
    pub index: &'a DocIndex,
    pub scheme: &'a str,
}

impl<'a> DocRenderer<'a> {
    pub fn new(
        binary_name: &'a str,
        guide_subcommand: &'a str,
        explain_subcommand: &'a str,
        index: &'a DocIndex,
    ) -> Self {
        Self {
            binary_name,
            guide_subcommand,
            explain_subcommand,
            index,
            scheme: binary_name,
        }
    }

    pub fn with_scheme(mut self, scheme: &'a str) -> Self {
        self.scheme = scheme;
        self
    }

    pub fn render_page(&self, page: &DocPage, mode: RenderMode) -> String {
        match mode {
            RenderMode::Json => serde_json::to_string_pretty(page).unwrap_or_default(),
            RenderMode::Agent => self.render_agent(page),
            RenderMode::Terminal => self.render_terminal(page),
            RenderMode::Mcp => self.render_mcp(page),
        }
    }

    fn render_agent(&self, page: &DocPage) -> String {
        self.render_markdown(page, RenderMode::Agent)
    }

    fn render_mcp(&self, page: &DocPage) -> String {
        self.render_markdown(page, RenderMode::Mcp)
    }

    fn render_markdown(&self, page: &DocPage, mode: RenderMode) -> String {
        let body_rewritten = self.rewrite_links(&page.path, &page.raw_content, mode);
        let has_fm_summary = has_frontmatter_summary(&page.source_content);
        let summary = page.summary.trim();

        if !starts_with_h1(&page.raw_content) {
            let mut out = String::new();
            out.push_str(&format!("# {}\n\n", page.title));
            if has_fm_summary && !summary.is_empty() {
                out.push_str(summary);
                out.push_str("\n\n");
            }
            out.push_str(&body_rewritten);
            out.push('\n');
            out
        } else if has_fm_summary && !summary.is_empty() {
            // Body starts with H1 heading: place summary immediately after the H1 heading
            let lines: Vec<&str> = body_rewritten.lines().collect();
            if let Some(h1_idx) = lines.iter().position(|l| {
                let trimmed = l.trim();
                trimmed.starts_with("# ") || trimmed == "#"
            }) {
                let rest = lines[h1_idx + 1..].join("\n");
                if rest.trim_start().starts_with(summary) {
                    let mut out = body_rewritten;
                    out.push('\n');
                    out
                } else {
                    let mut out = String::new();
                    for line in &lines[..=h1_idx] {
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push('\n');
                    out.push_str(summary);
                    out.push('\n');
                    let rest_trimmed = rest.trim_start_matches('\n');
                    if !rest_trimmed.is_empty() {
                        out.push('\n');
                        out.push_str(rest_trimmed);
                        out.push('\n');
                    }
                    out
                }
            } else {
                let mut out = body_rewritten;
                out.push('\n');
                out
            }
        } else {
            let mut out = body_rewritten;
            out.push('\n');
            out
        }
    }

    fn render_terminal(&self, page: &DocPage) -> String {
        let mut out = String::new();
        // Bold title
        out.push_str(&format!("\x1b[1;36m# {}\x1b[0m\n\n", page.title));
        if !page.summary.is_empty() {
            out.push_str(&format!("\x1b[3m{}\x1b[0m\n\n", page.summary));
        }

        let body_rewritten =
            self.rewrite_links(&page.path, &page.raw_content, RenderMode::Terminal);
        out.push_str(&body_rewritten);
        out.push('\n');

        out
    }

    pub fn rewrite_links(&self, current_path: &str, content: &str, mode: RenderMode) -> String {
        let mut result = String::new();
        let chars: Vec<char> = content.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for markdown link: `[label](target)`
            if chars[i] == '[' {
                let start_label = i + 1;
                let mut label_end = None;
                let mut j = start_label;
                while j < len {
                    if chars[j] == ']' {
                        label_end = Some(j);
                        break;
                    }
                    if chars[j] == '\n' {
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
                            if chars[k] == '\n' {
                                break;
                            }
                            k += 1;
                        }

                        if let Some(tend) = target_end {
                            let label: String = chars[start_label..lend].iter().collect();
                            let target: String = chars[start_target..tend].iter().collect();

                            let resolved = self.index.resolve_link(current_path, &target);
                            let rendered_link =
                                self.format_link(&label, &target, resolved.as_ref(), mode);
                            result.push_str(&rendered_link);

                            i = tend + 1;
                            continue;
                        }
                    }
                }
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }

    fn format_link(
        &self,
        label: &str,
        raw_target: &str,
        resolved: Option<&ResolvedLink>,
        mode: RenderMode,
    ) -> String {
        match mode {
            RenderMode::Agent => {
                if let Some(res) = resolved {
                    let cmd = if res.category == "rule" {
                        format!(
                            "{} {} {}",
                            self.binary_name, self.explain_subcommand, res.target_id
                        )
                    } else {
                        format!(
                            "{} {} {}",
                            self.binary_name, self.guide_subcommand, res.target_id
                        )
                    };
                    format!("{} (see: '{}')", label, cmd)
                } else {
                    format!("{} ({})", label, raw_target)
                }
            }
            RenderMode::Terminal => {
                // OSC 8 terminal hyperlink
                if let Some(res) = resolved {
                    let cmd_hint = if res.category == "rule" {
                        format!(
                            "{} {} {}",
                            self.binary_name, self.explain_subcommand, res.target_id
                        )
                    } else {
                        format!(
                            "{} {} {}",
                            self.binary_name, self.guide_subcommand, res.target_id
                        )
                    };
                    format!(
                        "\x1b[4;34m{}\x1b[0m \x1b[90m(run '{}')\x1b[0m",
                        label, cmd_hint
                    )
                } else if raw_target.starts_with("http://") || raw_target.starts_with("https://") {
                    // Standard OSC 8 hyperlink: \x1b]8;;URL\x1b\LABEL\x1b]8;;\x1b\
                    format!(
                        "\x1b]8;;{}\x1b\\\x1b[4;34m{}\x1b[0m\x1b]8;;\x1b\\",
                        raw_target, label
                    )
                } else {
                    format!("\x1b[4m{}\x1b[0m ({})", label, raw_target)
                }
            }
            RenderMode::Json => format!("[{}]({})", label, raw_target),
            RenderMode::Mcp => {
                if let Some(res) = resolved {
                    if let Some(ref anchor) = res.anchor {
                        format!(
                            "{} (resource: '{}://doc/{}#{}', tool: 'doc_read {{\"id\": \"{}\", \"section\": \"{}\"}}')",
                            label, self.scheme, res.target_id, anchor, res.target_id, anchor
                        )
                    } else {
                        format!(
                            "{} (resource: '{}://doc/{}', tool: 'doc_read {{\"id\": \"{}\"}}')",
                            label, self.scheme, res.target_id, res.target_id
                        )
                    }
                } else {
                    format!("{} ({})", label, raw_target)
                }
            }
        }
    }
}

fn starts_with_h1(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return trimmed.starts_with("# ") || trimmed == "#";
    }
    false
}

fn has_frontmatter_summary(source: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    if lines.first().map(|l| l.trim()) == Some("---") {
        for line in lines.iter().skip(1) {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }
            if let Some((k, _)) = trimmed.split_once(':') {
                if k.trim().eq_ignore_ascii_case("summary") {
                    return true;
                }
            }
        }
    }
    false
}
