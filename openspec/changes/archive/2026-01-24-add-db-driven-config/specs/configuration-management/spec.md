## ADDED Requirements

### Requirement: Database Configuration Schema
Martin SHALL provide a PostgreSQL schema for storing tile source configuration metadata in the database.

#### Scenario: Configuration tables exist
- **WHEN** operator runs `martin --create-config-schema`
- **THEN** the following tables are created in the `martin_config` schema:
  - `martin_config.metadata` (singleton table with version and updated_at)
  - `martin_config.data_sources` (PostgreSQL table/function sources)
  - `martin_config.file_sources` (MBTiles/PMTiles/COG file sources)

#### Scenario: Schema validation on startup
- **WHEN** Martin starts in database mode
- **THEN** Martin validates that the configuration schema exists and has the correct structure
- **AND** logs an error and exits if schema is missing or incompatible

#### Scenario: External version updates
- **WHEN** external systems update `martin_config.data_sources` or `martin_config.file_sources`
- **THEN** they increment `martin_config.metadata.version`
- **AND** Martin uses that version change to trigger reloads

### Requirement: Configuration Source Mode
Martin SHALL support two mutually exclusive configuration modes: file-based (default) and database-driven.

#### Scenario: File mode (default behavior)
- **WHEN** `config_source` is not specified or set to `file` in YAML
- **THEN** Martin loads tile source configuration from YAML file or auto-discovery
- **AND** ignores configuration tables even if they exist

#### Scenario: Database mode
- **WHEN** `config_source: database` is set in YAML
- **THEN** Martin queries `martin_config.*` tables for tile source configuration
- **AND** ignores tile source definitions in YAML (postgres.tables, postgres.functions, pmtiles, mbtiles, cog sections)
- **AND** base configuration (ports, cache, pool size) still comes from YAML

#### Scenario: Invalid mode value
- **WHEN** `config_source` is set to an unsupported value
- **THEN** Martin logs an error and exits with non-zero status code

### Requirement: Database Configuration Loading
Martin SHALL load tile source configuration from PostgreSQL tables when in database mode.

#### Scenario: Load PostgreSQL table sources
- **WHEN** Martin loads configuration from database
- **THEN** Martin queries `martin_config.data_sources` WHERE `source_type = 'table'` AND `enabled = true`
- **AND** creates table-based tile sources for each row
- **AND** uses `source_id` as the source identifier in URLs

#### Scenario: Load PostgreSQL function sources
- **WHEN** Martin loads configuration from database
- **THEN** Martin queries `martin_config.data_sources` WHERE `source_type = 'function'` AND `enabled = true`
- **AND** creates function-based tile sources for each row
- **AND** validates that the function exists in the database

#### Scenario: Load MBTiles/PMTiles/COG sources
- **WHEN** Martin loads configuration from database
- **THEN** Martin queries `martin_config.file_sources` WHERE `enabled = true`
- **AND** creates file-based tile sources for each row based on `source_type`
- **AND** validates that the file path is accessible

#### Scenario: Empty configuration
- **WHEN** all configuration tables return zero enabled sources
- **THEN** Martin logs an error message about no sources available
- **AND** exits with non-zero status code (same as file mode with no sources)

#### Scenario: Configuration load failure
- **WHEN** querying configuration tables fails (database unreachable, schema invalid)
- **THEN** Martin logs the error with details
- **AND** exits with non-zero status code on startup
- **AND** logs warning but continues serving (with stale config) if error occurs during runtime refresh

### Requirement: Configuration Refresh Interval
Martin SHALL periodically refresh configuration from the database when in database mode.

#### Scenario: Configurable refresh interval
- **WHEN** `config_refresh_interval_seconds` is set in YAML (database mode)
- **THEN** Martin polls the configuration tables at the specified interval
- **AND** the minimum allowed interval is 10 seconds

#### Scenario: Default refresh interval
- **WHEN** `config_refresh_interval_seconds` is not specified
- **THEN** Martin uses a default interval of 60 seconds

