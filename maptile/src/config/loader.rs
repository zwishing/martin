//! Configuration loading from file and database

use std::collections::HashSet;
use std::path::Path;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use log::{info, warn};
use martin_core::tiles::BoxedSource;
use martin_core::tiles::postgres::tls::{make_connector, parse_conn_str};
use thiserror::Error;

use super::{ConfigMetadata, DataSourceRow, MaptileConfig, PostgresConfig, SourceType};

/// Errors that can occur during configuration operations
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileReadError(#[from] std::io::Error),

    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Database query failed: {0}")]
    QueryFailed(String),

    #[error("Schema validation failed: {0}")]
    SchemaInvalid(String),

    #[error("No sources found in configuration")]
    NoSources,

    #[error("Source resolution failed: {0}")]
    SourceResolutionFailed(String),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

/// Loaded configuration with sources
#[derive(Clone)]
pub struct LoadedSources {
    pub metadata: ConfigMetadata,
    pub sources: Vec<BoxedSource>,
}

/// Load configuration from YAML file
pub async fn load_config(path: &Path) -> ConfigResult<MaptileConfig> {
    let content = tokio::fs::read_to_string(path).await?;
    let config: MaptileConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Create a database connection pool for configuration loading
pub async fn create_config_pool(config: &PostgresConfig) -> ConfigResult<Pool> {
    let (pg_cfg, ssl_mode) = parse_conn_str(&config.connection_string)
        .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };

    let mgr =
        if pg_cfg.get_ssl_mode() == deadpool_postgres::tokio_postgres::config::SslMode::Disable {
            Manager::from_config(pg_cfg, deadpool_postgres::tokio_postgres::NoTls, mgr_config)
        } else {
            let connector = make_connector(
                config.ssl_cert.as_ref(),
                config.ssl_key.as_ref(),
                config.ssl_root_cert.as_ref(),
                ssl_mode,
            )
            .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;
            Manager::from_config(pg_cfg, connector, mgr_config)
        };

    Pool::builder(mgr)
        .max_size(config.pool_size)
        .build()
        .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))
}

/// Validate that the required database schema exists
pub async fn validate_db_schema(pool: &Pool) -> ConfigResult<()> {
    let conn = pool
        .get()
        .await
        .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;

    let required_tables = [
        (
            "martin_config",
            "metadata",
            &["id", "version", "updated_at"][..],
        ),
        (
            "martin_config",
            "data_sources",
            &[
                "source_id",
                "source_type",
                "schema_name",
                "table_or_function_name",
                "geometry_column",
                "srid",
                "id_column",
                "properties",
                "enabled",
            ][..],
        ),
    ];

    for (schema, table, columns) in required_tables {
        let rows = conn
            .query(
                r#"
SELECT column_name
FROM information_schema.columns
WHERE table_schema = $1 AND table_name = $2
"#,
                &[&schema, &table],
            )
            .await
            .map_err(|e| ConfigError::QueryFailed(e.to_string()))?;

        if rows.is_empty() {
            return Err(ConfigError::SchemaInvalid(format!(
                "missing table {schema}.{table}"
            )));
        }

        let existing: HashSet<String> = rows.into_iter().map(|r| r.get("column_name")).collect();
        for col in columns {
            if !existing.contains(*col) {
                return Err(ConfigError::SchemaInvalid(format!(
                    "missing column {schema}.{table}.{col}"
                )));
            }
        }
    }

    Ok(())
}

/// Query configuration metadata from database
pub async fn query_config_metadata(pool: &Pool) -> ConfigResult<ConfigMetadata> {
    let conn = pool
        .get()
        .await
        .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;

    let row = conn
        .query_one(
            "SELECT version, updated_at FROM martin_config.metadata WHERE id = 1",
            &[],
        )
        .await
        .map_err(|e| ConfigError::QueryFailed(e.to_string()))?;

    Ok(ConfigMetadata {
        version: row.get("version"),
        updated_at: row.get("updated_at"),
    })
}

