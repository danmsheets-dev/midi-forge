//! Technician MCP façade: host trait + tool handlers. Stdio in `stdio`.

pub mod attach;
pub mod host;
pub mod http;
pub mod stdio;
pub mod tools;

pub(crate) const DEFAULT_MCP_PORT: u16 = 7420;
