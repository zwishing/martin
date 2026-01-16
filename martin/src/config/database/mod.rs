// Database-driven configuration module
//
// This module provides database-backed configuration storage and loading
// for dynamic tile source management.

mod types;
pub use types::{
    ConfigMetadata, ConfigSource, ConfigStatus, ConfigStatusHandle, DataSourceRow, FileSourceRow,
    FileSourceType, SourceType,
};

#[cfg(feature = "postgres")]
mod loader;
#[cfg(feature = "postgres")]
pub use loader::{LoadedConfig, create_config_pool, load_config_from_database, query_config_metadata, validate_db_schema};
#[cfg(feature = "postgres")]
pub use loader::{create_config_schema, export_config_to_db, validate_db_config, ExportSummary};

#[cfg(feature = "postgres")]
mod poller;
#[cfg(feature = "postgres")]
pub use poller::{ConfigPoller, ConfigReloadHandle, ReloadSummary};

mod error;
pub use error::{DatabaseConfigError, DatabaseConfigResult};