/// Query data sources from database
pub async fn query_data_sources(pool: &Pool) -> ConfigResult<Vec<DataSourceRow>> {
    let conn = pool
        .get()
        .await
        .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;

    let rows = conn
        .query(
            r#"
SELECT
  source_id,
  source_type,
  schema_name,
  table_or_function_name,
  geometry_column,
  srid,
  id_column,
  properties,
  enabled
FROM martin_config.data_sources
WHERE enabled = TRUE
ORDER BY source_id
"#,
            &[],
        )
        .await
        .map_err(|e| ConfigError::QueryFailed(e.to_string()))?;

    rows.into_iter()
        .map(|row| {
            let source_type: String = row.get("source_type");
            let source_type = match source_type.as_str() {
                "table" => SourceType::Table,
                "function" => SourceType::Function,
                other => {
                    return Err(ConfigError::QueryFailed(format!(
                        "unknown source_type '{other}'"
                    )));
                }
            };
            Ok(DataSourceRow {
                source_id: row.get("source_id"),
                source_type,
                schema_name: row.get("schema_name"),
                table_or_function_name: row.get("table_or_function_name"),
                geometry_column: row.get("geometry_column"),
                srid: row.get("srid"),
                id_column: row.get("id_column"),
                properties: row.get("properties"),
                enabled: row.get("enabled"),
            })
        })
        .collect()
}

/// Load sources from database configuration
pub async fn load_sources_from_database(
    config: &MaptileConfig,
    pool: &Pool,
) -> ConfigResult<LoadedSources> {
    validate_db_schema(pool).await?;

    let metadata = query_config_metadata(pool).await?;
    let data_sources = query_data_sources(pool).await?;

    if data_sources.is_empty() {
        return Err(ConfigError::NoSources);
    }

    info!(
        "Loaded {} data sources from database (version {})",
        data_sources.len(),
        metadata.version
    );

    // Build PostgreSQL sources using martin-core
    let sources = build_postgres_sources(config, pool, &data_sources).await?;

    Ok(LoadedSources { metadata, sources })
}

/// Build PostgreSQL sources from data source configurations
async fn build_postgres_sources(
    config: &MaptileConfig,
    _pool: &Pool,
    data_sources: &[DataSourceRow],
) -> ConfigResult<Vec<BoxedSource>> {
    use martin_core::tiles::postgres::{PostgresPool, PostgresSource, PostgresSqlInfo};

    let mut sources = Vec::new();

    // Create a PostgresPool using the connection string
    let pg_pool = PostgresPool::new(
        &config.postgres.connection_string,
        config.postgres.ssl_cert.as_ref(),
        config.postgres.ssl_key.as_ref(),
        config.postgres.ssl_root_cert.as_ref(),
        config.postgres.pool_size,
    )
    .await
    .map_err(|e| ConfigError::ConnectionFailed(e.to_string()))?;

    for row in data_sources {
        if let Err(err) = row.validate() {
            warn!("Skipping source '{}': {}", row.source_id, err);
            continue;
        }

        if !identifiers_valid(row) {
            warn!("Skipping source '{}': invalid identifier", row.source_id);
            continue;
        }

        match row.source_type {
            SourceType::Table => {
                let geometry_column = row.geometry_column.clone().ok_or_else(|| {
                    ConfigError::SourceResolutionFailed(format!(
                        "geometry_column is required for table source '{}'",
                        row.source_id
                    ))
                })?;

                // Build SQL query for the table source
                let sql_query = build_table_query(
                    &row.schema_name,
                    &row.table_or_function_name,
                    &geometry_column,
                    row.id_column.as_deref(),
                    row.srid.unwrap_or(4326),
                );

                let info = PostgresSqlInfo::new(
                    sql_query.clone(),
                    false, // no URL query params
                    format!("{}:{}", row.source_id, row.table_or_function_name),
                );

                // Build tilejson from properties
                let tilejson = build_tilejson(&row.source_id, row.properties.as_ref());

                let source =
                    PostgresSource::new(row.source_id.clone(), info, tilejson, pg_pool.clone());

                sources.push(Box::new(source) as BoxedSource);
                info!("Loaded table source: {}", row.source_id);
            }
            SourceType::Function => {
                // Build SQL query for function source
                let sql_query = build_function_query(&row.schema_name, &row.table_or_function_name);

                let info = PostgresSqlInfo::new(
                    sql_query.clone(),
                    true, // functions typically support URL query params
                    format!("{}:{}", row.source_id, row.table_or_function_name),
                );

                let tilejson = build_tilejson(&row.source_id, row.properties.as_ref());

                let source =
                    PostgresSource::new(row.source_id.clone(), info, tilejson, pg_pool.clone());

                sources.push(Box::new(source) as BoxedSource);
                info!("Loaded function source: {}", row.source_id);
            }
        }
    }

    if sources.is_empty() {
        return Err(ConfigError::NoSources);
    }

    Ok(sources)
}

