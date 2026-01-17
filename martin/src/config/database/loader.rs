use std::collections::HashSet;
use std::path::PathBuf;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use martin_core::config::IdResolver;
use martin_core::config::OptBoolObj;
use martin_core::tiles::BoxedSource;
use serde_json::Value;

use crate::config::database::{
    ConfigMetadata, DataSourceRow, DatabaseConfigError, DatabaseConfigResult, FileSourceRow,
    FileSourceType, SourceType,
};
use crate::config::file::postgres::utils::patch_json;
use crate::config::file::postgres::{FuncInfoSources, FunctionInfo, TableInfo, TableInfoSources};
use crate::config::file::{Config, ConfigFileError, TileSourceWarning};
use crate::config::file::{FileConfigEnum, FileConfigSrc, TileSourceConfiguration};
use crate::source::TileSources;
use crate::srv::RESERVED_KEYWORDS;

use martin_core::tiles::Source;
use tilejson::TileJSON;

#[cfg(feature = "postgres")]
use crate::config::file::postgres::PostgresConfig;

#[cfg(feature = "postgres")]
use martin_core::tiles::postgres::tls::{make_connector, parse_conn_str};

#[cfg(feature = "postgres")]
use deadpool_postgres::tokio_postgres::NoTls;

const CONFIG_SCHEMA_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/sql/config_schema.sql"
));

#[derive(Debug, serde::Serialize)]
pub struct ExportSummary {
    pub data_sources: usize,
    pub file_sources: usize,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub metadata: ConfigMetadata,
    pub sources: TileSources,
    pub warnings: Vec<TileSourceWarning>,
}

