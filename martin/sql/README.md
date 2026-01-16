

 # Martin Configuration Schema

This directory contains SQL migration scripts for Martin's database-driven configuration feature.

## Overview

The configuration schema enables dynamic tile source management without restarting Martin. Configuration is stored in PostgreSQL tables and polled periodically.

## Schema Structure

### Tables

| Table | Purpose |
|-------|---------|
| `martin_config.metadata` | Tracks configuration version for change detection |
| `martin_config.data_sources` | PostgreSQL table/function tile sources |
| `martin_config.file_sources` | MBTiles/PMTiles/COG file sources |

### Views

| View | Purpose |
|------|---------|
| `martin_config.active_sources` | Union of all enabled sources |
| `martin_config.config_summary` | Statistics about configuration state |

## Installation

### Method 1: Using Martin CLI

```bash
# Create schema automatically
martin --create-config-schema

# Or with specific connection string
martin --create-config-schema --connection "postgresql://user:pass@localhost/db"
```

### Method 2: Manual psql

```bash
# Install schema
psql -d your_database -f config_schema.sql

# Verify installation
psql -d your_database -c "SELECT * FROM martin_config.config_summary;"
```

## Usage Examples

### Adding a PostgreSQL Table Source

```sql
-- Add a PostGIS table as a tile source
INSERT INTO martin_config.data_sources (
    source_id,
    source_type,
    schema_name,
    table_or_function_name,
    geometry_column,
    srid,
    id_column,
    properties
) VALUES (
    'cities',                    -- Source ID (used in URLs: /cities/{z}/{x}/{y})
    'table',                     -- Source type
    'public',                    -- Schema name
    'cities',                    -- Table name
    'geom',                      -- Geometry column
    4326,                        -- SRID
    'id',                        -- Primary key column
    '{
        "name": "World Cities",
        "description": "Major cities worldwide",
        "attribution": "© OpenStreetMap contributors"
    }'::jsonb
);

-- After changing sources, update the metadata version
-- Martin will pick up the change within the configured polling interval (default: 60s)
```

### Adding a PostgreSQL Function Source

```sql
-- Add an MVT-returning function
INSERT INTO martin_config.data_sources (
    source_id,
    source_type,
    schema_name,
    table_or_function_name,
    properties
) VALUES (
    'heatmap',
    'function',
    'public',
    'get_heatmap_tiles',         -- Function must accept (z int, x int, y int) and return bytea
    '{"name": "Density Heatmap"}'::jsonb
);
```

### Adding a File Source

```sql
-- Add an MBTiles file
INSERT INTO martin_config.file_sources (
    source_id,
    source_type,
    file_path,
    properties
) VALUES (
    'countries',
    'mbtiles',
    '/data/tiles/countries.mbtiles',
    '{"name": "Country Boundaries"}'::jsonb
);

-- Add a PMTiles file
INSERT INTO martin_config.file_sources (
    source_id,
    source_type,
    file_path
) VALUES (
    'satellite',
    'pmtiles',
    '/data/tiles/satellite.pmtiles'
);
```

### Disabling a Source Temporarily

```sql
-- Disable without deleting
UPDATE martin_config.data_sources
SET enabled = FALSE
WHERE source_id = 'cities';

-- Re-enable
UPDATE martin_config.data_sources
SET enabled = TRUE
WHERE source_id = 'cities';
```

### Manual Version Increment

If you've disabled triggers, increment version manually:

```sql
-- After making configuration changes
SELECT martin_config.increment_version();
```

### Viewing Current Configuration

```sql
-- Summary statistics
SELECT * FROM martin_config.config_summary;

-- All active sources
SELECT * FROM martin_config.active_sources WHERE enabled = TRUE;

-- Data sources only
SELECT source_id, source_type, schema_name, table_or_function_name
FROM martin_config.data_sources
WHERE enabled = TRUE
ORDER BY source_id;

-- File sources only
SELECT source_id, source_type, file_path
FROM martin_config.file_sources
WHERE enabled = TRUE
ORDER BY source_id;
```

## Version Management

The `metadata` table tracks configuration versions:

