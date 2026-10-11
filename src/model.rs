use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Agent,     // Plain text with actionable CLI link hints, no ANSI escapes
    Terminal,  // Formatted with ANSI styling and OSC 8 terminal hyperlinks
    Json,      // Structured JSON output
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSection {
    pub heading: String,
    pub level: usize,
    pub anchor: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocPage {
    pub path: String,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    pub aliases: Vec<String>,
    pub sections: Vec<DocSection>,
    pub raw_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    pub target_id: String,
    pub category: String,
    pub anchor: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub category: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    pub key: String,
    pub heading: String,
    pub aliases: Vec<String>,
    pub use_id_as_title: bool,
}

impl CategoryConfig {
    pub fn new(key: impl Into<String>, heading: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            heading: heading.into(),
            aliases: Vec::new(),
            use_id_as_title: false,
        }
    }

    pub fn with_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_id_as_title(mut self, flag: bool) -> Self {
        self.use_id_as_title = flag;
        self
    }

    pub fn matches(&self, category: &str) -> bool {
        self.key.eq_ignore_ascii_case(category)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(category))
    }
}

pub fn default_categories() -> Vec<CategoryConfig> {
    vec![
        CategoryConfig::new("getting-started", "Getting Started & Migration")
            .with_aliases(["start", "migration"]),
        CategoryConfig::new("guide", "Guides & Best Practices")
            .with_aliases(["guides", "best-practices"]),
        CategoryConfig::new("rule", "Diagnostics & Rules")
            .with_aliases(["rules", "diagnostic", "diagnostics"])
            .with_id_as_title(true),
    ]
}
