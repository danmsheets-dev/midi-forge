//! Stdio MCP is an HTTP **client** to the GUI `/mcp` and a **server** on stdio
//! that forwards the same tool names. No second MIDI stack (WinMM exclusive-open).

use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ClientCapabilities, ClientInfo,
    ContentBlock, ErrorData, Implementation, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleClient, RoleServer, RunningService, ServiceExt};
use rmcp::transport::StreamableHttpClientTransport;

pub(crate) type GuiClient = RunningService<RoleClient, ClientInfo>;

/// Connect to GUI streamable HTTP MCP (`http://127.0.0.1:<port>/mcp`).
pub(crate) async fn connect(uri: &str) -> Result<GuiClient, String> {
    let transport = StreamableHttpClientTransport::from_uri(uri.to_string());
    let info = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("midi-forge-attach", env!("CARGO_PKG_VERSION")),
    );
    info.serve(transport)
        .await
        .map_err(|e| format!("attach {uri}: {e}"))
}

/// Stdio MCP server that proxies `tools/list` and `tools/call` to the GUI session.
pub(crate) struct AttachProxy {
    client: Arc<GuiClient>,
}

impl AttachProxy {
    pub(crate) fn new(client: GuiClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }
}

impl ServerHandler for AttachProxy {
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

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.client
            .list_tools(request)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.client
            .call_tool_once(request)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }
}

pub(crate) async fn serve_stdio(uri: &str) -> Result<(), String> {
    let client = connect(uri).await?;
    let proxy = AttachProxy::new(client);
    let running = proxy
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| e.to_string())?;
    running.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn call_tool(
    client: &GuiClient,
    name: &'static str,
    arguments: Option<JsonObject>,
) -> Result<CallToolResult, String> {
    let mut params = CallToolRequestParams::new(name);
    if let Some(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    client.call_tool(params).await.map_err(|e| e.to_string())
}

pub(crate) fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Same tool name as the GUI: `list_endpoints`.
pub(crate) async fn list_endpoints(client: &GuiClient) -> Result<String, String> {
    let result = call_tool(client, "list_endpoints", None).await?;
    Ok(result_text(&result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::EngineInner;
    use crate::mcp::http;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn block_on<T>(fut: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("tokio")
            .block_on(fut)
    }

    #[test]
    fn attach_list_endpoints_hits_gui_http() {
        let inner = Arc::new(Mutex::new(EngineInner::for_test()));
        let handle = http::spawn(Arc::clone(&inner), 0).expect("listen 127.0.0.1:0");
        let uri = format!("http://127.0.0.1:{}/mcp", handle.local_addr.port());
        let json = block_on(async {
            let client = tokio::time::timeout(Duration::from_secs(5), connect(&uri))
                .await
                .expect("connect timeout")
                .expect("connect GUI MCP");
            list_endpoints(&client).await.expect("list_endpoints")
        });
        assert!(
            json.contains("Null Keyboard"),
            "expected GUI fixture ports, got {json}"
        );
        assert!(json.contains("Null Synth"), "got {json}");
        handle.shutdown();
    }

    #[test]
    fn attach_writes_follow_gui_arm() {
        let inner = Arc::new(Mutex::new(EngineInner::for_test()));
        let handle = http::spawn(Arc::clone(&inner), 0).expect("listen 127.0.0.1:0");
        let uri = format!("http://127.0.0.1:{}/mcp", handle.local_addr.port());
        let args = serde_json::json!({ "out": "Null Synth", "note": 60 })
            .as_object()
            .cloned();
        block_on(async {
            let client = tokio::time::timeout(Duration::from_secs(5), connect(&uri))
                .await
                .expect("connect timeout")
                .expect("connect GUI MCP");

            let unarmed = call_tool(&client, "send_note", args.clone())
                .await
                .expect("send_note unarmed");
            assert_eq!(unarmed.is_error, Some(true));
            let text = result_text(&unarmed);
            assert!(
                text.to_lowercase().contains("arm"),
                "unarmed write should mention arm, got {text}"
            );

            inner.lock().unwrap_or_else(|e| e.into_inner()).agent_armed = true;
            let armed = call_tool(&client, "send_note", args)
                .await
                .expect("send_note armed");
            assert_ne!(armed.is_error, Some(true));
            let text = result_text(&armed);
            assert!(
                text.contains("Null Synth"),
                "armed send_note should reach GUI output, got {text}"
            );
        });
        handle.shutdown();
    }
}
