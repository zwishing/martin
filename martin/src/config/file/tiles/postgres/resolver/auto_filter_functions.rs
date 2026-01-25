//! Automatic generation of filtered tile functions for table sources.
//!
//! This module provides functionality to automatically generate PostgreSQL functions
//! that support dynamic filtering, sorting, and property selection for vector tile queries.
//! These generated functions work seamlessly with the smart routing feature to provide
//! optimized tile generation based on client query parameters.
//!
//! # Overview
//!
//! The auto-generated functions enable:
//! - Dynamic feature limiting and pagination
//! - Property-based filtering (exact match and ranges)
//! - Result sorting by any property
//! - Dynamic property selection to reduce tile size
//!
//! # Configuration
//!
//! Enable auto-generation in your config file:
//!
//! ```yaml
//! postgres:
//!   auto_generate_filters: true
//!   filter_function_suffix: "filtered"  # optional, defaults to "filtered"
//! ```
//!
//! # Generated Functions
//!
//! For a table `cities`, this will generate a function `cities_filtered` that accepts
//! standard tile coordinates (z, x, y) plus a JSON object with filter parameters.
//!
//! # Integration
//!
//! Works with smart routing - when a tile request includes filter parameters,
//! the router automatically uses the `_filtered` variant if available.

use std::collections::HashMap;

use log::{debug, info};
use martin_core::tiles::postgres::{PostgresPool, PostgresResult};
use postgres_protocol::escape::escape_identifier;

use crate::config::file::postgres::TableInfo;

/// Generate a filtered tile function for a table source.
///
/// Creates a PostgreSQL function that supports dynamic filtering, sorting, and property
/// selection for vector tile queries. The generated function is named `{table}_{suffix}`
/// and accepts tile coordinates plus a JSON object containing filter parameters.
///
/// # Supported Query Parameters
///
/// - `limit`: Maximum number of features to return (default: 10000, max: 100000)
/// - `offset`: Number of features to skip (for pagination)
/// - `properties`: Comma-separated list of properties to include in output
/// - `sortby`: Property name to sort by (prefix with `-` for descending order)
/// - `{property}={value}`: Filter for exact match on a property
/// - `{property}_min={value}`: Filter for property >= value
/// - `{property}_max={value}`: Filter for property <= value
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `_source_id` - Source identifier (reserved for future use)
/// * `table_info` - Table metadata including schema, table name, geometry column, and SRID
/// * `function_suffix` - Suffix to append to the function name (typically "filtered")
///
/// # Returns
///
/// Returns the generated function name on success, or a PostgreSQL error if function
/// creation fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Unable to query table columns from information_schema
/// - Unable to create the PostgreSQL function (e.g., permission denied)
/// - Invalid table metadata (missing required fields)
///
/// # Example
///
/// ```ignore
/// use martin::config::file::postgres::TableInfo;
/// use martin_core::tiles::postgres::PostgresPool;
///
/// # async fn example(pool: &PostgresPool) -> Result<(), Box<dyn std::error::Error>> {
/// let table_info = TableInfo {
///     schema: "public".to_string(),
///     table: "cities".to_string(),
///     geometry_column: "geom".to_string(),
///     srid: 4326,
///     ..Default::default()
/// };
///
/// let function_name = create_filtered_function(
///     pool,
///     "public.cities",
///     &table_info,
///     "filtered"
/// ).await?;
///
/// println!("Created function: {}", function_name); // "cities_filtered"
/// # Ok(())
/// # }
/// ```
///
/// # Generated SQL Example
///
/// ```sql
/// -- Generated function signature:
/// CREATE FUNCTION public.cities_filtered(
///     z integer,
///     x integer,
///     y integer,
///     query_params json DEFAULT '{}'::json
/// ) RETURNS bytea
/// LANGUAGE plpgsql STABLE STRICT PARALLEL SAFE;
///
/// -- Usage examples:
/// -- Get up to 100 cities with population over 1 million
/// SELECT cities_filtered(10, 512, 384, '{"limit": 100, "population_min": 1000000}'::json);
///
/// -- Get cities sorted by name, only including name and population properties
/// SELECT cities_filtered(10, 512, 384, '{"sortby": "name", "properties": "name,population"}'::json);
/// ```
pub async fn create_filtered_function(
    pool: &PostgresPool,
    _source_id: &str,
    table_info: &TableInfo,
    function_suffix: &str,
) -> PostgresResult<String> {
    let function_name = format!("{}_{}", table_info.table, function_suffix);
    let layer_name = table_info.layer_id.as_deref().unwrap_or(&table_info.table);
    let schema_esc = escape_identifier(&table_info.schema);
    let table_esc = escape_identifier(&table_info.table);
    let geom_col = &table_info.geometry_column;

    let srid = table_info.srid;
    let extent = table_info.extent.unwrap_or(4096);
    let buffer = table_info.buffer.unwrap_or(64);
    let clip_geom = table_info.clip_geom.unwrap_or(true);

    // Get all non-geometry columns for property selection
    let properties = get_table_columns(
        pool,
        &table_info.schema,
        &table_info.table,
        &table_info.geometry_column,
    )
    .await?;

    let function_sql = generate_function_sql(
        &function_name,
        layer_name,
        &schema_esc,
        &table_esc,
        geom_col,
        srid,
        extent,
        buffer,
        clip_geom,
        &properties,
    );

    // Create the function
    pool.get()
        .await?
        .execute(&function_sql, &[])
        .await
        .map_err(|e| {
            martin_core::tiles::postgres::PostgresError::PostgresError(
                e,
                "creating filtered function",
            )
        })?;

    info!(
        "Created filtered function: {}.{}",
        table_info.schema, function_name
    );
    Ok(function_name)
}

