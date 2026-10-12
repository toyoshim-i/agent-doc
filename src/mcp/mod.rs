pub mod protocol;
pub mod tool;
pub mod types;

#[cfg(feature = "clap")]
pub mod clap;

#[cfg(all(feature = "mcp-stdio", not(target_arch = "wasm32")))]
pub mod stdio;

#[cfg(all(feature = "mcp-http", not(target_arch = "wasm32")))]
pub mod http;

#[cfg(all(feature = "webmcp", target_arch = "wasm32"))]
pub mod webmcp;

pub use protocol::handle_json_rpc;
pub use tool::{McpToolDefinition, ParamType, Tool, ToolBuilder, ToolParam};
pub use types::{
    poll_until_settled, HandlerResult, McpResource, McpResourceContents, PendingResult,
    ToolContent, ToolResult,
};

#[cfg(all(feature = "mcp-http", not(target_arch = "wasm32")))]
pub use http::HttpServer;
