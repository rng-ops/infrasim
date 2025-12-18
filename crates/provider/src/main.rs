//! Terraform Provider for InfraSim
//!
//! This binary implements the Terraform Plugin Protocol v6 for managing
//! InfraSim virtual machines, networks, and volumes.

use std::env;
use std::io::{self, BufRead, Write};
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing::{info, error};

mod server;
mod provider;
mod resources;
mod schema;
mod state;
mod client;

mod generated {
    pub mod infrasim {
        include!("generated/infrasim.v1.rs");
    }
    pub mod tfplugin6 {
        include!("generated/tfplugin6.rs");
    }
}

use generated::tfplugin6::provider_server::ProviderServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("Starting InfraSim Terraform Provider");

    // Terraform expects the provider to listen on a port and communicate via gRPC
    // The protocol handshake is done via stdout
    
    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    
    info!("Provider listening on {}", addr);

    // Create the provider service
    let provider_service = provider::InfraSimProvider::new().await?;

    // Output the handshake to stdout as Terraform expects
    // Format: <proto_version>|<addr>|<proto_type>|<cert_pem>|<server_cert>
    // For unencrypted local connections, we use the simple format
    let handshake = format!(
        "1|{}|tcp||\n",
        addr
    );
    
    // Write handshake to stdout
    io::stdout().write_all(handshake.as_bytes())?;
    io::stdout().flush()?;

    info!("Handshake sent, starting gRPC server");

    // Start the gRPC server
    // Note: We need to drop the listener and rebind with tonic
    drop(listener);
    
    Server::builder()
        .add_service(ProviderServer::new(provider_service))
        .serve(addr)
        .await?;

    Ok(())
}
