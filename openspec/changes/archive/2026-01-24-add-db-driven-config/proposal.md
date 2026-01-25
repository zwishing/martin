# Change: Add Database-Driven Dynamic Configuration

## Why

Martin currently loads tile source configuration only at startup from static YAML files or environment variables. When new data tables are added to PostgreSQL or when existing configurations need to be updated, Martin must be restarted to pick up these changes. This creates operational friction in dynamic environments where:

1. **Data sources change frequently** - New tables/schemas are added by data pipelines or other services
2. **System integration is difficult** - External services cannot dynamically register or update data sources without file-system access
3. **Multi-tenant scenarios** - Different services managing their own tile sources need a centralized, dynamic registry
4. **Configuration drift** - File-based configuration can become out of sync with actual database state

This proposal adds **optional database-driven configuration** where tile source metadata (PostgreSQL tables/functions, file paths for MBTiles/PMTiles/COG) is stored in PostgreSQL tables. Martin will periodically refresh this configuration, enabling dynamic updates without service restarts.

## What Changes

- **ADDED**: New PostgreSQL schema for storing tile source configuration metadata
  - `martin_config.data_sources` table for PostgreSQL table/function source definitions
  - `martin_config.file_sources` table for MBTiles/PMTiles/COG file source definitions
  - `martin_config.metadata` table for tracking configuration version and refresh timestamps
- **ADDED**: Configuration option `config_source: database` to enable database-driven mode (default: `file`)
- **ADDED**: Configuration polling mechanism with configurable refresh interval (default: 60 seconds)
- **ADDED**: Configuration option `admin_reload_enabled` to explicitly enable `/admin/config/reload`
- **ADDED**: Dynamic source reload capability without dropping existing tile cache
- **ADDED**: Optional HTTP endpoint `/admin/config/reload` (explicitly enabled in config) to trigger manual refresh
- **ADDED**: SQL migration scripts to create configuration schema
- **MODIFIED**: `Config` struct to support dual mode (file-based vs database-driven)
- **MODIFIED**: Server initialization to set up configuration polling when in database mode
- **MODIFIED**: Source discovery logic to query configuration tables instead of config file

**Key Design Principles:**
- **Backward compatible** - Existing file-based configuration remains the default
- **Hybrid support** - Base system config (ports, SSL, cache sizes) stays in YAML; only data source config moves to database
- **Non-breaking** - No changes to existing tile serving endpoints or TileJSON API
- **Graceful degradation** - Configuration reload errors are logged but don't crash the server

## Impact

### Affected Specs
- **configuration-management** (NEW) - Database-driven configuration storage and loading
- **source-discovery** (MODIFIED) - Add database query path alongside existing auto-discovery
- **server-initialization** (MODIFIED) - Add configuration polling background task

### Affected Code
- `martin/src/config/file/main.rs` - Add database configuration loading path
- `martin/src/config/mod.rs` - Add `ConfigSource` enum and database config support
- `martin/src/bin/martin.rs` - Initialize configuration polling task
- `martin/src/srv/mod.rs` - Add admin endpoint for manual reload
- `martin/src/config/file/tiles/postgres/builder.rs` - Add database-driven source discovery
- `martin-core/src/tiles/postgres/` - Add configuration table queries
- New: `martin/src/config/database/` module for database configuration logic
- New: `martin/sql/config_schema.sql` - Configuration table DDL

### Migration Path
1. Existing deployments continue using file-based config (no action required)
2. New deployments can opt-in by:
   - Running `martin --create-config-schema` to create configuration tables
   - Setting `config_source: database` in YAML
   - Inserting source configuration rows into `martin_config.*` tables
3. Gradual migration: Start with database config for new sources, migrate existing over time

### Breaking Changes
**None** - This is purely additive functionality with a feature flag.

### Performance Considerations
- Configuration refresh queries are read-only and lightweight (indexed lookups)
- Refresh interval is configurable (default 60s)
- Tile serving performance unchanged (sources are pre-loaded in memory)
- Initial startup slightly slower in database mode due to configuration table queries

### Security Considerations
- Configuration tables require separate PostgreSQL schema with restricted permissions
- Admin reload endpoint is disabled by default and must be explicitly enabled; protect it with authentication or a reverse proxy (future work)
- Configuration connection string can be separate from tile data connection (read-only vs full access)
