//! `midi-forge mcp` stdio server (official `rmcp` transport).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo};
use rmcp::service::ServiceExt;
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;

use super::DEFAULT_MCP_PORT;
use super::host::{McpHost, StandaloneHost};
use super::tools;

const DEFAULT_PROBE_HOST: &str = "127.0.0.1";
const ATTACH_FAIL: &str = "GUI MCP not listening; standalone session";
const GUI_LISTENING: &str =
    "GUI MCP is listening; this process is still standalone until attach proxy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpFlags {
    pub arm: bool,
    pub attach: bool,
    pub standalone: bool,
    pub mcp_url: Option<String>,
    pub mcp_port: u16,
    pub probe_host: String,
}

impl Default for McpFlags {
    fn default() -> Self {
        Self {
            arm: false,
            attach: false,
            standalone: false,
            mcp_url: None,
            mcp_port: DEFAULT_MCP_PORT,
            probe_host: DEFAULT_PROBE_HOST.into(),
        }
    }
}

/// Parse `midi-forge mcp` flags. `--mcp-url` is stored for Task 4; host/port
/// feed the TCP probe (always, unless `--standalone`).
pub fn parse_mcp_flags(args: &[String]) -> Result<McpFlags, String> {
    let mut i = 0usize;
    if args
        .first()
        .is_some_and(|s| !s.starts_with('-') && s != "mcp")
    {
        i = 1;
    }
    if args.get(i).map(String::as_str) == Some("mcp") {
        i += 1;
    }

    let mut flags = McpFlags::default();
    let mut port_override: Option<u16> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--arm" => flags.arm = true,
            "--attach" => flags.attach = true,
            "--standalone" => flags.standalone = true,
            "--mcp-port" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --mcp-port".to_string())?;
                flags.mcp_port = parse_port(v)?;
                port_override = Some(flags.mcp_port);
                i += 1;
            }
            "--mcp-url" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --mcp-url".to_string())?;
                flags.mcp_url = Some(v.clone());
                i += 1;
            }
            other if other.starts_with("--mcp-port=") => {
                let v = &other["--mcp-port=".len()..];
                flags.mcp_port = parse_port(v)?;
                port_override = Some(flags.mcp_port);
            }
            other if other.starts_with("--mcp-url=") => {
                flags.mcp_url = Some(other["--mcp-url=".len()..].to_string());
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(url) = flags.mcp_url.as_deref() {
        let (host, port) = split_mcp_url(url);
        if !host.is_empty() {
            flags.probe_host = host;
        }
        if let Some(port) = port {
            flags.mcp_port = port;
        }
    }
    if let Some(port) = port_override {
        flags.mcp_port = port;
    }
    Ok(flags)
}

fn parse_port(v: &str) -> Result<u16, String> {
    v.parse::<u16>()
        .map_err(|_| format!("invalid --mcp-port {v:?}"))
}

fn split_mcp_url(url: &str) -> (String, Option<u16>) {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let hostport = rest.split('/').next().unwrap_or(rest);
    if let Some((host, port)) = hostport.rsplit_once(':') {
        (host.to_string(), port.parse().ok())
    } else {
        (hostport.to_string(), None)
    }
}

/// Stdio MCP entry. Does not touch real MIDI from unit tests — call
/// [`parse_mcp_flags`] instead of [`run`].
pub fn run(args: &[String]) -> i32 {
    let flags = match parse_mcp_flags(args) {
        Ok(f) => f,
        Err(err) => {
            eprintln!("{err}");
            return 2;
        }
    };
    match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(rt) => match rt.block_on(serve(flags)) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("{err}");
                1
            }
        },
        Err(err) => {
            eprintln!("tokio runtime: {err}");
            1
        }
    }
}

async fn serve(flags: McpFlags) -> Result<(), String> {
    let gui_listening = if flags.standalone {
        None
    } else {
        Some(probe_gui(&flags.probe_host, flags.mcp_port).await)
    };
    let (auto_open, notice) = session_plan(&flags, gui_listening);
    if let Some(msg) = notice {
        eprintln!("{msg}");
    }

    // GUI present: do not open OS inputs (WinMM exclusive-open). Null backend
    // until Task 4 attach proxy; `from_default()` always auto-opens.
    let mut host = if auto_open {
        StandaloneHost::from_default()
    } else {
        StandaloneHost::with_null()
    };
    if flags.arm {
        host.set_armed(true);
    }
    let server = ForgeMcp::new(host);
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| e.to_string())?;
    running.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// `gui_listening` is `None` when `--standalone` skipped the TCP probe.
/// Returns `(auto_open_inputs, stderr_notice)`.
fn session_plan(flags: &McpFlags, gui_listening: Option<bool>) -> (bool, Option<&'static str>) {
    match gui_listening {
        Some(true) => (false, Some(GUI_LISTENING)),
        Some(false) if flags.attach => (true, Some(ATTACH_FAIL)),
        _ => (true, None),
    }
}

