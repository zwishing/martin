//! Maptile RPC Server Entry Point

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use log::{error, info};
use tokio::sync::{RwLock, watch};

use maptile::config::{load_config, start_reload_task};
use maptile::server::MaptileServiceImpl;
use maptile::volo_gen::maptile::r#gen::MaptileServiceServer;

/// Maptile - High-performance Thrift RPC microservice for vector tiles
#[derive(Parser, Debug)]
#[command(name = "maptile")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,
}

fn install_crypto_provider() {
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        log::warn!("Failed to install rustls crypto provider");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_crypto_provider();
    env_logger::init();

    let args = Args::parse();

    info!("Loading configuration from: {:?}", args.config);
    let config = load_config(&args.config).await?;

    let addr: SocketAddr = config.server.listen_address.parse()?;
    info!("Starting Maptile RPC server on {}", addr);

    // Initialize the service
    let service = MaptileServiceImpl::new(config.clone()).await?;
    let service = Arc::new(RwLock::new(service));
    let service_for_reload = Arc::clone(&service);

    // Build the Volo server using the generated MaptileServiceServer
    let server = MaptileServiceServer::new(service);

    // Start config reload task if enabled
    if let Some(reload_interval) = config.config.reload_interval_sec {
        let service_clone = Arc::clone(&service_for_reload);
        let config_clone = config.clone();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        tokio::spawn(async move {
            start_reload_task(service_clone, config_clone, reload_interval, shutdown_rx).await;
        });

        tokio::select! {
            result = server.run(volo::net::Address::from(addr)) => {
                let _ = shutdown_tx.send(true);
                if let Err(e) = result {
                    error!("Server error: {:?}", e);
                    return Err(anyhow::anyhow!("Server error: {:?}", e));
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal, stopping server...");
                let _ = shutdown_tx.send(true);
            }
        }

        info!("Maptile RPC server stopped");
        return Ok(());
    }

    info!("Maptile RPC server started successfully");

    // Handle graceful shutdown
    tokio::select! {
        result = server.run(volo::net::Address::from(addr)) => {
            if let Err(e) = result {
                error!("Server error: {:?}", e);
                return Err(anyhow::anyhow!("Server error: {:?}", e));
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, stopping server...");
        }
    }

    info!("Maptile RPC server stopped");
    Ok(())
}
