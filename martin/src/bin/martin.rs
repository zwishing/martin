use std::env;

use clap::Parser;
use martin::MartinResult;
use martin::config::args::Args;
use martin::config::file::{Config, read_config};
#[cfg(feature = "postgres")]
use martin::config::database::{
    ConfigPoller, create_config_schema, export_config_to_db, validate_db_config,
};
use martin::logging::{ensure_martin_core_log_level_matches, init_tracing};
use martin::srv::new_server;
use martin_core::config::env::OsEnv;
use tracing::{error, info};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

async fn start(args: Args) -> MartinResult<()> {
    info!("Starting Martin v{VERSION}");

    let env = OsEnv::default();
    let save_config = args.meta.save_config.clone();
    let mut config = if let Some(ref cfg_filename) = args.meta.config {
        info!("Using {}", cfg_filename.display());
        read_config(cfg_filename, &env)?
    } else {
        info!("Config file is not specified, auto-detecting sources");
        Config::default()
    };

    args.merge_into_config(&mut config, &env)?;
    if let Err(err) = config.finalize() {
        if matches!(
            err,
            martin::MartinError::ConfigFileError(
                martin::config::file::ConfigFileError::NoSources
            )
        ) && (args.meta.create_config_schema
            || args.meta.export_config_to_db
            || args.meta.validate_db_config)
        {
            info!("No tile sources configured; continuing for admin command.");
        } else {
            return Err(err);
        }
    }

    #[cfg(feature = "postgres")]
    if args.meta.create_config_schema
        || args.meta.export_config_to_db
        || args.meta.validate_db_config
    {
        let (connection_string, ssl_certs, pool_size) = config.config_database_settings()?;
        let pool = martin::config::database::create_config_pool(
            &connection_string,
            ssl_certs.and_then(|c| c.ssl_cert.as_ref()),
            ssl_certs.and_then(|c| c.ssl_key.as_ref()),
            ssl_certs.and_then(|c| c.ssl_root_cert.as_ref()),
            pool_size,
        )
        .await?;

        if args.meta.create_config_schema {
            create_config_schema(&pool).await?;
            info!("Configuration schema created.");
        }
        if args.meta.export_config_to_db {
            let summary = export_config_to_db(&mut config, &pool, args.meta.overwrite).await?;
            info!(
                "Exported {} data sources and {} file sources.",
                summary.data_sources, summary.file_sources
            );
        }
        if args.meta.validate_db_config {
            let count = validate_db_config(&config, &pool).await?;
            info!("Database configuration is valid ({} sources).", count);
        }
        return Ok(());
    }
    let config_source = config.config_source;
    let refresh_interval = config
        .config_refresh_interval_seconds
        .unwrap_or(60);

    let sources = config.resolve().await?;
    #[cfg(feature = "postgres")]
    let reload_handle = sources.config_reload.clone();

    if let Some(file_name) = save_config {
        config.save_to_file(file_name.as_path())?;
    } else {
        info!("Use --save-config to save or print Martin configuration.");
    }

    #[cfg(all(feature = "webui", not(docsrs)))]
    let web_ui_mode = config.srv.web_ui.unwrap_or_default();

    let (server, listen_addresses) = new_server(config.srv, sources)?;
    info!("Martin has been started on {listen_addresses}.");
    info!("Use http://{listen_addresses}/catalog to get the list of available sources.");

    #[cfg(all(feature = "webui", not(docsrs)))]
    if web_ui_mode == martin::config::args::WebUiMode::EnableForAll {
        log::warn!("Web UI is enabled for all connections at http://{listen_addresses}/");
    } else {
        info!(
            "Web UI is disabled. Use `--webui enable-for-all` in CLI or a config value to enable it for all connections."
        );
    }

    #[cfg(feature = "postgres")]
    if config_source.is_database() {
        let Some(reload_handle) = reload_handle else {
            return Err(martin::MartinError::ConfigError(
                "configuration reload handle was not initialized".to_string(),
            ));
        };
        let (poller, poller_handle) =
            ConfigPoller::new(reload_handle, Duration::from_secs(refresh_interval));
        let poller_task = tokio::spawn(poller.run());
        let res = server.await;
        poller_handle.shutdown();
        let _ = poller_task.await;
        return res;
    }

    server.await
}

#[tokio::main]
async fn main() {
    let filter = ensure_martin_core_log_level_matches(env::var("RUST_LOG").ok(), "martin=");
    init_tracing(&filter, env::var("RUST_LOG_FORMAT").ok());

    let args = Args::parse();
    if let Err(e) = start(args).await {
        // Ensure the message is printed, even if the logging is disabled
        if tracing::event_enabled!(tracing::Level::ERROR) {
            error!("{e}");
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
}
