use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResourceContents {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl ToolContent {
    /// Create a new text content element.
    pub fn text(t: impl Into<String>) -> Self {
        Self::Text { text: t.into() }
    }

    /// Create a new image content element.
    ///
    /// `data` must be base64-encoded image data without a `data:` URL scheme.
    /// If a data URL prefix (e.g. `data:image/png;base64,...`) is provided,
    /// the prefix up to the first comma is automatically stripped.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        let mut d = data.into();
        if d.starts_with("data:") {
            if let Some(idx) = d.find(',') {
                d = d[idx + 1..].to_string();
            }
        }
        Self::Image {
            data: d,
            mime_type: mime_type.into(),
        }
    }

    /// Return the text content if this element is a `Text` variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text.as_str()),
            Self::Image { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "structuredContent")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "std::ops::Not::not",
        rename = "isError"
    )]
    pub is_error: bool,
}

impl ToolResult {
    /// Create a new tool result with custom content elements.
    pub fn new(content: Vec<ToolContent>) -> Self {
        Self {
            content,
            structured_content: None,
            is_error: false,
        }
    }

    /// Create a successful tool result with a single text element.
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(vec![ToolContent::text(text)])
    }

    /// Create an error tool result with a single text element.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::text(msg)],
            structured_content: None,
            is_error: true,
        }
    }

    /// Create a successful tool result with a single image element.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::new(vec![ToolContent::image(data, mime_type)])
    }

    /// Create a successful tool result with an explanation text and an image element.
    pub fn text_and_image(
        text: impl Into<String>,
        data: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self::new(vec![
            ToolContent::text(text),
            ToolContent::image(data, mime_type),
        ])
    }

    /// Attach structured JSON content to this result.
    pub fn with_structured(mut self, data: serde_json::Value) -> Self {
        self.structured_content = Some(data);
        self
    }

    /// Mark this result as an error (`isError: true`).
    pub fn as_error(mut self) -> Self {
        self.is_error = true;
        self
    }

    /// Create a successful tool result with a text element and structured data.
    pub fn structured(text: impl Into<String>, data: serde_json::Value) -> Self {
        Self::text(text).with_structured(data)
    }

    /// Create an error tool result with a text element and structured data.
    pub fn error_structured(text: impl Into<String>, data: serde_json::Value) -> Self {
        Self::error(text).with_structured(data)
    }

    /// Extract and concatenate all text elements joined by newlines, skipping non-text elements.
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Asynchronous or delayed result support (e.g. for Karakuri rendering/pending tools).
pub trait PendingResult<T>: Send {
    /// Non-blocking check for result readiness.
    fn poll_settled(&mut self) -> Option<T>;

    /// Blocking wait for result completion on platforms that support blocking.
    #[cfg(not(target_arch = "wasm32"))]
    fn settled(self: Box<Self>) -> T;
}

pub enum HandlerResult<T> {
    Ready(T),
    Pending(Box<dyn PendingResult<T>>),
}

impl<T> From<T> for HandlerResult<T> {
    fn from(val: T) -> Self {
        HandlerResult::Ready(val)
    }
}

/// Waits for a pending result to settle by repeatedly polling, yielding between attempts,
/// and timing out if it takes too long.
pub async fn poll_until_settled<T, F, Fut, Now>(
    pending: &mut dyn PendingResult<T>,
    timeout: std::time::Duration,
    mut yield_frame: F,
    mut now: Now,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
    Now: FnMut() -> std::time::Duration,
{
    let start = now();
    loop {
        if let Some(res) = pending.poll_settled() {
            return Ok(res);
        }
        let current = now();
        if current.saturating_sub(start) >= timeout {
            return Err("MCP tool execution timed out".to_string());
        }
        yield_frame().await;
    }
}