async fn probe_gui(host: &str, port: u16) -> bool {
    let addr = format!("{host}:{port}");
    matches!(
        tokio::time::timeout(
            Duration::from_millis(100),
            tokio::net::TcpStream::connect(addr)
        )
        .await,
        Ok(Ok(_))
    )
}

fn tool_text(result: Result<String, String>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(s) => Ok(CallToolResult::success(vec![ContentBlock::text(s)])),
        Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
    }
}

#[derive(Clone)]
pub(crate) struct ForgeMcp {
    host: Arc<Mutex<dyn McpHost>>,
}

impl ForgeMcp {
    pub(crate) fn new(host: impl McpHost + 'static) -> Self {
        Self {
            host: Arc::new(Mutex::new(host)),
        }
    }

    fn with_host<T>(&self, f: impl FnOnce(&mut dyn McpHost) -> T) -> T {
        let mut host = self.host.lock().unwrap_or_else(|e| e.into_inner());
        host.poll();
        f(&mut *host)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MonitorTailArgs {
    #[serde(default = "default_monitor_limit")]
    limit: u32,
}

fn default_monitor_limit() -> u32 {
    40
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendNoteArgs {
    out: String,
    note: u8,
    #[serde(default = "default_vel")]
    vel: u8,
    #[serde(default = "default_ch")]
    ch: u8,
    #[serde(default)]
    group: u8,
    #[serde(default)]
    m2: bool,
}

fn default_vel() -> u8 {
    100
}

fn default_ch() -> u8 {
    1
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendCcArgs {
    out: String,
    cc: u8,
    val: u8,
    #[serde(default = "default_ch")]
    ch: u8,
    #[serde(default)]
    group: u8,
    #[serde(default)]
    m2: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IdentityArgs {
    out: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PanicArgs {
    #[serde(default)]
    out: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetPortOpenArgs {
    id: String,
    #[serde(default)]
    output: bool,
    open: bool,
}

#[tool_router]
impl ForgeMcp {
    #[tool(description = "List MIDI endpoints (id, name, direction, protocol, open)")]
    fn list_endpoints(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::list_endpoints))
    }

    #[tool(description = "Last N monitor rows as JSON (time, port, UMP words, decoded)")]
    fn monitor_tail(
        &self,
        Parameters(args): Parameters<MonitorTailArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| tools::monitor_tail(h, args.limit as usize)))
    }

    #[tool(description = "Sounding notes / last CC / bend per channel")]
    fn live_now(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::live_now))
    }

    #[tool(description = "MIDI clock health summary")]
    fn clock_health(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::clock_health))
    }

    #[tool(description = "Notes still hanging (no note-off)")]
    fn stuck_notes(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::stuck_notes))
    }

    #[tool(description = "Thru graph: each link from→to and filters that are off")]
    fn thru_graph(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::thru_graph))
    }

    #[tool(description = "MPE mode summary and sounding voices")]
    fn mpe_status(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::mpe_status))
    }

    #[tool(description = "Text snapshot of the current session")]
    fn snapshot(&self) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(tools::snapshot))
    }

    #[tool(description = "Send a note-on (requires --arm)")]
    fn send_note(
        &self,
        Parameters(args): Parameters<SendNoteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| {
            tools::send_note(
                h,
                tools::SendNote {
                    out: args.out,
                    note: args.note,
                    vel: args.vel,
                    ch: args.ch,
                    group: args.group,
                    m2: args.m2,
                },
            )
        }))
    }

    #[tool(description = "Send a control change (requires --arm)")]
    fn send_cc(
        &self,
        Parameters(args): Parameters<SendCcArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| {
            tools::send_cc(
                h,
                tools::SendCc {
                    out: args.out,
                    cc: args.cc,
                    val: args.val,
                    ch: args.ch,
                    group: args.group,
                    m2: args.m2,
                },
            )
        }))
    }

    #[tool(description = "Send MIDI identity request (requires --arm)")]
    fn identity(
        &self,
        Parameters(args): Parameters<IdentityArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| tools::identity(h, tools::Identity { out: args.out })))
    }

    #[tool(
        name = "panic",
        description = "All-notes-off / panic (requires --arm). Optional out, else open outputs."
    )]
    fn panic_all(
        &self,
        Parameters(args): Parameters<PanicArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| tools::panic(h, tools::Panic { out: args.out })))
    }

    #[tool(description = "Open or close a MIDI port (requires --arm)")]
    fn set_port_open(
        &self,
        Parameters(args): Parameters<SetPortOpenArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        tool_text(self.with_host(|h| {
            tools::set_port_open(
                h,
                tools::SetPortOpen {
                    id: args.id,
                    output: args.output,
                    open: args.open,
                },
            )
        }))
    }
}

