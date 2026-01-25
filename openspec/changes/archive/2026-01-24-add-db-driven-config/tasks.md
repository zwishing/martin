## 1. Database Schema Setup
- [x] 1.1 Create SQL migration script for `martin_config` schema
- [x] 1.2 Create `martin_config.metadata` table with version tracking
- [x] 1.3 Create `martin_config.data_sources` table for PostgreSQL sources
- [x] 1.4 Create `martin_config.file_sources` table for file-based sources
- [x] 1.5 Add appropriate indexes for performance (enabled, schema lookups)
- [x] 1.6 Write SQL script documentation and examples

## 2. Configuration Data Structures
- [x] 2.1 Add `ConfigSource` enum (File, Database) to `martin/src/config/mod.rs`
- [x] 2.2 Add `config_source`, `config_refresh_interval_seconds`, and `admin_reload_enabled` fields to `Config` struct
- [x] 2.3 Create `martin/src/config/database/` module directory
- [x] 2.4 Define structs for database configuration rows (`DataSourceRow`, `FileSourceRow`, `MetadataRow`)
- [x] 2.5 Implement deserialization from database query results to config structs
- [x] 2.6 Add validation for minimum refresh interval (10 seconds)

## 3. Database Configuration Loading
- [x] 3.1 Implement `query_config_metadata()` function to fetch version and timestamp
- [x] 3.2 Implement `query_data_sources()` to fetch PostgreSQL table/function sources
- [x] 3.3 Implement `query_file_sources()` to fetch MBTiles/PMTiles/COG sources
- [x] 3.4 Create `build_sources_from_database()` function to construct source registry
- [x] 3.5 Add database schema validation on startup
- [x] 3.6 Implement error handling for database query failures
- [x] 3.7 Add configuration row validation (required fields, valid enums)

## 4. Configuration Mode Selection
- [x] 4.1 Update `Config::resolve()` to branch on `config_source` mode
- [x] 4.2 In file mode: Use existing source discovery logic (no changes)
- [x] 4.3 In database mode: Call database configuration loading functions
- [x] 4.4 Add validation warning if YAML has tile sources defined in database mode
- [x] 4.5 Update startup logging to indicate active configuration mode

## 5. Background Configuration Polling
- [x] 5.1 Create `ConfigPoller` struct in `martin/src/config/database/poller.rs`
- [x] 5.2 Implement version-based polling logic (compare current vs last known version)
- [x] 5.3 Add tokio interval timer with configurable duration
- [x] 5.4 Implement atomic source registry swap using `Arc<RwLock<SourceRegistry>>`
- [x] 5.5 Add graceful shutdown for poller task
- [x] 5.6 Add error handling and retry logic (log errors, don't crash)
- [x] 5.7 Spawn poller task in `martin/src/bin/martin.rs` after server initialization

## 6. Admin Reload Endpoint
- [x] 6.1 Create `POST /admin/config/reload` endpoint handler in `martin/src/srv/admin.rs`
- [x] 6.2 Implement manual reload trigger (same logic as poller)
- [x] 6.3 Return JSON response with success status and source count
- [x] 6.4 Return HTTP 400 in file mode with appropriate error message
- [x] 6.5 Add error handling for reload failures (return HTTP 500)
- [x] 6.6 Register admin endpoint conditionally (only when `admin_reload_enabled` is true in database mode)

## 7. Health Endpoint Enhancement
- [x] 7.1 Update `/health` endpoint to include `config_source` field
- [x] 7.2 Add `config_version` field (current metadata version)
- [x] 7.3 Add `last_config_reload` timestamp (database mode only)
- [x] 7.4 Write tests for health endpoint response format

## 8. CLI Commands
- [x] 8.1 Add `--create-config-schema` CLI flag to create database tables
- [x] 8.2 Implement schema creation logic (execute SQL migration script)
- [x] 8.3 Add `--export-config-to-db` CLI flag to migrate YAML to database
- [x] 8.4 Implement config export logic (parse YAML, insert into tables, increment version)
- [x] 8.5 Add `--overwrite` flag for export to replace existing entries
- [x] 8.6 Add `--validate-db-config` CLI flag to validate database configuration
- [x] 8.7 Implement validation logic (query tables, check for errors)

## 9. Source Discovery Updates
- [x] 9.1 Update `PostgresAutoDiscoveryBuilder` to support database mode
- [x] 9.2 Disable auto-discovery in database mode (explicit config only)
- [x] 9.3 Update file source discovery to support database mode
- [x] 9.4 Ensure source ID resolution works in both modes
- [x] 9.5 Add tests for source discovery in database mode

## 10. Testing
- [x] 10.1 Write unit tests for configuration query functions
- [x] 10.2 Write unit tests for configuration row validation
- [x] 10.3 Write unit tests for version-based polling logic
- [x] 10.4 Write integration tests for database mode startup
- [x] 10.5 Write integration tests for configuration reload (version change)
- [x] 10.6 Write integration tests for admin reload endpoint
- [x] 10.7 Write integration tests for config export CLI command
- [x] 10.8 Add test fixtures with sample configuration tables
- [x] 10.9 Test error scenarios (missing schema, invalid rows, database failures)
- [x] 10.10 Test migration path (file mode → database mode)

## 11. Documentation
- [x] 11.1 Update `CLAUDE.md` with database configuration mode documentation
- [x] 11.2 Create user guide: "Database-Driven Configuration"
- [x] 11.3 Document configuration table schema with examples
- [x] 11.4 Document configuration migration steps (file → database)
- [x] 11.5 Add example configuration YAML for database mode
- [x] 11.6 Document admin reload endpoint in API docs
- [x] 11.7 Add troubleshooting guide for common issues
- [x] 11.8 Update `--help` output with new CLI flags

## 12. Code Quality
- [x] 12.1 Run `cargo fmt` on all new and modified files
- [x] 12.2 Run `cargo clippy` and fix all warnings
- [x] 12.3 Run `just check` to validate all feature combinations
- [x] 12.4 Update CHANGELOG.md with feature description
- [x] 12.5 Review error messages for clarity and actionability

## Dependencies Between Tasks
- Task 2 must complete before Task 3 (data structures needed for queries)
- Task 3 must complete before Task 4 (loading logic needed for mode selection)
- Task 4 must complete before Task 5 (source registry must support reload)
- Task 1 must complete before any integration tests (schema required)
- Task 8 depends on Task 1 and Task 3 (uses schema and loading logic)
- Task 10 can partially parallelize (unit tests early, integration tests later)

## Parallelizable Work
- Task 1 (schema) and Task 2 (data structures) can be done in parallel
- Task 6 (admin endpoint) and Task 7 (health endpoint) are independent
- Task 8 (CLI commands) can be done in parallel with Task 5 (poller)
- Task 11 (documentation) can be written concurrently with implementation
