//! QMP (QEMU Machine Protocol) client implementation
//!
//! Provides async communication with QEMU via Unix socket.

use crate::{Error, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, trace, warn};

/// QMP client for QEMU communication
pub struct QmpClient {
    socket_path: String,
    stream: Mutex<Option<BufReader<UnixStream>>>,
}

impl QmpClient {
    /// Create a new QMP client (does not connect)
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            stream: Mutex::new(None),
        }
    }

    /// Connect to the QMP socket
    pub async fn connect(&self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            Error::Qmp(format!("Failed to connect to {}: {}", self.socket_path, e))
        })?;

        let mut reader = BufReader::new(stream);

        // Read greeting
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        trace!("QMP greeting: {}", line.trim());

        let greeting: QmpMessage = serde_json::from_str(&line)
            .map_err(|e| Error::Qmp(format!("Invalid greeting: {}", e)))?;

        if greeting.qmp.is_none() {
            return Err(Error::Qmp("Invalid QMP greeting".to_string()));
        }

        // Send capabilities negotiation
        let negotiate = QmpCommand {
            execute: "qmp_capabilities".to_string(),
            arguments: None::<()>,
        };

        let writer = reader.get_mut();
        let cmd = serde_json::to_string(&negotiate)?;
        writer.write_all(cmd.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Read response
        line.clear();
        reader.read_line(&mut line).await?;
        trace!("QMP capabilities response: {}", line.trim());

        let response: QmpResponse<serde_json::Value> = serde_json::from_str(&line)
            .map_err(|e| Error::Qmp(format!("Invalid response: {}", e)))?;

        if response.error.is_some() {
            return Err(Error::Qmp(format!(
                "Capabilities negotiation failed: {:?}",
                response.error
            )));
        }

        *self.stream.lock().await = Some(reader);
        debug!("Connected to QMP socket: {}", self.socket_path);

        Ok(())
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.stream.lock().await.is_some()
    }

    /// Execute a QMP command
    pub async fn execute<A: Serialize, R: DeserializeOwned>(
        &self,
        command: &str,
        arguments: Option<A>,
    ) -> Result<R> {
        let mut guard = self.stream.lock().await;
        let reader = guard.as_mut().ok_or_else(|| Error::Qmp("Not connected".to_string()))?;

        let cmd = QmpCommand {
            execute: command.to_string(),
            arguments,
        };

        let writer = reader.get_mut();
        let cmd_str = serde_json::to_string(&cmd)?;
        trace!("QMP command: {}", cmd_str);

        writer.write_all(cmd_str.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Read response (skip events)
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            trace!("QMP response: {}", line.trim());

            // Skip event messages
            if line.contains("\"event\"") {
                continue;
            }

            let response: QmpResponse<R> = serde_json::from_str(&line)
                .map_err(|e| Error::Qmp(format!("Invalid response: {}", e)))?;

            if let Some(error) = response.error {
                return Err(Error::Qmp(format!(
                    "{}: {}",
                    error.class,
                    error.desc
                )));
            }

            return response.result.ok_or_else(|| Error::Qmp("No return value".to_string()));
        }
    }

    /// Execute a command with no return value
    pub async fn execute_void<A: Serialize>(&self, command: &str, arguments: Option<A>) -> Result<()> {
        let _: serde_json::Value = self.execute(command, arguments).await?;
        Ok(())
    }

    /// Query VM status
    pub async fn query_status(&self) -> Result<VmStatus> {
        self.execute("query-status", None::<()>).await
    }

    /// Stop the VM
    pub async fn stop(&self) -> Result<()> {
        self.execute_void("stop", None::<()>).await
    }

    /// Continue (resume) the VM
    pub async fn cont(&self) -> Result<()> {
        self.execute_void("cont", None::<()>).await
    }

    /// Quit QEMU
    pub async fn quit(&self) -> Result<()> {
        self.execute_void("quit", None::<()>).await
    }

    /// System powerdown
    pub async fn system_powerdown(&self) -> Result<()> {
        self.execute_void("system_powerdown", None::<()>).await
    }

    /// System reset
    pub async fn system_reset(&self) -> Result<()> {
        self.execute_void("system_reset", None::<()>).await
    }

    /// Query QEMU version
    pub async fn query_version(&self) -> Result<QemuVersion> {
        self.execute("query-version", None::<()>).await
    }

    /// Query block devices
    pub async fn query_block(&self) -> Result<Vec<BlockDevice>> {
        self.execute("query-block", None::<()>).await
    }

    /// Save VM memory to file
    pub async fn dump_guest_memory(&self, path: &str, paging: bool) -> Result<()> {
        #[derive(Serialize)]
        struct Args {
            paging: bool,
            protocol: String,
        }

        self.execute_void(
            "dump-guest-memory",
            Some(Args {
                paging,
                protocol: format!("file:{}", path),
            }),
        )
        .await
    }

    /// Create internal snapshot
    pub async fn savevm(&self, name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Args {
            name: String,
        }

        // Note: savevm is HMP command, need to use human-monitor-command
        self.execute_hmp(&format!("savevm {}", name)).await
    }

    /// Load internal snapshot
    pub async fn loadvm(&self, name: &str) -> Result<()> {
        self.execute_hmp(&format!("loadvm {}", name)).await
    }

    /// Execute HMP (Human Monitor Protocol) command
    pub async fn execute_hmp(&self, command: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Args {
            #[serde(rename = "command-line")]
            command_line: String,
        }

        let _: serde_json::Value = self.execute(
            "human-monitor-command",
            Some(Args {
                command_line: command.to_string(),
            }),
        )
        .await?;
        Ok(())
    }

    /// Query VNC server info
    pub async fn query_vnc(&self) -> Result<VncInfo> {
        self.execute("query-vnc", None::<()>).await
    }

    /// Send key event
    pub async fn send_key(&self, keys: &[&str]) -> Result<()> {
        #[derive(Serialize)]
        struct KeyValue {
            #[serde(rename = "type")]
            key_type: String,
            data: String,
        }

        #[derive(Serialize)]
        struct Args {
            keys: Vec<KeyValue>,
        }

        let args = Args {
            keys: keys
                .iter()
                .map(|k| KeyValue {
                    key_type: "qcode".to_string(),
                    data: k.to_string(),
                })
                .collect(),
        };

        self.execute_void("send-key", Some(args)).await
    }

    /// Close the connection
    pub async fn close(&self) {
        let mut guard = self.stream.lock().await;
        *guard = None;
    }
}