fn identifiers_valid(row: &DataSourceRow) -> bool {
    if !is_valid_identifier(&row.schema_name)
        || !is_valid_identifier(&row.table_or_function_name)
    {
        return false;
    }

    if row.source_type == SourceType::Table {
        if let Some(geometry_column) = row.geometry_column.as_deref() {
            if !is_valid_identifier(geometry_column) {
                return false;
            }
        }
        if let Some(id_column) = row.id_column.as_deref() {
            if !is_valid_identifier(id_column) {
                return false;
            }
        }
    }

    true
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Build SQL query for table source
fn build_table_query(
    schema: &str,
    table: &str,
    geometry_column: &str,
    id_column: Option<&str>,
    _srid: i32,
) -> String {
    let (id_select_expr, id_mvt_expr) = id_column
        .map(|id| (format!(", {id}"), format!(", '{}'", id.replace('\'', "''"))))
        .unwrap_or_default();

    format!(
        r#"SELECT ST_AsMVT(tile, '{table}', 4096, 'geom'{id_mvt_expr}) FROM (
  SELECT ST_AsMVTGeom(
    ST_Transform({geometry_column}, 3857),
    ST_TileEnvelope($1::integer, $2::integer, $3::integer),
    4096, 64, true
  ) AS geom{id_select_expr}
  FROM "{schema}"."{table}"
  WHERE ST_Intersects(
    ST_Transform({geometry_column}, 4326),
    ST_Transform(ST_TileEnvelope($1::integer, $2::integer, $3::integer), 4326)
  )
) AS tile"#
    )
}

/// Build SQL query for function source
fn build_function_query(schema: &str, function: &str) -> String {
    format!(r#"SELECT * FROM "{schema}"."{function}"($1, $2, $3, $4)"#)
}

/// Build TileJSON from source properties
fn build_tilejson(source_id: &str, properties: Option<&serde_json::Value>) -> tilejson::TileJSON {
    let mut tj = tilejson::tilejson! {
        tiles: vec![],
        name: source_id.to_string(),
    };

    if let Some(props) = properties {
        if let Some(obj) = props.as_object() {
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                tj.name = Some(name.to_string());
            }
            if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                tj.description = Some(desc.to_string());
            }
            if let Some(attr) = obj.get("attribution").and_then(|v| v.as_str()) {
                tj.attribution = Some(attr.to_string());
            }
            if let Some(minzoom) = obj.get("minzoom").and_then(|v| v.as_u64()) {
                tj.minzoom = Some(minzoom as u8);
            }
            if let Some(maxzoom) = obj.get("maxzoom").and_then(|v| v.as_u64()) {
                tj.maxzoom = Some(maxzoom as u8);
            }
        }
    }

    tj
}

#[cfg(test)]
mod tests {
    use super::{identifiers_valid, is_valid_identifier};
    use crate::config::SourceType;
    use crate::config::DataSourceRow;

    #[test]
    fn identifier_validation_allows_safe_names() {
        assert!(is_valid_identifier("public"));
        assert!(is_valid_identifier("_geom"));
        assert!(is_valid_identifier("table_1"));
    }

    #[test]
    fn identifier_validation_rejects_invalid_names() {
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("1table"));
        assert!(!is_valid_identifier("has-dash"));
        assert!(!is_valid_identifier("has space"));
        assert!(!is_valid_identifier("schema.table"));
    }

    #[test]
    fn identifiers_valid_flags_invalid_columns() {
        let row = DataSourceRow {
            source_id: "source".to_string(),
            source_type: SourceType::Table,
            schema_name: "public".to_string(),
            table_or_function_name: "table".to_string(),
            geometry_column: Some("geom".to_string()),
            srid: Some(4326),
            id_column: Some("bad-column".to_string()),
            properties: None,
            enabled: true,
        };

        assert!(!identifiers_valid(&row));
    }
}
