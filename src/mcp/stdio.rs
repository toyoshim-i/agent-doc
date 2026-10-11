use crate::mcp::types::HandlerResult;
use crate::DocEngine;
use std::io::{self, BufRead, Write};

/// Run MCP server over stdio using newline-delimited JSON-RPC messages.
pub fn serve_stdio(engine: &DocEngine) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve_stdio_io(engine, stdin.lock(), stdout.lock())
}

/// Run MCP server over arbitrary reader and writer streams.
pub fn serve_stdio_io<R: BufRead, W: Write>(
    engine: &DocEngine,
    mut reader: R,
    mut writer: W,
) -> io::Result<()> {
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let res = engine.handle(trimmed);
            let output = match res {
                HandlerResult::Ready(opt) => opt,
                HandlerResult::Pending(p) => p.settled(),
            };

            if let Some(resp) = output {
                writer.write_all(resp.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
        }
        line.clear();
    }

    Ok(())
}
