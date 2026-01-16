## 1. Database Schema Setup
- [ ] 1.1 Create SQL migration script for `martin_config` schema
- [ ] 1.2 Create `martin_config.metadata` table with version tracking
- [ ] 1.3 Create `martin_config.data_sources` table for PostgreSQL sources
- [ ] 1.4 Create `martin_config.file_sources` table for file-based sources
- [ ] 1.5 Add appropriate indexes for performance (enabled, schema lookups)
- [ ] 1.6 Write SQL script documentation and examples

## 2. Configuration Data Structures
- [ ] 2.1 Add `ConfigSource` enum (File, Database) to `martin/src/config/mod.rs`
- [ ] 2.2 Add `config_source`, `config_refresh_interval_seconds`, and `admin_reload_enabled` fields to `Config` struct
- [ ] 2.3 Create `martin/src/config/database/` module directory
- [ ] 2.4 Define structs for database configuration rows (`DataSourceRow`, `FileSourceRow`, `MetadataRow`)
- [ ] 2.5 Implement deserialization from database query results to config structs
- [ ] 2.6 Add validation for minimum refresh interval (10 seconds)

## 3. Database Configuration Loading
- [ ] 3.1 Implement `query_config_metadata()` function to fetch version and timestamp
- [ ] 3.2 Implement `query_data_sources()` to fetch PostgreSQL table/function sources
- [ ] 3.3 Implement `query_file_sources()` to fetch MBTiles/PMTiles/COG sources
- [ ] 3.4 Create `build_sources_from_database()` function to construct source registry
- [ ] 3.5 Add database schema validation on startup
- [ ] 3.6 Implement error handling for database query failures
- [ ] 3.7 Add configuration row validation (required fields, valid enums)

## 4. Configuration Mode Selection
- [ ] 4.1 Update `Config::resolve()` to branch on `config_source` mode
- [ ] 4.2 In file mode: Use existing source discovery logic (no changes)
- [ ] 4.3 In database mode: Call database configuration loading functions
- [ ] 4.4 Add validation warning if YAML has tile sources defined in database mode
- [ ] 4.5 Update startup logging to indicate active configuration mode

## 5. Background Configuration Polling
- [ ] 5.1 Create `ConfigPoller` struct in `martin/src/config/database/poller.rs`
- [ ] 5.2 Implement version-based polling logic (compare current vs last known version)
- [ ] 5.3 Add tokio interval timer with configurable duration
- [ ] 5.4 Implement atomic source registry swap using `Arc<RwLock<SourceRegistry>>`
- [ ] 5.5 Add graceful shutdown for poller task
- [ ] 5.6 Add error handling and retry logic (log errors, don't crash)
- [ ] 5.7 Spawn poller task in `martin/src/bin/martin.rs` after server initialization

## 6. Admin Reload Endpoint
- [ ] 6.1 Create `POST /admin/config/reload` endpoint handler in `martin/src/srv/admin.rs`
- [ ] 6.2 Implement manual reload trigger (same logic as poller)
- [ ] 6.3 Return JSON response with success status and source count
- [ ] 6.4 Return HTTP 400 in file mode with appropriate error message
- [ ] 6.5 Add error handling for reload failures (return HTTP 500)
- [ ] 6.6 Register admin endpoint conditionally (only when `admin_reload_enabled` is true in database mode)

## 7. Health Endpoint Enhancement
- [ ] 7.1 Update `/health` endpoint to include `config_source` field
- [ ] 7.2 Add `config_version` field (current metadata version)
- [ ] 7.3 Add `last_config_reload` timestamp (database mode only)
- [ ] 7.4 Write tests for health endpoint response format

## 8. CLI Commands
- [ ] 8.1 Add `--create-config-schema` CLI flag to create database tables
- [ ] 8.2 Implement schema creation logic (execute SQL migration script)
- [ ] 8.3 Add `--export-config-to-db` CLI flag to migrate YAML to database
- [ ] 8.4 Implement config export logic (parse YAML, insert into tables, increment version)
- [ ] 8.5 Add `--overwrite` flag for export to replace existing entries
- [ ] 8.6 Add `--validate-db-config` CLI flag to validate database configuration
- [ ] 8.7 Implement validation logic (query tables, check for errors)

## 9. Source Discovery Updates
- [ ] 9.1 Update `PostgresAutoDiscoveryBuilder` to support database mode
- [ ] 9.2 Disable auto-discovery in database mode (explicit config only)
- [ ] 9.3 Update file source discovery to support database mode
- [ ] 9.4 Ensure source ID resolution works in both modes
- [ ] 9.5 Add tests for source discovery in database mode

## 10. Testing
- [ ] 10.1 Write unit tests for configuration query functions
- [ ] 10.2 Write unit tests for configuration row validation
- [ ] 10.3 Write unit tests for version-based polling logic
- [ ] 10.4 Write integration tests for database mode startup
- [ ] 10.5 Write integration tests for configuration reload (version change)
- [ ] 10.6 Write integration tests for admin reload endpoint
- [ ] 10.7 Write integration tests for config export CLI command
- [ ] 10.8 Add test fixtures with sample configuration tables
- [ ] 10.9 Test error scenarios (missing schema, invalid rows, database failures)
- [ ] 10.10 Test migration path (file mode → database mode)

## 11. Documentation
- [ ] 11.1 Update `CLAUDE.md` with database configuration mode documentation
- [ ] 11.2 Create user guide: "Database-Driven Configuration"
- [ ] 11.3 Document configuration table schema with examples
- [ ] 11.4 Document configuration migration steps (file → database)
- [ ] 11.5 Add example configuration YAML for database mode
- [ ] 11.6 Document admin reload endpoint in API docs
- [ ] 11.7 Add troubleshooting guide for common issues
- [ ] 11.8 Update `--help` output with new CLI flags

## 12. Code Quality
- [ ] 12.1 Run `cargo fmt` on all new and modified files
- [ ] 12.2 Run `cargo clippy` and fix all warnings
- [ ] 12.3 Run `just check` to validate all feature combinations
- [ ] 12.4 Update CHANGELOG.md with feature description
- [ ] 12.5 Review error messages for clarity and actionability

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
