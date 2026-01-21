//! Configuration types for Maptile RPC service

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Main configuration for Maptile RPC service
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaptileConfig {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// PostgreSQL connection configuration
    pub postgres: PostgresConfig,

    /// Configuration source settings
    #[serde(default)]
    pub config: ConfigSettings,
}

/// Server configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Listen address (default: 0.0.0.0:8089)
    #[serde(default = "default_listen_address")]
    pub listen_address: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
        }
    }
}

fn default_listen_address() -> String {
    "0.0.0.0:8089".to_string()
}

/// PostgreSQL connection configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PostgresConfig {
    /// PostgreSQL connection string
    pub connection_string: String,

    /// Connection pool size (default: 10)
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// SSL certificate path (optional)
    pub ssl_cert: Option<PathBuf>,

    /// SSL key path (optional)
    pub ssl_key: Option<PathBuf>,

    /// SSL root certificate path (optional)
    pub ssl_root_cert: Option<PathBuf>,
}

fn default_pool_size() -> usize {
    10
}

/// Configuration source settings
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfigSettings {
    /// Configuration source (always "database" for maptile)
    #[serde(default = "default_config_source")]
    pub source: String,

    /// Reload interval in seconds (optional, for hot reload)
    pub reload_interval_sec: Option<u64>,
}

impl Default for ConfigSettings {
    fn default() -> Self {
        Self {
            source: default_config_source(),
            reload_interval_sec: None,
        }
    }
}

fn default_config_source() -> String {
    "database".to_string()
}

/// Source type for data sources (PostgreSQL tables/functions)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Table,
    Function,
}

/// Row from martin_config.data_sources table
#[derive(Clone, Debug)]
pub struct DataSourceRow {
    pub source_id: String,
    pub source_type: SourceType,
    pub schema_name: String,
    pub table_or_function_name: String,
    pub geometry_column: Option<String>,
    pub srid: Option<i32>,
    pub id_column: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub enabled: bool,
}

impl DataSourceRow {
    /// Validate required fields based on source type
    pub fn validate(&self) -> Result<(), String> {
        if self.source_id.is_empty() {
            return Err("source_id cannot be empty".to_string());
        }
        if self.schema_name.is_empty() {
            return Err(format!(
                "schema_name cannot be empty for source '{}'",
                self.source_id
            ));
        }
        if self.table_or_function_name.is_empty() {
            return Err(format!(
                "table_or_function_name cannot be empty for source '{}'",
                self.source_id
            ));
        }

        // Table-specific validation
        if self.source_type == SourceType::Table && self.geometry_column.is_none() {
            return Err(format!(
                "geometry_column is required for table source '{}'",
                self.source_id
            ));
        }

        Ok(())
    }
}

/// Configuration metadata from martin_config.metadata table
#[derive(Clone, Debug)]
pub struct ConfigMetadata {
    pub version: i64,
    pub updated_at: std::time::SystemTime,
}
