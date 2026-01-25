//! SQL generator for dynamic filtered tile functions

use log::warn;

/// Generate SQL for a filtered tile function
pub fn generate_filtered_function_sql(
    schema: &str,
    table: &str,
    function_name: &str,
    geom_col: &str,
    srid: i32,
    properties_list: &str,
    properties_list_quoted: &str,
) -> String {
    format!(
        r#"
CREATE OR REPLACE FUNCTION {schema}.{function_name}(
    z int2,
    x int8,
    y int8,
    query_params json DEFAULT '{{}}'::json
)
RETURNS bytea
LANGUAGE plpgsql
STABLE STRICT PARALLEL SAFE
AS $func$
DECLARE
    mvt bytea;
    limit_val integer;
    offset_val integer;
    where_clauses text[] := ARRAY[]::text[];
    order_clause text := '';
    properties_clause text := '';
    key text;
    value text;
BEGIN
    -- Parse limit parameter
    limit_val := COALESCE((query_params->>'limit')::integer, 10000);
    IF limit_val > 100000 THEN limit_val := 100000; END IF;

    -- Parse offset parameter
    offset_val := COALESCE((query_params->>'offset')::integer, 0);

    -- Parse sortby parameter
    IF (query_params::jsonb) ? 'sortby' THEN
        DECLARE
            sortby_val text := query_params->>'sortby';
        BEGIN
            IF sortby_val LIKE '-%' THEN
                order_clause := format(' ORDER BY %I DESC', ltrim(sortby_val, '-'));
            ELSE
                order_clause := format(' ORDER BY %I ASC', ltrim(sortby_val, '+'));
            END IF;
        END;
    END IF;

    -- Parse properties parameter (column selection)
    IF (query_params::jsonb) ? 'properties' THEN
        FOR key IN SELECT trim(k) FROM unnest(string_to_array(query_params->>'properties', ',')) AS k
        LOOP
            IF key IN ({properties_list_quoted}) THEN
                 properties_clause := properties_clause || ', ' || quote_ident(key);
            END IF;
        END LOOP;
    END IF;

    -- If no valid properties selected, use all allowed properties
    IF properties_clause = '' THEN
        properties_clause := ', {properties}';
    END IF;

    -- Parse property filters
    FOR key, value IN SELECT * FROM jsonb_each_text(query_params::jsonb)
    LOOP
        IF key IN ('limit', 'offset', 'sortby', 'properties') THEN
            CONTINUE;
        END IF;

        -- Handle range filters with left() to remove suffix (NOT rtrim!)
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
            where_clauses := array_append(
                where_clauses,
                format('%I = %L', key, value)
            );
        END IF;
    END LOOP;

    -- Build WHERE clause and execute
    DECLARE
        additional_where text := '';
    BEGIN
        IF array_length(where_clauses, 1) > 0 THEN
            additional_where := ' AND ' || array_to_string(where_clauses, ' AND ');
        END IF;

        EXECUTE format($sql$
            SELECT ST_AsMVT(tile, %L, 4096, 'geom')
            FROM (
                SELECT
                    ST_AsMVTGeom(
                        ST_Transform(%I, 3857),
                        ST_TileEnvelope(%s, %s, %s),
                        4096, 64, true
                    ) AS geom
                    %s
                FROM {schema}.{table}
                WHERE %I && ST_Transform(ST_TileEnvelope(%s, %s, %s), {srid})
                %s
                %s
                LIMIT %s OFFSET %s
            ) AS tile
            WHERE geom IS NOT NULL
        $sql$,
            '{table}',
            '{geom_col}',
            z, x, y,
            properties_clause,
            '{geom_col}',
            z, x, y,
            additional_where,
            order_clause,
            limit_val,
            offset_val
        ) INTO mvt;
    END;

    RETURN COALESCE(mvt, ''::bytea);
END;
$func$;"#,
        schema = schema,
        function_name = function_name,
        table = table,
        geom_col = geom_col,
        srid = srid,
        properties = properties_list,
        properties_list_quoted = properties_list_quoted
    )
}

/// Fetch all columns for a table excluding the geometry column
/// Returns tuple (properties_list, properties_list_quoted)
pub async fn fetch_table_columns(
    client: &deadpool_postgres::Client,
    schema: &str,
    table: &str,
    geom_col: &str,
) -> Result<(String, String), String> {
    let query = format!(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = $1 AND table_name = $2 AND column_name != $3 \
         ORDER BY ordinal_position"
    );

    match client.query(&query, &[&schema, &table, &geom_col]).await {
        Ok(rows) => {
            let list = rows
                .iter()
                .map(|r| {
                    let col: String = r.get(0);
                    format!("\"{}\"", col.replace('"', "\"\""))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let quoted = rows
                .iter()
                .map(|r| {
                    let col: String = r.get(0);
                    format!("'{}'", col.replace('\'', "''"))
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok((list, quoted))
        }
        Err(e) => {
            let msg = format!("Failed to get columns for table '{schema}.{table}': {e:?}");
            warn!("{}", msg);
            Err(msg)
        }
    }
}
