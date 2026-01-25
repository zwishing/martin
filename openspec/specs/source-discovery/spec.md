# source-discovery Specification

## Purpose
TBD - created by archiving change add-db-driven-config. Update Purpose after archive.
## Requirements
### Requirement: PostgreSQL Source Discovery
PostgreSQL tile sources SHALL be discovered through auto-discovery OR explicit configuration from YAML OR database tables.

#### Scenario: Auto-discovery in file mode
- **WHEN** Martin runs in file mode with `auto_publish` enabled
- **THEN** Martin scans `geometry_columns` and `pg_proc` for spatial tables and MVT functions
- **AND** creates tile sources for discovered objects
- **AND** uses default source ID format (schema.table or schema.function)

#### Scenario: Explicit YAML configuration in file mode
- **WHEN** Martin runs in file mode with explicit `postgres.tables` or `postgres.functions` in YAML
- **THEN** Martin loads only the explicitly configured sources
- **AND** skips auto-discovery for explicitly configured sources

#### Scenario: Database-driven discovery
- **WHEN** Martin runs in database mode
- **THEN** Martin queries `martin_config.data_sources` table for PostgreSQL sources
- **AND** creates tile sources based on table rows
- **AND** auto-discovery is disabled (explicit configuration only)
- **AND** uses `source_id` from configuration table as source identifier

#### Scenario: Mixed mode not supported
- **WHEN** Martin is in database mode
- **AND** YAML contains explicit tile source definitions (postgres.tables, postgres.functions, pmtiles, mbtiles, cog)
- **THEN** Martin logs a warning: "Tile source definitions in YAML are ignored in database mode"
- **AND** only uses database configuration

### Requirement: File Source Discovery
File-based tile sources (MBTiles, PMTiles, COG) SHALL be discovered through file path scanning OR explicit configuration from YAML OR database tables.

#### Scenario: File path scanning in file mode
- **WHEN** Martin runs in file mode with `pmtiles.paths` or `mbtiles.paths` configured
- **THEN** Martin scans the specified directories for matching file extensions
- **AND** creates tile sources for each discovered file
- **AND** derives source ID from file name

#### Scenario: Explicit file configuration in file mode
- **WHEN** Martin runs in file mode with explicit file source configuration in YAML
- **THEN** Martin loads sources for the specified file paths
- **AND** uses configured source IDs

#### Scenario: Database-driven file sources
- **WHEN** Martin runs in database mode
- **THEN** Martin queries `martin_config.file_sources` table
- **AND** creates file-based tile sources based on `source_type` (mbtiles, pmtiles, cog)
- **AND** validates that `file_path` is accessible
- **AND** uses `source_id` from configuration table as source identifier

#### Scenario: Relative vs absolute paths
- **WHEN** `file_path` in database configuration is relative
- **THEN** Martin resolves it relative to the working directory (same behavior as file mode)
- **AND** logs a warning recommending absolute paths for clarity