// QMP protocol types
#[derive(Debug, Serialize)]
struct QmpCommand<A> {
    execute: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<A>,
}

#[derive(Debug, Deserialize)]
struct QmpMessage {
    #[serde(rename = "QMP")]
    qmp: Option<QmpGreeting>,
}

#[derive(Debug, Deserialize)]
struct QmpGreeting {
    version: QmpVersionInfo,
}

#[derive(Debug, Deserialize)]
struct QmpVersionInfo {
    qemu: QemuVersionNumbers,
}

#[derive(Debug, Deserialize)]
struct QemuVersionNumbers {
    micro: u32,
    minor: u32,
    major: u32,
}

#[derive(Debug, Deserialize)]
struct QmpResponse<T> {
    #[serde(rename = "return")]
    result: Option<T>,
    error: Option<QmpError>,
}

#[derive(Debug, Deserialize)]
struct QmpError {
    class: String,
    desc: String,
}

/// VM status from query-status
#[derive(Debug, Clone, Deserialize)]
pub struct VmStatus {
    pub running: bool,
    pub singlestep: bool,
    pub status: String,
}

/// QEMU version info
#[derive(Debug, Clone, Deserialize)]
pub struct QemuVersion {
    pub qemu: QemuVersionDetail,
    pub package: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QemuVersionDetail {
    pub micro: u32,
    pub minor: u32,
    pub major: u32,
}

impl std::fmt::Display for QemuVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            self.qemu.major, self.qemu.minor, self.qemu.micro
        )
    }
}

/// Block device info
#[derive(Debug, Clone, Deserialize)]
pub struct BlockDevice {
    pub device: String,
    pub locked: bool,
    pub removable: bool,
    #[serde(rename = "type")]
    pub device_type: String,
    pub inserted: Option<BlockInserted>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockInserted {
    pub file: String,
    pub ro: bool,
    pub drv: String,
}

/// VNC server info
#[derive(Debug, Clone, Deserialize)]
pub struct VncInfo {
    pub enabled: bool,
    pub host: Option<String>,
    pub service: Option<String>,
    pub auth: Option<String>,
    pub clients: Option<Vec<VncClient>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VncClient {
    pub host: String,
    pub service: String,
}

impl VncInfo {
    /// Get VNC port as number
    pub fn port(&self) -> Option<u16> {
        self.service.as_ref()?.parse().ok()
    }
}

/// Helper to check if QMP socket is available
pub async fn wait_for_qmp(socket_path: &Path, timeout_secs: u64) -> Result<QmpClient> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    loop {
        if start.elapsed() > timeout {
            return Err(Error::Timeout {
                seconds: timeout_secs,
            });
        }

        if socket_path.exists() {
            let client = QmpClient::new(socket_path.to_string_lossy().to_string());
            match client.connect().await {
                Ok(_) => return Ok(client),
                Err(e) => {
                    trace!("QMP not ready: {}", e);
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qmp_command_serialization() {
        #[derive(Serialize)]
        struct TestArgs {
            name: String,
        }

        let cmd = QmpCommand {
            execute: "test".to_string(),
            arguments: Some(TestArgs {
                name: "value".to_string(),
            }),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"execute\":\"test\""));
        assert!(json.contains("\"arguments\""));
    }

    #[test]
    fn test_qmp_response_parsing() {
        let json = r#"{"return": {"running": true, "singlestep": false, "status": "running"}}"#;
        let response: QmpResponse<VmStatus> = serde_json::from_str(json).unwrap();
        assert!(response.result.is_some());
        assert!(response.result.unwrap().running);
    }

    #[test]
    fn test_qmp_error_parsing() {
        let json = r#"{"error": {"class": "GenericError", "desc": "Something went wrong"}}"#;
        let response: QmpResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().class, "GenericError");
    }
}
