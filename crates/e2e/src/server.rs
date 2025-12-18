//! Server management - spawning and health checking the web server

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{info, warn};

use crate::error::{E2eError, E2eResult};

/// Handle to a running server process
pub struct ServerHandle {
    child: Child,
    pub base_url: String,
    pub port: u16,
}

impl ServerHandle {
    /// Spawn the infrasim-web server
    pub async fn spawn(config: ServerConfig) -> E2eResult<Self> {
        let port = config.port.unwrap_or_else(find_free_port);
        let base_url = format!("http://127.0.0.1:{}", port);

        info!("Spawning web server on port {}", port);

        let mut cmd = Command::new(&config.binary_path);
        
        // Set environment variables
        cmd.env("INFRASIM_WEB_PORT", port.to_string())
            .env("INFRASIM_WEB_HOST", "127.0.0.1")
            .env("INFRASIM_WEB_STATIC_DIR", &config.static_dir)
            .env("INFRASIM_DAEMON_ADDR", &config.daemon_addr);
        
        // Enable test mode if requested
        if config.test_mode {
            cmd.env("INFRASIM_E2E_TEST_MODE", "1");
        }

        // Disable auth bypass in tests unless explicitly enabled
        if !config.bypass_auth {
            cmd.env("INFRASIM_WEB_DEV_BYPASS_AUTH", "0");
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            E2eError::ServerStartup(format!(
                "Failed to spawn {}: {}",
                config.binary_path.display(),
                e
            ))
        })?;

        let handle = ServerHandle {
            child,
            base_url: base_url.clone(),
            port,
        };

        // Wait for server to be healthy
        handle.wait_for_healthy(config.startup_timeout).await?;

        info!("Server is healthy at {}", base_url);
        Ok(handle)
    }

    /// Wait for the server to respond to health checks
    async fn wait_for_healthy(&self, timeout_duration: Duration) -> E2eResult<()> {
        let health_url = format!("{}/health", self.base_url);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;

        let start = std::time::Instant::now();
        let mut attempts = 0;

        while start.elapsed() < timeout_duration {
            attempts += 1;
            
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                Ok(resp) => {
                    warn!("Health check returned {}", resp.status());
                }
                Err(e) => {
                    if attempts == 1 {
                        info!("Waiting for server to start...");
                    }
                    // Connection refused is expected while server is starting
                    if !e.is_connect() {
                        warn!("Health check error: {}", e);
                    }
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        Err(E2eError::ServerHealthCheck(attempts))
    }

    /// Get the base URL for this server
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Stop the server
    pub fn stop(&mut self) -> E2eResult<()> {
        info!("Stopping server (pid: {})", self.child.id());
        
        // Try graceful shutdown first
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            let pid = Pid::from_raw(self.child.id() as i32);
            if kill(pid, Signal::SIGTERM).is_ok() {
                // Give it a moment to shut down gracefully
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        // Force kill if still running
        let _ = self.child.kill();
        let _ = self.child.wait();
        
        Ok(())
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Configuration for spawning a server
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Path to the infrasim-web binary
    pub binary_path: PathBuf,
    
    /// Directory containing static files (UI dist)
    pub static_dir: PathBuf,
    
    /// Daemon address to connect to
    pub daemon_addr: String,
    
    /// Port to listen on (None = find free port)
    pub port: Option<u16>,
    
    /// Timeout for server startup
    pub startup_timeout: Duration,
    
    /// Enable test mode (e.g., mock data, faster timeouts)
    pub test_mode: bool,
    
    /// Bypass authentication for testing
    pub bypass_auth: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("target/debug/infrasim-web"),
            static_dir: PathBuf::from("ui/apps/console/dist"),
            daemon_addr: "http://127.0.0.1:9090".to_string(),
            port: None,
            startup_timeout: Duration::from_secs(30),
            test_mode: true,
            bypass_auth: false,
        }
    }
}

/// Find a free port to use
fn find_free_port() -> u16 {
    use std::net::TcpListener;
    
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to find free port")
        .local_addr()
        .expect("Failed to get local addr")
        .port()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_free_port() {
        let port1 = find_free_port();
        let port2 = find_free_port();
        
        // Ports should be in valid range
        assert!(port1 > 1024);
        assert!(port2 > 1024);
    }
}
