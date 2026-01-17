use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info};
use martin_core::config::IdResolver;
use tokio::sync::{Mutex, watch};

use crate::config::database::{
    ConfigStatusHandle, DatabaseConfigResult, load_config_from_database, query_config_metadata,
};
use crate::config::file::Config;
use crate::source::SharedTileSources;
use crate::srv::RESERVED_KEYWORDS;

#[cfg(feature = "postgres")]
use deadpool_postgres::Pool;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ReloadSummary {
    pub status: String,
    pub sources_loaded: usize,
    pub version: i64,
}

struct ReloadState {
    config: Config,
    pool: Pool,
    sources: SharedTileSources,
    status: ConfigStatusHandle,
    last_version: Mutex<Option<i64>>,
    #[cfg(feature = "pmtiles")]
    pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
}

#[derive(Clone)]
pub struct ConfigReloadHandle {
    inner: Arc<ReloadState>,
}

impl ConfigReloadHandle {
    pub fn new(
        config: Config,
        pool: Pool,
        sources: SharedTileSources,
        status: ConfigStatusHandle,
        current_version: Option<i64>,
        #[cfg(feature = "pmtiles")] pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
    ) -> Self {
        Self {
            inner: Arc::new(ReloadState {
                config,
                pool,
                sources,
                status,
                last_version: Mutex::new(current_version),
                #[cfg(feature = "pmtiles")]
                pmtiles_cache,
            }),
        }
    }

    pub async fn reload(&self) -> DatabaseConfigResult<ReloadSummary> {
        let id_resolver = IdResolver::new(RESERVED_KEYWORDS);
        let loaded = load_config_from_database(
            &self.inner.config,
            &self.inner.pool,
            &id_resolver,
            #[cfg(feature = "pmtiles")]
            self.inner.pmtiles_cache.clone(),
        )
        .await?;

        let sources_loaded = loaded.sources.source_names().len();
        {
            let mut guard = self.inner.sources.write().await;
            *guard = loaded.sources;
        }

        {
            let mut status = self.inner.status.write().await;
            status.config_version = Some(loaded.metadata.version);
            status.last_config_reload = Some(std::time::SystemTime::now());
        }

        {
            let mut version = self.inner.last_version.lock().await;
            *version = Some(loaded.metadata.version);
        }

        info!(
            "Configuration reloaded: {sources_loaded} sources loaded, version {}",
            loaded.metadata.version
        );

        Ok(ReloadSummary {
            status: "success".to_string(),
            sources_loaded,
            version: loaded.metadata.version,
        })
    }

    pub async fn reload_if_changed(&self) -> DatabaseConfigResult<Option<ReloadSummary>> {
        let metadata = query_config_metadata(&self.inner.pool).await?;
        let last_version = self.inner.last_version.lock().await;
        if last_version.is_some_and(|v| v == metadata.version) {
            return Ok(None);
        }
        drop(last_version);
        self.reload().await.map(Some)
    }
}

pub struct ConfigPoller {
    handle: ConfigReloadHandle,
    interval: Duration,
    shutdown: watch::Receiver<bool>,
}

#[derive(Clone)]
pub struct ConfigPollerHandle {
    shutdown: watch::Sender<bool>,
}

impl ConfigPoller {
    pub fn new(handle: ConfigReloadHandle, interval: Duration) -> (Self, ConfigPollerHandle) {
        let (shutdown, receiver) = watch::channel(false);
        (
            Self {
                handle,
                interval,
                shutdown: receiver,
            },
            ConfigPollerHandle { shutdown },
        )
    }

    pub async fn run(mut self) {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match self.handle.reload_if_changed().await {
                        Ok(Some(_)) => {}
                        Ok(None) => debug!("Configuration version unchanged; skipping reload"),
                        Err(err) => error!("Configuration reload failed: {err}"),
                    }
                }
                _ = self.shutdown.changed() => {
                    if *self.shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }
}

impl ConfigPollerHandle {
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}