#### Scenario: Version-based refresh
- **WHEN** the configuration poller checks for updates
- **THEN** Martin queries `martin_config.metadata.version`
- **AND** if the version is unchanged, skips reloading sources
- **AND** if the version has changed, reloads all sources from configuration tables

### Requirement: Dynamic Source Reload
Martin SHALL reload tile sources without restarting the server when configuration changes are detected.

#### Scenario: Successful reload
- **WHEN** configuration version changes and Martin reloads sources
- **THEN** Martin builds a new source registry from configuration tables
- **AND** replaces the old registry atomically (no partial state visible to requests)
- **AND** tile serving endpoints immediately use the new sources
- **AND** old source tiles remain in cache (LRU eviction)
- **AND** logs info message: "Configuration reloaded: X sources loaded"

#### Scenario: Reload with errors
- **WHEN** some sources fail to load during configuration refresh
- **THEN** Martin logs warnings for failed sources with details
- **AND** loads successfully validated sources
- **AND** continues serving tiles from the new registry (excluding failed sources)

#### Scenario: Reload preserves tile cache
- **WHEN** configuration is reloaded
- **THEN** the tile cache is not cleared
- **AND** cached tiles for unchanged sources remain valid
- **AND** requests for new/changed sources miss cache and generate fresh tiles

### Requirement: Manual Configuration Reload
Martin SHALL provide an HTTP endpoint to trigger immediate configuration reload when `admin_reload_enabled: true`.

#### Scenario: Manual reload request
- **WHEN** operator sends `POST /admin/config/reload`
- **THEN** Martin immediately queries configuration tables
- **AND** reloads sources (same logic as periodic refresh)
- **AND** returns HTTP 200 with JSON response: `{"status": "success", "sources_loaded": N}`

#### Scenario: Manual reload failure
- **WHEN** `POST /admin/config/reload` is called but reload fails
- **THEN** Martin returns HTTP 500 with JSON error details
- **AND** keeps serving with previous configuration

#### Scenario: Manual reload in file mode
- **WHEN** `POST /admin/config/reload` is called in file mode
- **THEN** Martin returns HTTP 400 with error message: "Manual reload only supported in database mode"

#### Scenario: Manual reload disabled in config
- **WHEN** `admin_reload_enabled` is false
- **AND** a request is sent to `POST /admin/config/reload`
- **THEN** Martin returns HTTP 404

### Requirement: Configuration Export
Martin SHALL provide a tool to export existing file-based configuration to database tables.

#### Scenario: Export config to database
- **WHEN** operator runs `martin --export-config-to-db --config martin.yaml`
- **THEN** Martin parses the YAML configuration
- **AND** inserts tile source definitions into `martin_config.*` tables
- **AND** logs success message with count of exported sources
- **AND** increments `martin_config.metadata.version`

#### Scenario: Export with conflicts
- **WHEN** exporting configuration with source IDs that already exist in database
- **THEN** Martin logs warning about conflicts
- **AND** skips conflicting sources (no overwrite)
- **AND** operator can use `--overwrite` flag to replace existing entries

### Requirement: Configuration Validation
Martin SHALL validate database configuration before starting or reloading.

#### Scenario: Invalid source definition
- **WHEN** a configuration table row has invalid or missing required fields
- **THEN** Martin logs a validation error with source_id and field details
- **AND** skips the invalid source
- **AND** continues loading other valid sources

#### Scenario: Duplicate source IDs
- **WHEN** multiple configuration rows have the same source_id
- **THEN** Martin logs an error about duplicate source ID
- **AND** uses the first occurrence and skips duplicates

#### Scenario: Non-existent database objects
- **WHEN** a data_sources row references a non-existent table or function
- **THEN** Martin logs a warning with schema.object details
- **AND** skips the source
- **AND** continues loading other sources

#### Scenario: Inaccessible file paths
- **WHEN** a file_sources row references a non-existent or inaccessible file path
- **THEN** Martin logs a warning with file path details
- **AND** skips the source
- **AND** continues loading other sources
