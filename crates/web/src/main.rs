use std::net::SocketAddr;

use tracing::info;

use infrasim_web::server::{JwtAuthConfig, WebServerConfig, WebUiAuth};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let web_addr: SocketAddr = std::env::var("INFRASIM_WEB_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()?;

    let daemon_addr = std::env::var("INFRASIM_DAEMON_ADDR")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    // Auth config
    // - INFRASIM_AUTH_MODE=jwt enables JWT validation against a local JWKS.
    // - Otherwise, fall back to static token (INFRASIM_WEB_AUTH_TOKEN) or DevRandom.
    let auth = match std::env::var("INFRASIM_AUTH_MODE").ok().as_deref() {
        Some("jwt") => {
            let allowed = std::env::var("INFRASIM_AUTH_ALLOWED_ISSUERS")
                .map_err(|_| anyhow::anyhow!("INFRASIM_AUTH_ALLOWED_ISSUERS is required in jwt mode"))?;
            let audience = std::env::var("INFRASIM_AUTH_AUDIENCE")
                .map_err(|_| anyhow::anyhow!("INFRASIM_AUTH_AUDIENCE is required in jwt mode"))?;
            let local_jwks_path = std::env::var("INFRASIM_AUTH_LOCAL_JWKS_PATH")
                .map_err(|_| anyhow::anyhow!("INFRASIM_AUTH_LOCAL_JWKS_PATH is required in jwt mode"))?;

            WebUiAuth::Jwt(JwtAuthConfig {
                allowed_issuers: allowed
                    .split(',')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect(),
                audience,
                local_jwks_path,
            })
        }
        _ => match std::env::var("INFRASIM_WEB_AUTH_TOKEN") {
            Ok(token) if !token.trim().is_empty() => WebUiAuth::Token(token),
            _ => WebUiAuth::DevRandom,
        },
    };

    let cfg = WebServerConfig {
        daemon_addr,
        auth,
    };

    info!(
        "Starting InfraSim Web UI on http://{} (daemon: {})",
        web_addr, cfg.daemon_addr
    );

    infrasim_web::server::serve(web_addr, cfg).await
}
