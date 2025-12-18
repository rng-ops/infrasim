//! VNC WebSocket proxy
//!
//! Bridges WebSocket connections to VNC servers.

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, trace};

/// VNC WebSocket proxy
pub struct VncProxy {
    host: String,
    port: u16,
}

impl VncProxy {
    /// Create a new VNC proxy
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
        }
    }

    /// Bridge a WebSocket to the VNC server
    pub async fn bridge(self, socket: WebSocket) -> anyhow::Result<()> {
        // Connect to VNC server
        let vnc_addr = format!("{}:{}", self.host, self.port);
        debug!("Connecting to VNC server at {}", vnc_addr);

        let vnc_stream = TcpStream::connect(&vnc_addr).await.map_err(|e| {
            error!("Failed to connect to VNC server: {}", e);
            anyhow::anyhow!("VNC connection failed: {}", e)
        })?;

        debug!("Connected to VNC server");

        let (vnc_read, vnc_write) = vnc_stream.into_split();
        let (ws_write, ws_read) = socket.split();

        // Spawn bidirectional forwarding
        let ws_to_vnc = Self::forward_ws_to_vnc(ws_read, vnc_write);
        let vnc_to_ws = Self::forward_vnc_to_ws(vnc_read, ws_write);

        tokio::select! {
            result = ws_to_vnc => {
                if let Err(e) = result {
                    debug!("WS->VNC forwarding ended: {}", e);
                }
            }
            result = vnc_to_ws => {
                if let Err(e) = result {
                    debug!("VNC->WS forwarding ended: {}", e);
                }
            }
        }

        debug!("VNC proxy session ended");
        Ok(())
    }

    /// Forward WebSocket messages to VNC
    async fn forward_ws_to_vnc(
        mut ws_read: futures::stream::SplitStream<WebSocket>,
        mut vnc_write: tokio::net::tcp::OwnedWriteHalf,
    ) -> anyhow::Result<()> {
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    trace!("WS->VNC: {} bytes", data.len());
                    vnc_write.write_all(&data).await?;
                }
                Ok(Message::Text(text)) => {
                    // Some WebSocket clients send text for RFB version
                    trace!("WS->VNC (text): {} bytes", text.len());
                    vnc_write.write_all(text.as_bytes()).await?;
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket closed by client");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    // Ping is handled by axum
                    trace!("Ping received");
                }
                Ok(Message::Pong(_)) => {}
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Forward VNC data to WebSocket
    async fn forward_vnc_to_ws(
        mut vnc_read: tokio::net::tcp::OwnedReadHalf,
        mut ws_write: futures::stream::SplitSink<WebSocket, Message>,
    ) -> anyhow::Result<()> {
        let mut buffer = vec![0u8; 64 * 1024];

        loop {
            let n = vnc_read.read(&mut buffer).await?;
            if n == 0 {
                debug!("VNC server closed connection");
                break;
            }

            trace!("VNC->WS: {} bytes", n);

            if let Err(e) = ws_write.send(Message::Binary(buffer[..n].to_vec())).await {
                error!("Failed to send to WebSocket: {}", e);
                break;
            }
        }

        let _ = ws_write.close().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_creation() {
        let proxy = VncProxy::new("127.0.0.1", 5900);
        assert_eq!(proxy.host, "127.0.0.1");
        assert_eq!(proxy.port, 5900);
    }
}
