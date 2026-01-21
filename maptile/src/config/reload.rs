//! Configuration hot reload functionality

use std::sync::Arc;
use std::time::Duration;

use log::{error, info};
use tokio::sync::{RwLock, watch};
use tokio::time::interval;

use super::{
    ConfigResult, MaptileConfig, create_config_pool, load_sources_from_database,
    query_config_metadata,
};
use crate::server::MaptileServiceImpl;

/// Start the configuration reload task
pub async fn start_reload_task(
    service: Arc<RwLock<MaptileServiceImpl>>,
    config: MaptileConfig,
    reload_interval_sec: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = interval(Duration::from_secs(reload_interval_sec));
    let mut last_version: Option<i64> = None;

    info!(
        "Starting configuration reload task (interval: {}s)",
        reload_interval_sec
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match check_and_reload(&service, &config, &mut last_version).await {
                    Ok(reloaded) => {
                        if reloaded {
                            info!("Configuration reloaded successfully");
                        }
                    }
                    Err(e) => {
                        error!("Configuration reload failed: {}", e);
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("Stopping configuration reload task");
                    break;
                }
            }
        }
    }
}

/// Check for configuration changes and reload if necessary
async fn check_and_reload(
    service: &Arc<RwLock<MaptileServiceImpl>>,
    config: &MaptileConfig,
    last_version: &mut Option<i64>,
) -> ConfigResult<bool> {
    let pool = create_config_pool(&config.postgres).await?;
    let metadata = query_config_metadata(&pool).await?;

    // Check if version changed
    let should_reload = match last_version {
        Some(last) if *last == metadata.version => false,
        _ => true,
    };

    if !should_reload {
        return Ok(false);
    }

    info!(
        "Configuration version changed: {:?} -> {}",
        last_version, metadata.version
    );

    // Reload sources
    let loaded = load_sources_from_database(config, &pool).await?;

    // Update service
    {
        let mut service_guard = service.write().await;
        service_guard.update_sources(loaded.sources);
        service_guard.update_metadata(loaded.metadata.clone());
    }

    *last_version = Some(metadata.version);

    Ok(true)
}
