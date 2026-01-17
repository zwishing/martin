use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;

/// Configuration source mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSource {
    /// Load configuration from YAML file (default)
    #[default]
    File,
    /// Load configuration from database tables
    Database,
}

impl ConfigSource {
    pub fn is_database(self) -> bool {
        matches!(self, Self::Database)
    }

    pub fn is_file(&self) -> bool {
        matches!(self, Self::File)
    }
}

/// Source type for data sources (PostgreSQL tables/functions)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Table,
    Function,
}

/// File source type (MBTiles, PMTiles, COG)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSourceType {
    Mbtiles,
    Pmtiles,
    Cog,
}

/// Configuration metadata from martin_config.metadata table
#[derive(Clone, Debug)]
pub struct ConfigMetadata {
    pub version: i64,
    pub updated_at: SystemTime,
}

/// Runtime configuration status exposed via /health.
#[derive(Clone, Debug)]
pub struct ConfigStatus {
    pub config_source: ConfigSource,
    pub config_version: Option<i64>,
    pub last_config_reload: Option<SystemTime>,
}

impl ConfigStatus {
    pub fn new(config_source: ConfigSource) -> Self {
        Self {
            config_source,
            config_version: None,
            last_config_reload: None,
        }
    }
}

pub type ConfigStatusHandle = Arc<RwLock<ConfigStatus>>;

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

/// Row from martin_config.file_sources table
#[derive(Clone, Debug)]
pub struct FileSourceRow {
    pub source_id: String,
    pub source_type: FileSourceType,
    pub file_path: String,
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
        if self.source_type == SourceType::Table {
            if self.geometry_column.is_none() {
                return Err(format!(
                    "geometry_column is required for table source '{}'",
                    self.source_id
                ));
            }
        }

        Ok(())
    }
}

impl FileSourceRow {
    /// Validate required fields
    pub fn validate(&self) -> Result<(), String> {
        if self.source_id.is_empty() {
            return Err("source_id cannot be empty".to_string());
        }
        if self.file_path.is_empty() {
            return Err(format!(
                "file_path cannot be empty for source '{}'",
                self.source_id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_source_default() {
        assert_eq!(ConfigSource::default(), ConfigSource::File);
    }

    #[test]
    fn test_config_source_is_database() {
        assert!(ConfigSource::Database.is_database());
        assert!(!ConfigSource::File.is_database());
    }

    #[test]
    fn test_data_source_validation() {
        let valid_table = DataSourceRow {
            source_id: "test".to_string(),
            source_type: SourceType::Table,
            schema_name: "public".to_string(),
            table_or_function_name: "points".to_string(),
            geometry_column: Some("geom".to_string()),
            srid: Some(4326),
            id_column: Some("id".to_string()),
            properties: None,
            enabled: true,
        };
        assert!(valid_table.validate().is_ok());

        let invalid_table = DataSourceRow {
            source_id: "test".to_string(),
            source_type: SourceType::Table,
            schema_name: "public".to_string(),
            table_or_function_name: "points".to_string(),
            geometry_column: None, // Missing required field
            srid: Some(4326),
            id_column: Some("id".to_string()),
            properties: None,
            enabled: true,
        };
        assert!(invalid_table.validate().is_err());
    }

    #[test]
    fn test_file_source_validation() {
        let valid = FileSourceRow {
            source_id: "test".to_string(),
            source_type: FileSourceType::Mbtiles,
            file_path: "/path/to/file.mbtiles".to_string(),
            properties: None,
            enabled: true,
        };
        assert!(valid.validate().is_ok());

        let invalid = FileSourceRow {
            source_id: "".to_string(), // Empty source_id
            source_type: FileSourceType::Mbtiles,
            file_path: "/path/to/file.mbtiles".to_string(),
            properties: None,
            enabled: true,
        };
        assert!(invalid.validate().is_err());
    }
}