#[cfg(feature = "postgres")]
pub async fn validate_db_schema(pool: &Pool) -> DatabaseConfigResult<()> {
    let conn = pool.get().await?;
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
        (
            "martin_config",
            "file_sources",
            &[
                "source_id",
                "source_type",
                "file_path",
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
            .await?;

        if rows.is_empty() {
            return Err(DatabaseConfigError::SchemaInvalid(format!(
                "missing table {schema}.{table}"
            )));
        }

        let existing: std::collections::HashSet<String> =
            rows.into_iter().map(|r| r.get("column_name")).collect();
        for col in columns {
            if !existing.contains(*col) {
                return Err(DatabaseConfigError::SchemaInvalid(format!(
                    "missing column {schema}.{table}.{col}"
                )));
            }
        }
    }

    Ok(())
}

#[cfg(feature = "postgres")]
pub async fn load_config_from_database(
    config: &Config,
    pool: &Pool,
    id_resolver: &IdResolver,
    #[cfg(feature = "pmtiles")] pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
) -> DatabaseConfigResult<LoadedConfig> {
    validate_db_schema(pool).await?;

    let metadata = query_config_metadata(pool).await?;
    let data_sources = query_data_sources(pool).await?;
    let file_sources = query_file_sources(pool).await?;

    let (sources, warnings) = build_sources_from_database(
        config,
        id_resolver,
        data_sources,
        file_sources,
        #[cfg(feature = "pmtiles")]
        pmtiles_cache,
    )
    .await?;

    Ok(LoadedConfig {
        metadata,
        sources,
        warnings,
    })
}

#[cfg(feature = "postgres")]
pub async fn query_config_metadata(pool: &Pool) -> DatabaseConfigResult<ConfigMetadata> {
    let conn = pool.get().await?;
    let row = conn
        .query_one(
            "SELECT version, updated_at FROM martin_config.metadata WHERE id = 1",
            &[],
        )
        .await?;
    Ok(ConfigMetadata {
        version: row.get("version"),
        updated_at: row.get("updated_at"),
    })
}

#[cfg(feature = "postgres")]
pub async fn query_data_sources(pool: &Pool) -> DatabaseConfigResult<Vec<DataSourceRow>> {
    let conn = pool.get().await?;
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
        .await?;

    rows.into_iter()
        .map(|row| {
            let source_type: String = row.get("source_type");
            let source_type = match source_type.as_str() {
                "table" => SourceType::Table,
                "function" => SourceType::Function,
                other => {
                    return Err(DatabaseConfigError::DeserializationFailed(format!(
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

#[cfg(feature = "postgres")]
pub async fn query_file_sources(pool: &Pool) -> DatabaseConfigResult<Vec<FileSourceRow>> {
    let conn = pool.get().await?;
    let rows = conn
        .query(
            r#"
SELECT
  source_id,
  source_type,
  file_path,
  properties,
  enabled
FROM martin_config.file_sources
WHERE enabled = TRUE
ORDER BY source_id
"#,
            &[],
        )
        .await?;

    rows.into_iter()
        .map(|row| {
            let source_type: String = row.get("source_type");
            let source_type = match source_type.as_str() {
                "mbtiles" => FileSourceType::Mbtiles,
                "pmtiles" => FileSourceType::Pmtiles,
                "cog" => FileSourceType::Cog,
                other => {
                    return Err(DatabaseConfigError::DeserializationFailed(format!(
                        "unknown source_type '{other}'"
                    )));
                }
            };
            Ok(FileSourceRow {
                source_id: row.get("source_id"),
                source_type,
                file_path: row.get("file_path"),
                properties: row.get("properties"),
                enabled: row.get("enabled"),
            })
        })
        .collect()
}

#[cfg(feature = "postgres")]
async fn build_sources_from_database(
    config: &Config,
    id_resolver: &IdResolver,
    data_sources: Vec<DataSourceRow>,
    file_sources: Vec<FileSourceRow>,
    #[cfg(feature = "pmtiles")] pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
) -> DatabaseConfigResult<(TileSources, Vec<TileSourceWarning>)> {
    let mut warnings = Vec::new();
    let mut sources: Vec<Vec<BoxedSource>> = Vec::new();

    let mut seen_ids = HashSet::new();
    let (tables, functions) = build_postgres_config(&data_sources, &mut warnings, &mut seen_ids)?;

    if !tables.is_empty() || !functions.is_empty() {
        let mut pg_config = extract_single_postgres_config(config)?;
        pg_config.tables = Some(tables);
        pg_config.functions = Some(functions);
        pg_config.auto_publish = OptBoolObj::Bool(false);

        let (pg_sources, pg_warnings) = pg_config
            .resolve(id_resolver.clone())
            .await
            .map_err(|e| DatabaseConfigError::QueryFailed(e.to_string()))?;
        warnings.extend(pg_warnings);
        sources.push(pg_sources);
    }

    let file_config_sources = build_file_sources_from_database(
        config,
        id_resolver,
        file_sources,
        &mut seen_ids,
        #[cfg(feature = "pmtiles")]
        pmtiles_cache,
    )
    .await
    .map_err(|e| DatabaseConfigError::ValidationFailed(e.to_string()))?;
    warnings.extend(file_config_sources.1);
    if !file_config_sources.0.is_empty() {
        sources.push(file_config_sources.0);
    }

    let all_sources = TileSources::new(sources);
    if all_sources.source_names().is_empty() {
        return Err(DatabaseConfigError::NoSources);
    }

    Ok((all_sources, warnings))
}

#[cfg(feature = "postgres")]
fn build_postgres_config(
    rows: &[DataSourceRow],
    warnings: &mut Vec<TileSourceWarning>,
    seen_ids: &mut HashSet<String>,
) -> DatabaseConfigResult<(TableInfoSources, FuncInfoSources)> {
    let mut tables = TableInfoSources::new();
    let mut functions = FuncInfoSources::new();
    for row in rows {
        if let Err(err) = row.validate() {
            warnings.push(TileSourceWarning::SourceError {
                source_id: row.source_id.clone(),
                error: err,
            });
            continue;
        }

        if !seen_ids.insert(row.source_id.clone()) {
            warnings.push(TileSourceWarning::SourceError {
                source_id: row.source_id.clone(),
                error: format!("duplicate source_id '{}'", row.source_id),
            });
            continue;
        }

        match row.source_type {
            SourceType::Table => {
                let geometry_column = row.geometry_column.clone().ok_or_else(|| {
                    DatabaseConfigError::ValidationFailed(format!(
                        "geometry_column is required for table source '{}'",
                        row.source_id
                    ))
                })?;
                let info = TableInfo {
                    schema: row.schema_name.clone(),
                    table: row.table_or_function_name.clone(),
                    geometry_column,
                    srid: row.srid.unwrap_or(0),
                    id_column: row.id_column.clone(),
                    tilejson: row.properties.clone(),
                    ..Default::default()
                };
                tables.insert(row.source_id.clone(), info);
            }
            SourceType::Function => {
                let info = FunctionInfo::new(
                    row.schema_name.clone(),
                    row.table_or_function_name.clone(),
                    row.properties.clone(),
                );
                functions.insert(row.source_id.clone(), info);
            }
        }
    }

    Ok((tables, functions))
}

#[cfg(feature = "postgres")]
async fn build_file_sources_from_database(
    config: &Config,
    id_resolver: &IdResolver,
    rows: Vec<FileSourceRow>,
    seen_ids: &mut HashSet<String>,
    #[cfg(feature = "pmtiles")] pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
) -> Result<(Vec<BoxedSource>, Vec<TileSourceWarning>), ConfigFileError> {
    let mut warnings = Vec::new();
    let mut sources = Vec::new();

    for row in rows {
        if let Err(err) = row.validate() {
            warnings.push(TileSourceWarning::SourceError {
                source_id: row.source_id.clone(),
                error: err,
            });
            continue;
        }
        if !seen_ids.insert(row.source_id.clone()) {
            warnings.push(TileSourceWarning::SourceError {
                source_id: row.source_id.clone(),
                error: format!("duplicate source_id '{}'", row.source_id),
            });
            continue;
        }

        match create_file_source(
            config,
            id_resolver,
            &row,
            #[cfg(feature = "pmtiles")]
            pmtiles_cache.clone(),
        )
        .await
        {
            Ok((resolved_id, source)) => {
                let source = wrap_with_tilejson_override(source, &row.properties);
                sources.push(source);
                let _ = resolved_id;
            }
            Err(err) => warnings.push(TileSourceWarning::SourceError {
                source_id: row.source_id.clone(),
                error: err,
            }),
        }
    }

    Ok((sources, warnings))
}

fn wrap_with_tilejson_override(source: BoxedSource, patch: &Option<Value>) -> BoxedSource {
    let Some(patch) = patch else {
        return source;
    };
    let tilejson = patch_json(source.get_tilejson().clone(), Some(patch));
    Box::new(TilejsonOverrideSource {
        inner: source,
        tilejson,
    })
}

#[derive(Clone, Debug)]
struct TilejsonOverrideSource {
    inner: BoxedSource,
    tilejson: TileJSON,
}

#[cfg(feature = "pmtiles")]
fn update_pmtiles_cache(
    mut custom: crate::config::file::pmtiles::PmtConfig,
    cache: Option<martin_core::tiles::pmtiles::PmtCache>,
) -> crate::config::file::pmtiles::PmtConfig {
    if let Some(cache) = cache {
        custom.pmtiles_directory_cache = cache;
    }
    custom
}

async fn create_file_source(
    config: &Config,
    id_resolver: &IdResolver,
    row: &FileSourceRow,
    #[cfg(feature = "pmtiles")] pmtiles_cache: Option<martin_core::tiles::pmtiles::PmtCache>,
) -> Result<(String, BoxedSource), String> {
    let file_path = PathBuf::from(row.file_path.clone());
    let src = FileConfigSrc::Path(file_path);

    match row.source_type {
        #[cfg(feature = "mbtiles")]
        FileSourceType::Mbtiles => {
            let custom = extract_file_custom(&config.mbtiles);
            let resolved = resolve_file_id::<crate::config::file::mbtiles::MbtConfig>(
                id_resolver,
                &row.source_id,
                &src,
            )
            .map_err(|e| e.to_string())?;
            let source = custom
                .new_sources(resolved.clone(), src.into_path())
                .await
                .map_err(|e| e.to_string())?;
            Ok((resolved, source))
        }
        #[cfg(feature = "pmtiles")]
        FileSourceType::Pmtiles => {
            let custom = update_pmtiles_cache(extract_file_custom(&config.pmtiles), pmtiles_cache);
            let resolved = resolve_file_id::<crate::config::file::pmtiles::PmtConfig>(
                id_resolver,
                &row.source_id,
                &src,
            )
            .map_err(|e| e.to_string())?;
            let source = create_source_with_urls(custom, resolved.clone(), src)
                .await
                .map_err(|e| e.to_string())?;
            Ok((resolved, source))
        }
        #[cfg(feature = "unstable-cog")]
        FileSourceType::Cog => {
            let custom = extract_file_custom(&config.cog);
            let resolved = resolve_file_id::<crate::config::file::cog::CogConfig>(
                id_resolver,
                &row.source_id,
                &src,
            )
            .map_err(|e| e.to_string())?;
            let source = custom
                .new_sources(resolved.clone(), src.into_path())
                .await
                .map_err(|e| e.to_string())?;
            Ok((resolved, source))
        }
        _ => Err(format!(
            "unsupported file source type for source '{}'",
            row.source_id
        )),
    }
}

async fn create_source_with_urls<T: TileSourceConfiguration>(
    custom: T,
    id: String,
    source: FileConfigSrc,
) -> Result<BoxedSource, String> {
    if let Some(url) = parse_url(T::parse_urls(), source.get_path())? {
        Ok(custom
            .new_sources_url(id, url)
            .await
            .map_err(|e| e.to_string())?)
    } else {
        Ok(custom
            .new_sources(id, source.into_path())
            .await
            .map_err(|e| e.to_string())?)
    }
}

fn resolve_file_id<T: TileSourceConfiguration>(
    id_resolver: &IdResolver,
    id: &str,
    source: &FileConfigSrc,
) -> Result<String, String> {
    if let Some(url) = parse_url(T::parse_urls(), source.get_path())? {
        Ok(id_resolver.resolve(id, url.to_string()))
    } else {
        let can = source.abs_path().map_err(|e| e.to_string())?;
        Ok(id_resolver.resolve(id, can.to_string_lossy().to_string()))
    }
}

fn parse_url(is_enabled: bool, path: &PathBuf) -> Result<Option<url::Url>, String> {
    if !is_enabled {
        return Ok(None);
    }
    let url_schemes = [
        "s3://", "s3a://", "gs://", "az://", "adl://", "azure://", "abfs://", "abfss://",
        "http://", "https://", "file://",
    ];
    path.to_str()
        .filter(|v| url_schemes.iter().any(|scheme| v.starts_with(scheme)))
        .map(|v| url::Url::parse(v).map_err(|e| e.to_string()))
        .transpose()
}

#[async_trait::async_trait]
impl Source for TilejsonOverrideSource {
    fn get_id(&self) -> &str {
        self.inner.get_id()
    }

    fn get_tilejson(&self) -> &TileJSON {
        &self.tilejson
    }

    fn get_tile_info(&self) -> martin_tile_utils::TileInfo {
        self.inner.get_tile_info()
    }

    fn clone_source(&self) -> BoxedSource {
        Box::new(self.clone())
    }

    fn get_version(&self) -> Option<String> {
        self.inner.get_version()
    }

    fn support_url_query(&self) -> bool {
        self.inner.support_url_query()
    }

    fn benefits_from_concurrent_scraping(&self) -> bool {
        self.inner.benefits_from_concurrent_scraping()
    }

    async fn get_tile(
        &self,
        xyz: martin_tile_utils::TileCoord,
        url_query: Option<&martin_core::tiles::UrlQuery>,
    ) -> martin_core::tiles::MartinCoreResult<martin_tile_utils::TileData> {
        self.inner.get_tile(xyz, url_query).await
    }
}

#[cfg(feature = "postgres")]
fn extract_single_postgres_config(config: &Config) -> DatabaseConfigResult<PostgresConfig> {
    let mut entries = config.postgres.iter();
    let Some(first) = entries.next() else {
        return Err(DatabaseConfigError::ValidationFailed(
            "database mode requires postgres configuration for data sources".to_string(),
        ));
    };
    if entries.next().is_some() {
        return Err(DatabaseConfigError::ValidationFailed(
            "database mode supports only one postgres connection".to_string(),
        ));
    }
    Ok(first.clone())
}

fn extract_file_custom<T: crate::config::file::ConfigurationLivecycleHooks + Clone>(
    config: &FileConfigEnum<T>,
) -> T {
    match config {
        FileConfigEnum::Config(cfg) => cfg.custom.clone(),
        _ => T::default(),
    }
}

#[cfg(feature = "postgres")]
pub async fn create_config_pool(
    connection_string: &str,
    ssl_cert: Option<&std::path::PathBuf>,
    ssl_key: Option<&std::path::PathBuf>,
    ssl_root_cert: Option<&std::path::PathBuf>,
    pool_size: usize,
) -> DatabaseConfigResult<Pool> {
    let (pg_cfg, ssl_mode) = parse_conn_str(connection_string)
        .map_err(|e| DatabaseConfigError::ConnectionFailed(e.to_string()))?;
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr =
        if pg_cfg.get_ssl_mode() == deadpool_postgres::tokio_postgres::config::SslMode::Disable {
            Manager::from_config(pg_cfg, NoTls, mgr_config)
        } else {
            let connector = make_connector(ssl_cert, ssl_key, ssl_root_cert, ssl_mode)
                .map_err(|e| DatabaseConfigError::ConnectionFailed(e.to_string()))?;
            Manager::from_config(pg_cfg, connector, mgr_config)
        };
    Pool::builder(mgr)
        .max_size(pool_size)
        .build()
        .map_err(|e| DatabaseConfigError::ConnectionFailed(e.to_string()))
}

#[cfg(feature = "postgres")]
pub fn apply_tilejson_patch(source: BoxedSource, patch: Option<Value>) -> BoxedSource {
    wrap_with_tilejson_override(source, &patch)
}

#[cfg(feature = "postgres")]
pub async fn create_config_schema(pool: &Pool) -> DatabaseConfigResult<()> {
    let conn = pool.get().await?;
    conn.batch_execute(CONFIG_SCHEMA_SQL).await?;
    Ok(())
}

#[cfg(feature = "postgres")]
pub async fn export_config_to_db(
    config: &mut Config,
    pool: &Pool,
    overwrite: bool,
) -> DatabaseConfigResult<ExportSummary> {
    let mut export_config = config.clone();
    export_config.config_source = crate::config::database::ConfigSource::File;
    export_config
        .finalize()
        .map_err(|e| DatabaseConfigError::ValidationFailed(e.to_string()))?;
    let _ = export_config
        .resolve()
        .await
        .map_err(|e| DatabaseConfigError::ValidationFailed(e.to_string()))?;

    let mut pg_configs: Vec<PostgresConfig> = export_config.postgres.iter().cloned().collect();
    if pg_configs.len() > 1 {
        return Err(DatabaseConfigError::ValidationFailed(
            "export supports only one postgres connection".to_string(),
        ));
    }

    let mut data_sources = Vec::new();
    if let Some(pg) = pg_configs.first_mut() {
        if let Some(tables) = &pg.tables {
            for (source_id, table) in tables {
                data_sources.push((
                    source_id.clone(),
                    "table",
                    table.schema.clone(),
                    table.table.clone(),
                    Some(table.geometry_column.clone()),
                    Some(table.srid),
                    table.id_column.clone(),
                    table.tilejson.clone(),
                ));
            }
        }
        if let Some(functions) = &pg.functions {
            for (source_id, func) in functions {
                data_sources.push((
                    source_id.clone(),
                    "function",
                    func.schema.clone(),
                    func.function.clone(),
                    None,
                    None,
                    None,
                    func.tilejson.clone(),
                ));
            }
        }
    }

    let mut file_sources = Vec::new();
    #[cfg(feature = "pmtiles")]
    file_sources.extend(collect_file_sources(
        &export_config.pmtiles,
        FileSourceType::Pmtiles,
    ));
    #[cfg(feature = "mbtiles")]
    file_sources.extend(collect_file_sources(
        &export_config.mbtiles,
        FileSourceType::Mbtiles,
    ));
    #[cfg(feature = "unstable-cog")]
    file_sources.extend(collect_file_sources(
        &export_config.cog,
        FileSourceType::Cog,
    ));

    let mut conn = pool.get().await?;
    let transaction = conn.transaction().await?;

    let mut data_inserted = 0;
    for (source_id, source_type, schema, name, geom, srid, id_column, props) in data_sources {
        let sql = if overwrite {
            r#"
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
"#
        } else {
            r#"
INSERT INTO martin_config.data_sources
  (source_id, source_type, schema_name, table_or_function_name, geometry_column, srid, id_column, properties, enabled)
VALUES
  ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)
ON CONFLICT DO NOTHING
"#
        };
        let affected = transaction
            .execute(
                sql,
                &[
                    &source_id,
                    &source_type,
                    &schema,
                    &name,
                    &geom,
                    &srid,
                    &id_column,
                    &props,
                ],
            )
            .await?;
        data_inserted += affected as usize;
    }

    let mut file_inserted = 0;
    for (source_id, source_type, file_path) in file_sources {
        let sql = if overwrite {
            r#"
INSERT INTO martin_config.file_sources
  (source_id, source_type, file_path, enabled)
VALUES
  ($1, $2, $3, TRUE)
ON CONFLICT (source_id) DO UPDATE SET
  source_type = EXCLUDED.source_type,
  file_path = EXCLUDED.file_path,
  enabled = TRUE
"#
        } else {
            r#"
INSERT INTO martin_config.file_sources
  (source_id, source_type, file_path, enabled)
VALUES
  ($1, $2, $3, TRUE)
ON CONFLICT DO NOTHING
"#
        };
        let affected = transaction
            .execute(sql, &[&source_id, &source_type, &file_path])
            .await?;
        file_inserted += affected as usize;
    }

    transaction
        .execute(
            "UPDATE martin_config.metadata SET version = version + 1, updated_at = NOW() WHERE id = 1",
            &[],
        )
        .await?;
    transaction.commit().await?;

    Ok(ExportSummary {
        data_sources: data_inserted,
        file_sources: file_inserted,
    })
}

#[cfg(feature = "postgres")]
pub async fn validate_db_config(config: &Config, pool: &Pool) -> DatabaseConfigResult<usize> {
    let id_resolver = IdResolver::new(RESERVED_KEYWORDS);
    let loaded = load_config_from_database(
        config,
        pool,
        &id_resolver,
        #[cfg(feature = "pmtiles")]
        None,
    )
    .await?;
    Ok(loaded.sources.source_names().len())
}

fn collect_file_sources<T: crate::config::file::ConfigurationLivecycleHooks + Clone>(
    cfg: &FileConfigEnum<T>,
    source_type: FileSourceType,
) -> Vec<(String, String, String)> {
    let type_str = match source_type {
        FileSourceType::Mbtiles => "mbtiles",
        FileSourceType::Pmtiles => "pmtiles",
        FileSourceType::Cog => "cog",
    };

    match cfg {
        FileConfigEnum::None => Vec::new(),
        FileConfigEnum::Path(path) => {
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            vec![(id, type_str.to_string(), path.to_string_lossy().to_string())]
        }
        FileConfigEnum::Paths(paths) => paths
            .iter()
            .map(|path| {
                let id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                (id, type_str.to_string(), path.to_string_lossy().to_string())
            })
            .collect(),
        FileConfigEnum::Config(cfg) => {
            let mut results = Vec::new();
            if let Some(sources) = &cfg.sources {
                results.extend(sources.iter().map(|(id, src)| {
                    (
                        id.clone(),
                        type_str.to_string(),
                        src.get_path().to_string_lossy().to_string(),
                    )
                }));
            }
            results.extend(cfg.paths.iter().map(|path| {
                let id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                (id, type_str.to_string(), path.to_string_lossy().to_string())
            }));
            results
        }
    }
}
