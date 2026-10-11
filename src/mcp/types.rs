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
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

impl ToolContent {
    pub fn text(t: impl Into<String>) -> Self {
        Self {
            content_type: "text".to_string(),
            text: t.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "structuredContent")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not", rename = "isError")]
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::text(text)],
            structured_content: None,
            is_error: false,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::text(msg)],
            structured_content: None,
            is_error: true,
        }
    }

    pub fn structured(text: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            content: vec![ToolContent::text(text)],
            structured_content: Some(data),
            is_error: false,
        }
    }
}

/// Asynchronous or delayed result support (e.g. for Karakuri rendering/pending tools).
pub trait PendingResult<T>: Send + Sync {
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