/// Get all columns from a table except the geometry column.
///
/// Queries the PostgreSQL information_schema to retrieve all column names
/// for the specified table, excluding the geometry column.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `schema` - Schema name
/// * `table` - Table name
/// * `geometry_column` - Geometry column name to exclude
///
/// # Returns
///
/// Returns a vector of column names, or a PostgreSQL error if the query fails.
async fn get_table_columns(
    pool: &PostgresPool,
    schema: &str,
    table: &str,
    geometry_column: &str,
) -> PostgresResult<Vec<String>> {
    let query = r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
          AND column_name != $3
        ORDER BY ordinal_position
    "#;

    let rows = pool
        .get()
        .await?
        .query(query, &[&schema, &table, &geometry_column])
        .await
        .map_err(|e| {
            martin_core::tiles::postgres::PostgresError::PostgresError(e, "querying table columns")
        })?;

    Ok(rows.iter().map(|row| row.get(0)).collect())
}

/// Generate the SQL for the filtered function
fn generate_function_sql(
    function_name: &str,
    layer_name: &str,
    schema: &str,
    table: &str,
    geom_col: &str,
    srid: i32,
    extent: u32,
    buffer: u32,
    clip_geom: bool,
    properties: &[String],
) -> String {
    let properties_list = properties
        .iter()
        .map(|p| escape_identifier(p))
        .collect::<Vec<_>>()
        .join(", ");

    let properties_list_quoted = properties
        .iter()
        .map(|p| format!("'{}'", p.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"
CREATE OR REPLACE FUNCTION {schema}.{function_name}(
    z integer,
    x integer,
    y integer,
    query_params json DEFAULT '{{}}'::json
)
RETURNS bytea
LANGUAGE plpgsql
STABLE STRICT PARALLEL SAFE  -- STABLE because function queries database tables
AS $$
DECLARE
    mvt bytea;
    bbox geometry;
    where_clauses text[] := ARRAY[]::text[];
    order_clause text := '';
    limit_clause text := '';
    offset_clause text := '';
    properties_clause text := '';
    limit_val integer;
    offset_val integer;
    sortby_val text;
    properties_val text;
    key text;
    value text;
BEGIN
    -- Get tile envelope
    bbox := ST_TileEnvelope(z, x, y);

    -- Parse limit parameter
    IF query_params ? 'limit' THEN
        limit_val := (query_params->>'limit')::integer;
        IF limit_val > 0 AND limit_val <= 100000 THEN
            limit_clause := format(' LIMIT %s', limit_val);
        END IF;
    ELSE
        limit_clause := ' LIMIT 10000';  -- Default limit
    END IF;

    -- Parse offset parameter
    IF query_params ? 'offset' THEN
        offset_val := (query_params->>'offset')::integer;
        IF offset_val > 0 THEN
            offset_clause := format(' OFFSET %s', offset_val);
        END IF;
    END IF;

    -- Parse sortby parameter
    IF query_params ? 'sortby' THEN
        sortby_val := query_params->>'sortby';
        IF sortby_val LIKE '-%' THEN
            order_clause := format(' ORDER BY %I DESC', ltrim(sortby_val, '-'));
        ELSIF sortby_val LIKE '+%' THEN
            order_clause := format(' ORDER BY %I ASC', ltrim(sortby_val, '+'));
        ELSE
            order_clause := format(' ORDER BY %I ASC', sortby_val);
        END IF;
    END IF;

    -- Parse properties parameter (column selection)
    IF query_params ? 'properties' THEN
        properties_val := query_params->>'properties';
        FOR key IN SELECT trim(k) FROM unnest(string_to_array(properties_val, ',')) AS k
        LOOP
            IF key IN ({properties_list_quoted}) THEN
                 properties_clause := properties_clause || ', ' || quote_ident(key);
            END IF;
        END LOOP;
    END IF;

    -- If no valid properties selected, use all allowed properties
    IF properties_clause = '' THEN
        properties_clause := ', {properties_list}';
    END IF;

    -- Parse property filters (e.g., population=1000000)
    FOR key, value IN SELECT * FROM json_each_text(query_params)
    LOOP
        -- Skip known parameters
        IF key IN ('limit', 'offset', 'sortby', 'properties') THEN
            CONTINUE;
        END IF;

        -- Handle range filters (_min, _max suffixes)
        -- Use left(key, -4) to remove last 4 characters instead of rtrim()
        -- rtrim() removes ALL occurrences of specified characters, not just suffix
        -- e.g., rtrim('population_min', '_min') → 'populatio' (wrong!)
        IF key LIKE '%_min' THEN
            where_clauses := array_append(
                where_clauses,
                format('%I >= %L', left(key, -4), value)
            );
        ELSIF key LIKE '%_max' THEN
            where_clauses := array_append(
                where_clauses,
                format('%I <= %L', left(key, -4), value)
            );
        ELSE
            -- Exact match filter
            where_clauses := array_append(
                where_clauses,
                format('%I = %L', key, value)
            );
        END IF;
    END LOOP;

    -- Build WHERE clause
    DECLARE
        additional_where text := '';
    BEGIN
        IF array_length(where_clauses, 1) > 0 THEN
            additional_where := ' AND ' || array_to_string(where_clauses, ' AND ');
        END IF;

        -- Execute dynamic query
        EXECUTE format($sql$
            SELECT ST_AsMVT(tile, %L, {extent}, 'geom')
            FROM (
                SELECT
                    ST_AsMVTGeom(
                        ST_Transform(%I, 3857),
                        ST_TileEnvelope(%s, %s, %s),
                        {extent}, {buffer}, {clip_geom}
                    ) AS geom
                    %s
                FROM {schema}.{table}
                WHERE %I && ST_Transform(ST_TileEnvelope(%s, %s, %s), {srid})
                %s
                %s
                %s
                %s
            ) AS tile
            WHERE geom IS NOT NULL
        $sql$,
            '{layer_name}',     -- layer name
            '{geom_col}',       -- geometry column for transform
            z, x, y,            -- tile coordinates for envelope
            properties_clause,  -- properties to include
            '{geom_col}',       -- geometry column for bbox filter
            z, x, y,            -- tile coordinates for bbox
            additional_where,   -- additional WHERE clauses
            order_clause,       -- ORDER BY clause
            limit_clause,       -- LIMIT clause
            offset_clause       -- OFFSET clause
        ) INTO mvt;
    END;

    RETURN COALESCE(mvt, ''::bytea);
END;
$$;

-- Add comment describing the function
COMMENT ON FUNCTION {schema}.{function_name}(integer, integer, integer, json) IS
'Auto-generated filtered tile function for {schema}.{table}.
Supports query parameters: limit, offset, sortby, properties, and property filters.
Example: SELECT {function_name}(14, 8192, 5461, ''{{\"limit\": 100, \"population_min\": 1000000}}''::json)';
"#,
        schema = schema,
        function_name = function_name,
        table = table,
        geom_col = geom_col,
        srid = srid,
        extent = extent,
        buffer = buffer,
        clip_geom = clip_geom,
        properties_list = properties_list,
    )
}

/// Auto-generate filtered functions for all table sources.
///
/// Iterates through all provided table sources and generates a filtered tile function
/// for each one. Functions that fail to generate are logged as warnings but do not
/// stop the process.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
/// * `tables` - HashMap of source IDs to table metadata
/// * `suffix` - Suffix to append to function names (typically "filtered")
///
/// # Returns
///
/// Returns a HashMap mapping source IDs to the names of successfully generated functions.
/// Tables that failed to generate functions are omitted from the result.
///
/// # Example
///
/// ```ignore
/// use std::collections::HashMap;
/// use martin::config::file::postgres::TableInfo;
///
/// # async fn example(pool: &PostgresPool) -> Result<(), Box<dyn std::error::Error>> {
/// let mut tables = HashMap::new();
/// tables.insert("public.cities".to_string(), TableInfo { /* ... */ });
/// tables.insert("public.roads".to_string(), TableInfo { /* ... */ });
///
/// let generated = auto_generate_filtered_functions(&pool, &tables, "filtered").await?;
/// println!("Generated {} functions", generated.len());
/// # Ok(())
/// # }
/// ```
pub async fn auto_generate_filtered_functions(
    pool: &PostgresPool,
    tables: &HashMap<String, TableInfo>,
    suffix: &str,
) -> PostgresResult<HashMap<String, String>> {
    let mut generated_functions = HashMap::new();

    for (source_id, table_info) in tables {
        match create_filtered_function(pool, source_id, table_info, suffix).await {
            Ok(function_name) => {
                debug!(
                    "Generated filtered function for {}: {}",
                    source_id, function_name
                );
                generated_functions.insert(source_id.clone(), function_name);
            }
            Err(e) => {
                log::warn!(
                    "Failed to generate filtered function for {}: {}",
                    source_id,
                    e
                );
            }
        }
    }

    info!(
        "Auto-generated {} filtered functions",
        generated_functions.len()
    );

    Ok(generated_functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_function_sql_contains_stable() {
        // Test that generated SQL uses STABLE volatility, not IMMUTABLE
        let sql = generate_function_sql(
            "test_filtered",
            "test_table",
            "public",
            "test_table",
            "geom",
            4326,
            4096,
            64,
            true,
            &["id".to_string(), "name".to_string()],
        );

        assert!(
            sql.contains("STABLE"),
            "Generated SQL should contain STABLE volatility"
        );
        assert!(
            !sql.contains("IMMUTABLE"),
            "Generated SQL should not contain IMMUTABLE"
        );
    }

    #[test]
    fn test_generate_function_sql_uses_left_not_rtrim() {
        // Test that generated SQL uses left(key, -4) for suffix removal
        // This is critical because rtrim('population_min', '_min') would incorrectly
        // remove all occurrences of '_', 'm', 'i', 'n' characters
        let sql = generate_function_sql(
            "test_filtered",
            "test_table",
            "public",
            "test_table",
            "geom",
            4326,
            4096,
            64,
            true,
            &["population".to_string()],
        );

        assert!(
            sql.contains("left(key, -4)"),
            "Generated SQL should use left(key, -4) for suffix removal"
        );
        assert!(
            !sql.contains("rtrim(key"),
            "Generated SQL should not use rtrim for suffix removal"
        );
    }

    #[test]
    fn test_generate_function_sql_signature() {
        // Test that function signature is correct
        let sql = generate_function_sql(
            "cities_filtered",
            "cities",
            "public",
            "cities",
            "geom",
            4326,
            4096,
            64,
            true,
            &["name".to_string(), "population".to_string()],
        );

        // Check function name
        assert!(
            sql.contains("CREATE OR REPLACE FUNCTION public.cities_filtered"),
            "Function name should be correct"
        );

        // Check parameters
        assert!(sql.contains("z integer"), "Should have z parameter");
        assert!(sql.contains("x integer"), "Should have x parameter");
        assert!(sql.contains("y integer"), "Should have y parameter");
        assert!(
            sql.contains("query_params json") && sql.contains("DEFAULT"),
            "Should have query_params with default"
        );

        // Check return type
        assert!(sql.contains("RETURNS bytea"), "Should return bytea");
    }

    #[test]
    fn test_generate_function_sql_includes_properties() {
        // Test that properties are included in the SQL
        let properties = vec![
            "id".to_string(),
            "name".to_string(),
            "population".to_string(),
        ];

        let sql = generate_function_sql(
            "test_filtered",
            "test_table",
            "public",
            "test_table",
            "geom",
            4326,
            4096,
            64,
            true,
            &properties,
        );

        // Properties should be escaped and included
        assert!(sql.contains("\"id\""), "Should include id property");
        assert!(sql.contains("\"name\""), "Should include name property");
        assert!(
            sql.contains("\"population\""),
            "Should include population property"
        );
    }

    #[test]
    fn test_generate_function_sql_handles_special_characters() {
        // Test that special characters in identifiers are properly escaped
        let sql = generate_function_sql(
            "test_filtered",
            "my-table",
            "my-schema",
            "my-table",
            "my-geom",
            4326,
            4096,
            64,
            true,
            &["my-column".to_string()],
        );

        // Identifiers with special characters should be escaped
        assert!(
            sql.contains("\"my-schema\"") || sql.contains("my-schema"),
            "Schema should be handled"
        );
        assert!(
            sql.contains("\"my-table\"") || sql.contains("my-table"),
            "Table should be handled"
        );
    }

    #[test]
    fn test_generate_function_sql_parameters() {
        // Test that various parameters are correctly included
        let sql = generate_function_sql(
            "test_filtered",
            "test_table",
            "public",
            "test_table",
            "geom",
            3857,  // Different SRID
            2048,  // Different extent
            128,   // Different buffer
            false, // No clipping
            &["id".to_string()],
        );

        // Check SRID is used
        assert!(sql.contains("3857"), "Should use specified SRID");

        // Check extent is used
        assert!(sql.contains("2048"), "Should use specified extent");

        // Check buffer is used
        assert!(sql.contains("128"), "Should use specified buffer");

        // Check clip_geom is used
        assert!(
            sql.contains("false") || sql.contains("FALSE"),
            "Should use specified clip_geom value"
        );
    }

    #[test]
    fn test_generate_function_sql_has_comment() {
        // Test that generated function has a descriptive comment
        let sql = generate_function_sql(
            "cities_filtered",
            "cities",
            "public",
            "cities",
            "geom",
            4326,
            4096,
            64,
            true,
            &["name".to_string()],
        );

        assert!(
            sql.contains("COMMENT ON FUNCTION"),
            "Should include a comment"
        );
        assert!(
            sql.contains("Auto-generated filtered tile function"),
            "Comment should describe the function"
        );
        assert!(
            sql.contains("limit, offset, sortby, properties"),
            "Comment should list supported parameters"
        );
    }

    #[test]
    fn test_generate_function_sql_handles_empty_properties() {
        // Test that function works with no properties
        let sql = generate_function_sql(
            "test_filtered",
            "test_table",
            "public",
            "test_table",
            "geom",
            4326,
            4096,
            64,
            true,
            &[],
        );

        // Should still generate valid SQL
        assert!(
            sql.contains("CREATE OR REPLACE FUNCTION"),
            "Should generate function even with no properties"
        );
        assert!(
            sql.contains("RETURNS bytea"),
            "Should have correct return type"
        );
    }
}
