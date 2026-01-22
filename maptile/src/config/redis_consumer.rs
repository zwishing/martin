//! Redis Stream consumer for dataset results

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::Pool;
use log::{error, info, warn};
use redis::streams::StreamReadReply;
use redis::FromRedisValue;
use serde::Deserialize;
use tokio::sync::{RwLock, watch};
use tokio::time::sleep;

use super::{MaptileConfig, RedisConfig, create_config_pool, load_sources_from_database};
use crate::server::MaptileServiceImpl;

const REDIS_STREAM: &str = "dataset:result";
const REDIS_CONSUMER_GROUP: &str = "dataset-result-maptile-consumers";
const REDIS_BLOCK_MS: u64 = 2000;
const REDIS_READ_COUNT: usize = 16;

#[derive(Debug)]
struct DatasetResultMessage {
    kind: String,
    payload: String,
    processed_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct VectorMetadata {
    pub uid: String,
    pub dataset_uid: String,
    pub tenant_uid: String,
    pub srid: i64,
    pub bound: [f64; 4],
    pub attributes_schema: HashMap<String, String>,
    pub geometry_type: String,
    pub feature_count: i64,
}

#[derive(Debug, PartialEq)]
struct VectorDataSource {
    source_id: String,
    schema_name: String,
    table_or_function_name: String,
    geometry_column: String,
    id_column: String,
    srid: Option<i32>,
    properties: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
enum ConsumerError {
    #[error("Redis error: {0}")]
    Redis(String),
    #[error("Missing field '{0}'")]
    MissingField(&'static str),
    #[error("Invalid field '{field}': {details}")]
    InvalidField { field: &'static str, details: String },
    #[error("Payload parse failed: {0}")]
    PayloadParse(String),
    #[error("Processed path is empty")]
    EmptyProcessedPath,
    #[error("Database error: {0}")]
    Database(String),
}

pub async fn start_redis_consumer_task(
    service: Arc<RwLock<MaptileServiceImpl>>,
    config: MaptileConfig,
    redis_config: RedisConfig,
    shutdown: watch::Receiver<bool>,
) {
    info!(
        "Starting Redis consumer for stream '{}' (group '{}')",
        REDIS_STREAM, REDIS_CONSUMER_GROUP
    );

    let pool = match create_config_pool(&config.postgres).await {
        Ok(pool) => pool,
        Err(err) => {
            error!("Failed to create database pool for Redis consumer: {err}");
            return;
        }
    };

    let consumer_name = redis_config
        .consumer_name
        .clone()
        .unwrap_or_else(|| format!("maptile-{}", std::process::id()));

    loop {
        if *shutdown.borrow() {
            info!("Stopping Redis consumer task");
            break;
        }

        let mut connection = match connect_redis(&redis_config).await {
            Ok(connection) => connection,
            Err(err) => {
                error!("Redis connection failed: {err}");
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        if let Err(err) = ensure_consumer_group(&mut connection).await {
            error!("Failed to ensure Redis consumer group: {err}");
            sleep(Duration::from_secs(2)).await;
            continue;
        }

        loop {
            if *shutdown.borrow() {
                info!("Stopping Redis consumer task");
                return;
            }

            match read_stream_entries(&mut connection, &consumer_name).await {
                Ok(entries) => {
                    for entry in entries {
                        let result = handle_entry(&pool, &service, &config, &entry).await;
                        if let Err(err) = result {
                            warn!("Failed to handle Redis message {}: {err}", entry.id);
                        }
                        if let Err(err) = acknowledge_message(&mut connection, &entry.id).await {
                            warn!("Failed to acknowledge Redis message {}: {err}", entry.id);
                        }
                    }
                }
                Err(err) => {
                    warn!("Redis read failed: {err}");
                    break;
                }
            }
        }
    }
}

async fn connect_redis(
    redis_config: &RedisConfig,
) -> Result<redis::aio::MultiplexedConnection, ConsumerError> {
    let client = redis::Client::open(redis_config.url.as_str())
        .map_err(|err| ConsumerError::Redis(err.to_string()))?;
    client
        .get_multiplexed_async_connection()
        .await
        .map_err(|err| ConsumerError::Redis(err.to_string()))
}

async fn ensure_consumer_group(
    connection: &mut redis::aio::MultiplexedConnection,
) -> Result<(), ConsumerError> {
    let result = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(REDIS_STREAM)
        .arg(REDIS_CONSUMER_GROUP)
        .arg("0")
        .arg("MKSTREAM")
        .query_async::<()>(connection)
        .await;

    match result {
        Ok(()) => {
            info!("Created Redis consumer group '{REDIS_CONSUMER_GROUP}'");
            Ok(())
        }
        Err(err) => {
            if err.to_string().contains("BUSYGROUP") {
                Ok(())
            } else {
                Err(ConsumerError::Redis(err.to_string()))
            }
        }
    }
}

struct StreamEntry {
    id: String,
    fields: HashMap<String, redis::Value>,
}

async fn read_stream_entries(
    connection: &mut redis::aio::MultiplexedConnection,
    consumer_name: &str,
) -> Result<Vec<StreamEntry>, ConsumerError> {
    let response: Option<StreamReadReply> = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(REDIS_CONSUMER_GROUP)
        .arg(consumer_name)
        .arg("BLOCK")
        .arg(REDIS_BLOCK_MS)
        .arg("COUNT")
        .arg(REDIS_READ_COUNT)
        .arg("STREAMS")
        .arg(REDIS_STREAM)
        .arg(">")
        .query_async(connection)
        .await
        .map_err(|err| ConsumerError::Redis(err.to_string()))?;

    let Some(response) = response else {
        return Ok(Vec::new());
    };

    let entries = response
        .keys
        .into_iter()
        .flat_map(|stream_key| stream_key.ids)
        .map(|stream_id| StreamEntry {
            id: stream_id.id,
            fields: stream_id.map,
        })
        .collect();

    Ok(entries)
}

async fn acknowledge_message(
    connection: &mut redis::aio::MultiplexedConnection,
    entry_id: &str,
) -> Result<(), ConsumerError> {
    redis::cmd("XACK")
        .arg(REDIS_STREAM)
        .arg(REDIS_CONSUMER_GROUP)
        .arg(entry_id)
        .query_async::<()>(connection)
        .await
        .map_err(|err| ConsumerError::Redis(err.to_string()))
}

async fn handle_entry(
    pool: &Pool,
    service: &Arc<RwLock<MaptileServiceImpl>>,
    config: &MaptileConfig,
    entry: &StreamEntry,
) -> Result<(), ConsumerError> {
    let message = parse_stream_fields(&entry.fields)?;

    if message.kind != "vector" {
        info!(
            "Skipping Redis message {} with kind '{}'",
            entry.id, message.kind
        );
        return Ok(());
    }

    let metadata = parse_vector_metadata(&message.payload)?;
    let processed_path = resolve_processed_path(&message)?;
    let data_source = VectorDataSource::from_metadata(metadata, &processed_path)?;

    write_vector_source(pool, &data_source).await?;
    refresh_sources(service, config, pool).await?;

    info!(
        "Applied vector source '{}' from Redis message {}",
        data_source.source_id, entry.id
    );

    Ok(())
}

fn parse_stream_fields(
    fields: &HashMap<String, redis::Value>,
) -> Result<DatasetResultMessage, ConsumerError> {
    let kind = parse_string_field(fields, "kind")?;
    let payload = parse_string_field(fields, "payload")?;
    let processed_path = parse_optional_string_field(fields, "processed_path")?;

    Ok(DatasetResultMessage {
        kind,
        payload,
        processed_path,
    })
}

fn parse_string_field(
    fields: &HashMap<String, redis::Value>,
    field: &'static str,
) -> Result<String, ConsumerError> {
    let value = fields
        .get(field)
        .ok_or(ConsumerError::MissingField(field))?;
    String::from_redis_value(value).map_err(|err| ConsumerError::InvalidField {
        field,
        details: err.to_string(),
    })
}

fn parse_optional_string_field(
    fields: &HashMap<String, redis::Value>,
    field: &'static str,
) -> Result<Option<String>, ConsumerError> {
    let Some(value) = fields.get(field) else {
        return Ok(None);
    };
    String::from_redis_value(value)
        .map(Some)
        .map_err(|err| ConsumerError::InvalidField {
            field,
            details: err.to_string(),
        })
}

fn parse_vector_metadata(payload: &str) -> Result<VectorMetadata, ConsumerError> {
    serde_json::from_str(payload).map_err(|err| ConsumerError::PayloadParse(err.to_string()))
}

fn resolve_processed_path(message: &DatasetResultMessage) -> Result<String, ConsumerError> {
    if let Some(processed_path) = message.processed_path.clone() {
        if !processed_path.trim().is_empty() {
            return Ok(processed_path);
        }
        return Err(ConsumerError::EmptyProcessedPath);
    }

    let value: serde_json::Value = serde_json::from_str(&message.payload)
        .map_err(|err| ConsumerError::PayloadParse(err.to_string()))?;
    let processed_path = value
        .get("processed_path")
        .or_else(|| value.get("ProcessedPath"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or(ConsumerError::MissingField("processed_path"))?;

    if processed_path.trim().is_empty() {
        return Err(ConsumerError::EmptyProcessedPath);
    }

    Ok(processed_path)
}

impl VectorDataSource {
    fn from_metadata(
        metadata: VectorMetadata,
        processed_path: &str,
    ) -> Result<Self, ConsumerError> {
        if processed_path.trim().is_empty() {
            return Err(ConsumerError::EmptyProcessedPath);
        }
        let (schema_name, table_or_function_name) = parse_processed_path(processed_path)?;
        let srid = i32::try_from(metadata.srid).map_err(|err| ConsumerError::InvalidField {
            field: "srid",
            details: err.to_string(),
        })?;
        let properties = serde_json::json!({
            "attributes_schema": metadata.attributes_schema,
        });
        Ok(Self {
            source_id: processed_path.to_string(),
            schema_name,
            table_or_function_name,
            geometry_column: "geom".to_string(),
            id_column: "gid".to_string(),
            srid: Some(srid),
            properties: Some(properties),
        })
    }
}

fn parse_processed_path(processed_path: &str) -> Result<(String, String), ConsumerError> {
    let mut parts = processed_path.split('.');
    let schema_name = parts
        .next()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .ok_or(ConsumerError::EmptyProcessedPath)?;
    let table_or_function_name = parts
        .next()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .ok_or(ConsumerError::EmptyProcessedPath)?;

    if parts.next().is_some() {
        return Err(ConsumerError::InvalidField {
            field: "processed_path",
            details: "expected schema.table".to_string(),
        });
    }

    Ok((schema_name.to_string(), table_or_function_name.to_string()))
}

async fn write_vector_source(pool: &Pool, data_source: &VectorDataSource) -> Result<(), ConsumerError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;
    let transaction = conn
        .transaction()
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;

    let sql = r#"
INSERT INTO martin_config.data_sources
  (source_id, source_type, schema_name, table_or_function_name, geometry_column, srid, id_column, properties, enabled)
VALUES
  ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)
ON CONFLICT (source_id) DO UPDATE SET
  source_type = EXCLUDED.source_type,
  schema_name = EXCLUDED.schema_name,
  table_or_function_name = EXCLUDED.table_or_function_name,
  geometry_column = EXCLUDED.geometry_column,
  srid = EXCLUDED.srid,
  id_column = EXCLUDED.id_column,
  properties = EXCLUDED.properties,
  enabled = TRUE
"#;

    transaction
        .execute(
            sql,
            &[
                &data_source.source_id,
                &"table",
                &data_source.schema_name,
                &data_source.table_or_function_name,
                &data_source.geometry_column,
                &data_source.srid,
                &data_source.id_column,
                &data_source.properties,
            ],
        )
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;

    transaction
        .execute(
            "UPDATE martin_config.metadata SET version = version + 1, updated_at = NOW() WHERE id = 1",
            &[],
        )
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;

    transaction
        .commit()
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;

    Ok(())
}

async fn refresh_sources(
    service: &Arc<RwLock<MaptileServiceImpl>>,
    config: &MaptileConfig,
    pool: &Pool,
) -> Result<(), ConsumerError> {
    let loaded = load_sources_from_database(config, pool)
        .await
        .map_err(|err| ConsumerError::Database(err.to_string()))?;

    {
        let mut service_guard = service.write().await;
        service_guard.update_sources(loaded.sources);
        service_guard.update_metadata(loaded.metadata);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use redis::Value;

    use super::{VectorDataSource, parse_stream_fields, parse_vector_metadata};

    #[test]
    fn parse_stream_fields_extracts_message() {
        let mut fields = HashMap::new();
        fields.insert("kind".to_string(), Value::BulkString(b"vector".to_vec()));
        fields.insert(
            "payload".to_string(),
            Value::BulkString(
                br#"{"uid":"u1","dataset_uid":"d1","tenant_uid":"t1","srid":4326,"bound":[0.0,0.0,1.0,1.0],"attributes_schema":{},"geometry_type":"MULTIPOLYGON","feature_count":1}"#.to_vec(),
            ),
        );
        fields.insert(
            "processed_path".to_string(),
            Value::BulkString(b"vector.roads".to_vec()),
        );

        let message = parse_stream_fields(&fields).expect("message");

        assert_eq!(message.kind, "vector");
        assert!(message.payload.contains("\"uid\":\"u1\""));
        assert_eq!(message.processed_path, Some("vector.roads".to_string()));
    }

    #[test]
    fn vector_metadata_builds_data_source() {
        let payload = r#"{"uid":"u1","dataset_uid":"d1","tenant_uid":"t1","srid":4326,"bound":[0.0,0.0,1.0,1.0],"attributes_schema":{},"geometry_type":"MULTIPOLYGON","feature_count":1}"#;
        let metadata = parse_vector_metadata(payload).expect("metadata");
        let data_source = VectorDataSource::from_metadata(metadata, "vector.roads")
            .expect("data source");

        assert_eq!(data_source.source_id, "vector.roads");
        assert_eq!(data_source.schema_name, "vector");
        assert_eq!(data_source.table_or_function_name, "roads");
        assert_eq!(data_source.geometry_column, "geom");
        assert_eq!(data_source.id_column, "gid");
        assert_eq!(data_source.srid, Some(4326));
        assert!(data_source.properties.is_none());
    }
}
