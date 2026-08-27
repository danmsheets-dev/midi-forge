//! GUI streamable HTTP MCP on **127.0.0.1 only**. Never bind `0.0.0.0`.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use super::host::LiveHost;
use super::stdio::ForgeMcp;
use crate::app::EngineInner;

/// Loopback bind only. Port `0` lets the OS pick an ephemeral port (tests).
pub(crate) fn bind_addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

pub(crate) struct McpHttpHandle {
    cancel: CancellationToken,
    join: Option<JoinHandle<()>>,
    pub local_addr: SocketAddr,
}

impl McpHttpHandle {
    pub(crate) fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Bind `127.0.0.1:port` `/mcp` and serve the 13 technician tools against `LiveHost`.
/// Blocks until the socket is listening (or bind fails) so the UI can show status.
pub(crate) fn spawn(inner: Arc<Mutex<EngineInner>>, port: u16) -> Result<McpHttpHandle, String> {
    let addr = bind_addr(port);
    if !addr.ip().is_loopback() || addr.ip().is_unspecified() {
        return Err("MCP HTTP refuses non-loopback bind".into());
    }

    let cancel = CancellationToken::new();
    let cancel_thread = cancel.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();

    let join = std::thread::Builder::new()
        .name("midi-mcp".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_io()
                .enable_time()
                .thread_name("midi-mcp-rt")
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    let _ = ready_tx.send(Err(format!("tokio runtime: {err}")));
                    return;
                }
            };
            rt.block_on(serve(inner, addr, cancel_thread, ready_tx));
        })
        .map_err(|err| format!("spawn midi-mcp: {err}"))?;

    match ready_rx.recv() {
        Ok(Ok(local_addr)) => Ok(McpHttpHandle {
            cancel,
            join: Some(join),
            local_addr,
        }),
        Ok(Err(err)) => {
            let _ = join.join();
            Err(err)
        }
        Err(_) => {
            cancel.cancel();
            let _ = join.join();
            Err("midi-mcp thread exited before bind".into())
        }
    }
}

async fn serve(
    inner: Arc<Mutex<EngineInner>>,
    addr: SocketAddr,
    cancel: CancellationToken,
    ready_tx: std::sync::mpsc::Sender<Result<SocketAddr, String>>,
) {
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            let _ = ready_tx.send(Err(format!("bind {addr}: {err}")));
            return;
        }
    };
    let local = match listener.local_addr() {
        Ok(local) => local,
        Err(err) => {
            let _ = ready_tx.send(Err(format!("local addr: {err}")));
            return;
        }
    };
    if !local.ip().is_loopback() || local.ip().is_unspecified() {
        let _ = ready_tx.send(Err(format!(
            "refusing non-loopback MCP bind {}",
            local.ip()
        )));
        return;
    }

    let host = LiveHost::new(inner);
    let port = local.port();
    let service = StreamableHttpService::new(
        move || Ok(ForgeMcp::new(host.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(cancel.child_token())
            .with_allowed_hosts([
                "127.0.0.1".into(),
                "localhost".into(),
                "::1".into(),
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ]),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let _ = ready_tx.send(Ok(local));
    let shutdown = cancel.clone();
    let _ = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown.cancelled().await;
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn bind_addr_is_loopback_only() {
        let addr = bind_addr(7420);
        assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
        assert!(addr.ip().is_loopback());
        assert!(!addr.ip().is_unspecified());
        assert_ne!(addr.ip(), Ipv4Addr::UNSPECIFIED);
        assert_eq!(addr.port(), 7420);
    }

    #[test]
    fn bind_localhost_only() {
        let inner = Arc::new(Mutex::new(EngineInner::for_test()));
        let handle = spawn(Arc::clone(&inner), 0).expect("listen 127.0.0.1:0");
        assert!(handle.local_addr.ip().is_loopback());
        assert!(!handle.local_addr.ip().is_unspecified());
        assert_eq!(handle.local_addr.ip(), Ipv4Addr::LOCALHOST);
        TcpStream::connect(handle.local_addr).expect("connect loopback MCP");
        handle.shutdown();
    }
}
