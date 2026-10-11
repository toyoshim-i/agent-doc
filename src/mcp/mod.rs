pub mod protocol;
pub mod tool;
pub mod types;

#[cfg(feature = "clap")]
pub mod clap;

#[cfg(feature = "mcp-stdio")]
pub mod stdio;

#[cfg(feature = "mcp-http")]
pub mod http;

pub use protocol::handle_json_rpc;
pub use tool::{McpToolDefinition, ParamType, Tool, ToolBuilder, ToolParam};
pub use types::{
    HandlerResult, McpResource, McpResourceContents, PendingResult, ToolContent, ToolResult,
};
