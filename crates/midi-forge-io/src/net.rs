//! UDP transport for Network MIDI 2.0 (M2-124 command + UMP datagrams).

use std::net::{SocketAddr, UdpSocket};

use crate::error::IoError;

pub struct NetUmp {
    sock: Option<UdpSocket>,
    pub bind: String,
    pub peer: String,
    pub last: String,
}

impl Default for NetUmp {
    fn default() -> Self {
        Self {
            sock: None,
            bind: format!("0.0.0.0:{}", midi_forge_core::NETUMP_PORT),
            peer: "127.0.0.1:5004".into(),
            last: String::new(),
        }
    }
}

impl NetUmp {
    pub fn listen(&mut self) -> Result<(), IoError> {
        let sock = UdpSocket::bind(&self.bind).map_err(|e| IoError::Backend(e.to_string()))?;
        sock.set_nonblocking(true)
            .map_err(|e| IoError::Backend(e.to_string()))?;
        self.last = format!("listening {}", self.bind);
        self.sock = Some(sock);
        Ok(())
    }

    pub fn close(&mut self) {
        self.sock = None;
        self.last = "closed".into();
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        let sock = self
            .sock
            .as_ref()
            .ok_or_else(|| IoError::Backend("not listening".into()))?;
        sock.send_to(bytes, self.peer.trim())
            .map_err(|e| IoError::Backend(e.to_string()))?;
        Ok(())
    }

    pub fn poll(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        let Some(sock) = self.sock.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = sock.recv_from(&mut buf) {
            out.push((from, buf[..n].to_vec()));
            if out.len() > 32 {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bind_mentions_port() {
        let n = NetUmp::default();
        assert!(n.bind.contains("5004"));
    }
}
