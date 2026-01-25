//! Configuration hot reload functionality

use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::Pool;
use log::{error, info};
use tokio::sync::{RwLock, watch};
use tokio::time::interval;

use crate::config::{ConfigResult, MaptileConfig, load_sources_from_database, query_config_metadata};
use crate::handler::MaptileServiceImpl;

/// Start the configuration reload task
pub async fn start_reload_task(
    service: Arc<RwLock<MaptileServiceImpl>>,
    config: MaptileConfig,
    reload_interval_sec: u64,
    mut shutdown: watch::Receiver<bool>,
    config_pool: Pool,
) {
    let mut interval = interval(Duration::from_secs(reload_interval_sec));
    let mut last_version: Option<i64> = None;

    info!(
        "Starting configuration reload task (interval: {reload_interval_sec}s)"
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match check_and_reload(&service, &config, &mut last_version, &config_pool).await {
                    Ok(reloaded) => {
                        if reloaded {
                            info!("Configuration reloaded successfully");
                        }
                    }
                    Err(e) => {
                        error!("Configuration reload failed: {e}");
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
    config_pool: &Pool,
) -> ConfigResult<bool> {
    let metadata = query_config_metadata(config_pool).await?;

    // Check if version changed
    let should_reload = !matches!(last_version, Some(last) if *last == metadata.version);

    if !should_reload {
        return Ok(false);
    }

    info!(
        "Configuration version changed: {last:?} -> {current}",
        last = last_version,
        current = metadata.version
    );

    // Reload sources
    let loaded = load_sources_from_database(config, config_pool).await?;

    // Update service
    {
        let mut service_guard = service.write().await;
        service_guard.update_sources(loaded.sources);
        service_guard.update_metadata(loaded.metadata.clone());
    }

    *last_version = Some(metadata.version);

    Ok(true)
}