#[tool_handler]
impl ServerHandler for ForgeMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "midi-forge",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Technician MIDI copilot. Reads always. Writes (send_note, send_cc, identity, panic, set_port_open) require --arm."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        std::iter::once("midi-forge")
            .chain(parts.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn parse_arm_flag() {
        let f = parse_mcp_flags(&args(&["mcp", "--arm"])).unwrap();
        assert!(f.arm);
    }

    #[test]
    fn parse_default_unarmed() {
        let f = parse_mcp_flags(&args(&["mcp"])).unwrap();
        assert!(!f.arm);
        assert!(!f.attach);
        assert!(!f.standalone);
        assert_eq!(f.mcp_port, 7420);
        assert_eq!(f.probe_host, "127.0.0.1");
        assert!(f.mcp_url.is_none());
    }

    #[test]
    fn parse_mcp_port_9() {
        let f = parse_mcp_flags(&args(&["mcp", "--mcp-port", "9"])).unwrap();
        assert_eq!(f.mcp_port, 9);
        assert!(!f.arm);
    }

    #[test]
    fn parse_mcp_url_feeds_probe_host_port() {
        let f = parse_mcp_flags(&args(&["mcp", "--mcp-url", "http://127.0.0.1:7420/mcp"])).unwrap();
        assert_eq!(f.mcp_url.as_deref(), Some("http://127.0.0.1:7420/mcp"));
        assert_eq!(f.probe_host, "127.0.0.1");
        assert_eq!(f.mcp_port, 7420);
    }

    #[test]
    fn parse_mcp_port_overrides_url_port() {
        let f = parse_mcp_flags(&args(&[
            "mcp",
            "--mcp-url",
            "http://127.0.0.1:7420/mcp",
            "--mcp-port",
            "9",
        ]))
        .unwrap();
        assert_eq!(f.mcp_port, 9);
        assert_eq!(f.probe_host, "127.0.0.1");
    }

    #[test]
    fn parse_attach() {
        let f = parse_mcp_flags(&args(&["mcp", "--attach"])).unwrap();
        assert!(f.attach);
        assert!(!f.arm);
        assert!(!f.standalone);
    }

    #[test]
    fn parse_standalone() {
        let f = parse_mcp_flags(&args(&["mcp", "--standalone"])).unwrap();
        assert!(f.standalone);
        assert!(!f.attach);
        assert!(!f.arm);
    }

    #[test]
    fn parse_standalone_and_attach() {
        let f = parse_mcp_flags(&args(&["mcp", "--standalone", "--attach"])).unwrap();
        assert!(f.standalone);
        assert!(f.attach);
    }

    #[test]
    fn default_probe_hit_skips_auto_open() {
        let f = parse_mcp_flags(&args(&["mcp"])).unwrap();
        let (auto_open, notice) = session_plan(&f, Some(true));
        assert!(!auto_open);
        assert_eq!(notice, Some(GUI_LISTENING));
    }

    #[test]
    fn default_probe_miss_is_quiet_standalone() {
        let f = parse_mcp_flags(&args(&["mcp"])).unwrap();
        let (auto_open, notice) = session_plan(&f, Some(false));
        assert!(auto_open);
        assert!(notice.is_none());
    }

    #[test]
    fn attach_probe_miss_prints_fallback() {
        let f = parse_mcp_flags(&args(&["mcp", "--attach"])).unwrap();
        let (auto_open, notice) = session_plan(&f, Some(false));
        assert!(auto_open);
        assert_eq!(notice, Some(ATTACH_FAIL));
    }

    #[test]
    fn attach_probe_hit_skips_auto_open() {
        let f = parse_mcp_flags(&args(&["mcp", "--attach"])).unwrap();
        let (auto_open, notice) = session_plan(&f, Some(true));
        assert!(!auto_open);
        assert_eq!(notice, Some(GUI_LISTENING));
    }

    #[test]
    fn standalone_skips_probe_and_auto_opens() {
        let f = parse_mcp_flags(&args(&["mcp", "--standalone"])).unwrap();
        let (auto_open, notice) = session_plan(&f, None);
        assert!(auto_open);
        assert!(notice.is_none());
    }

    #[test]
    fn tool_router_has_all_v1_names() {
        let names: Vec<String> = ForgeMcp::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for expected in [
            "list_endpoints",
            "monitor_tail",
            "live_now",
            "clock_health",
            "stuck_notes",
            "thru_graph",
            "mpe_status",
            "snapshot",
            "send_note",
            "send_cc",
            "identity",
            "panic",
            "set_port_open",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing tool {expected} in {names:?}"
            );
        }
        assert_eq!(names.len(), 13);
    }
}
