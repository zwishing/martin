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

    /// Redis configuration (optional)
    #[serde(default)]
    pub redis: Option<RedisConfig>,
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

    /// Auto-generate filtered tile functions at startup (default: false)
    #[serde(default)]
    pub auto_generate_filters: bool,

    /// Suffix for auto-generated filtered functions (default: "filtered")
    #[serde(default = "default_filter_suffix")]
    pub filter_function_suffix: String,
}

fn default_pool_size() -> usize {
    10
}

fn default_filter_suffix() -> String {
    "filtered".to_string()
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

/// Redis connection configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,

    /// Optional consumer name (defaults to process-based name)
    #[serde(default)]
    pub consumer_name: Option<String>,
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
                "schema_name cannot be empty for source '{source_id}'",
                source_id = self.source_id
            ));
        }
        if self.table_or_function_name.is_empty() {
            return Err(format!(
                "table_or_function_name cannot be empty for source '{source_id}'",
                source_id = self.source_id
            ));
        }

        // Table-specific validation
        if self.source_type == SourceType::Table && self.geometry_column.is_none() {
            return Err(format!(
                "geometry_column is required for table source '{source_id}'",
                source_id = self.source_id
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_generate_filters_default() {
        // Test that auto_generate_filters defaults to false
        let yaml = r#"
connection_string: "postgresql://localhost/test"
"#;
        let config: PostgresConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.auto_generate_filters);
        assert_eq!(config.filter_function_suffix, "filtered");
    }

    #[test]
    fn test_auto_generate_filters_enabled() {
        // Test that custom values are parsed correctly
        let yaml = r#"
connection_string: "postgresql://localhost/test"
auto_generate_filters: true
filter_function_suffix: "custom_suffix"
"#;
        let config: PostgresConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.auto_generate_filters);
        assert_eq!(config.filter_function_suffix, "custom_suffix");
    }

    #[test]
    fn test_backward_compatibility() {
        // Test that old configs without new fields still work
        let yaml = r#"
connection_string: "postgresql://localhost/test"
pool_size: 20
"#;
        let config: PostgresConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.auto_generate_filters); // Should default to false
        assert_eq!(config.filter_function_suffix, "filtered"); // Should use default
        assert_eq!(config.pool_size, 20);
    }

    #[test]
    fn test_filter_suffix_default_only() {
        // Test enabling auto_generate_filters without custom suffix
        let yaml = r#"
connection_string: "postgresql://localhost/test"
auto_generate_filters: true
"#;
        let config: PostgresConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.auto_generate_filters);
        assert_eq!(config.filter_function_suffix, "filtered"); // Should use default
    }
}