```sql
-- Check current version
SELECT version, updated_at FROM martin_config.metadata;

-- Martin polls this table periodically
-- When version changes, it reloads all sources
```

### Manual Version Updates (Required)

External tooling MUST increment the version after changes:

```sql
-- After making configuration changes
SELECT martin_config.increment_version();
```

## Martin Configuration

Enable database mode in `martin.yaml`:

```yaml
# Base configuration (ports, cache, etc.)
listening_address: 0.0.0.0:3000
cache_size_mb: 512

# Enable database-driven configuration
config_source: database
config_refresh_interval_seconds: 60  # Minimum 10 seconds
admin_reload_enabled: false          # Explicitly enable /admin/config/reload

# PostgreSQL connection (for both tile data and configuration)
postgres:
  connection_string: "postgresql://martin:password@localhost/geodata"
```

## Security Considerations

### Recommended Permissions

```sql
-- Create read-only user for Martin
CREATE USER martin_app WITH PASSWORD 'secure_password';
GRANT USAGE ON SCHEMA martin_config TO martin_app;
GRANT SELECT ON ALL TABLES IN SCHEMA martin_config TO martin_app;

-- Create admin user for configuration management
CREATE USER config_admin WITH PASSWORD 'admin_password';
GRANT USAGE ON SCHEMA martin_config TO config_admin;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA martin_config TO config_admin;
GRANT EXECUTE ON FUNCTION martin_config.increment_version() TO config_admin;
```

### Separate Configuration Database (Optional)

For enhanced security, store configuration in a separate database:

```yaml
# martin.yaml
postgres:
  connection_string: "postgresql://martin:pass@localhost/tile_data"  # Read-only tile data

config_database: "postgresql://config_admin:pass@localhost/config_db"  # Configuration (needs write access)
```

## Migration from File-Based Configuration

### Export Existing Configuration

```bash
# Export current YAML sources to database
martin --export-config-to-db --config martin.yaml

# Or with overwrite (replaces existing entries)
martin --export-config-to-db --config martin.yaml --overwrite
```

### Validation

```bash
# Validate database configuration before switching
martin --validate-db-config
```

### Cutover

1. **Backup** your current YAML configuration
2. **Export** to database (see above)
3. **Update** `martin.yaml`: Set `config_source: database`
4. **Restart** Martin
5. **Verify**: Check `/catalog` endpoint for all sources

### Rollback

```yaml
# martin.yaml
config_source: file  # Switch back to file mode
```

Then restart Martin.

## Troubleshooting

### Check Schema Installation

```sql
-- Verify tables exist
\dt martin_config.*

-- Verify views exist
\dv martin_config.*

-- Check permissions
\dp martin_config.*
```

### No Sources Loading

```sql
-- Check for enabled sources
SELECT COUNT(*) FROM martin_config.data_sources WHERE enabled = TRUE;
SELECT COUNT(*) FROM martin_config.file_sources WHERE enabled = TRUE;

-- If count is 0, Martin will error on startup
```

### Configuration Not Updating

```sql
-- Check version is incrementing
SELECT version, updated_at FROM martin_config.metadata;

-- Ensure your tooling increments metadata.version after changes
```

### Force Reload

```bash
# Trigger immediate reload without waiting for poll interval
# Requires admin_reload_enabled: true in config (otherwise returns 404)
curl -X POST http://localhost:3000/admin/config/reload
```

## Performance Notes

- Configuration queries use indexes (see `idx_*` in schema)
- Polling interval is configurable (default: 60 seconds, minimum: 10 seconds)
- Only queries when version changes (version-based check is fast)
- Source instantiation happens in background (tile serving unaffected)

## Schema Updates

Future schema changes will be versioned:

- `config_schema_v2.sql` - Migration from v1 to v2
- `config_schema_v3.sql` - Migration from v2 to v3
- etc.

Check Martin release notes for migration procedures.

## Support

- **Documentation**: https://maplibre.org/martin/
- **Issues**: https://github.com/maplibre/martin/issues
- **Discussions**: https://github.com/maplibre/martin/discussions
