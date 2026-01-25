## ADDED Requirements

### Requirement: Server Initialization
Martin SHALL initialize tile sources during startup based on the configured configuration mode.

#### Scenario: Startup in file mode (existing behavior)
- **WHEN** Martin starts with `config_source: file` or no `config_source` specified
- **THEN** Martin loads configuration from YAML and environment variables
- **AND** discovers tile sources via auto-discovery or explicit configuration
- **AND** builds source registry
- **AND** starts HTTP server
- **AND** no background configuration poller is spawned

#### Scenario: Startup in database mode
- **WHEN** Martin starts with `config_source: database`
- **THEN** Martin loads base configuration from YAML (ports, cache, TLS)
- **AND** validates that `martin_config` schema exists in the database
- **AND** queries configuration tables for tile sources
- **AND** builds initial source registry
- **AND** starts HTTP server
- **AND** spawns background configuration poller task (if refresh interval > 0)

#### Scenario: Configuration poller lifecycle
- **WHEN** Martin starts in database mode with a valid `config_refresh_interval_seconds`
- **THEN** Martin spawns a tokio background task for configuration polling
- **AND** the poller task runs continuously until server shutdown
- **AND** the poller sleeps for the configured interval between checks
- **AND** server shutdown gracefully stops the poller task

## ADDED Requirements

### Requirement: Background Configuration Polling
Martin SHALL poll configuration tables periodically when running in database mode.

#### Scenario: Periodic configuration check
- **WHEN** the configuration poller task wakes up after the interval
- **THEN** Martin queries `martin_config.metadata.version`
- **AND** compares it to the last known version
- **AND** if unchanged, logs debug message and goes back to sleep
- **AND** if changed, proceeds with full configuration reload

#### Scenario: Configuration reload on version change
- **WHEN** configuration version has changed since last check
- **THEN** Martin queries all enabled sources from `martin_config.*` tables
- **AND** builds a new source registry
- **AND** validates all sources (logs warnings for invalid sources)
- **AND** atomically swaps the new registry into the server state
- **AND** logs info message: "Configuration reloaded: {count} sources loaded, version {new_version}"
- **AND** updates last known version to `new_version`

#### Scenario: Configuration reload error handling
- **WHEN** configuration reload fails (database error, connection timeout)
- **THEN** Martin logs error with details: "Configuration reload failed: {error}"
- **AND** keeps serving with the previous configuration
- **AND** retries on next polling interval
- **AND** server continues running (does not crash)

#### Scenario: Poller task crash recovery
- **WHEN** the configuration poller task panics or encounters an unexpected error
- **THEN** Martin logs a critical error message
- **AND** the server continues running with the last known configuration
- **AND** the poller task does not automatically restart (operator intervention required)

### Requirement: Admin Endpoints
Martin SHALL provide administrative HTTP endpoints for configuration management.

#### Scenario: Admin reload endpoint registration
- **WHEN** Martin starts in database mode
- **AND** `admin_reload_enabled: true`
- **THEN** Martin registers `POST /admin/config/reload` endpoint
- **AND** the endpoint is accessible (no authentication in v1)

#### Scenario: Admin reload endpoint not available in file mode
- **WHEN** Martin starts in file mode
- **AND** a request is sent to `POST /admin/config/reload`
- **THEN** Martin returns HTTP 400 with JSON error:
  ```json
  {
    "error": "Configuration reload is only available in database mode",
    "config_source": "file"
  }
  ```

#### Scenario: Server health includes config mode
- **WHEN** operator queries `/health` endpoint
- **THEN** response includes `config_source` field indicating current mode
- **AND** if in database mode, includes `last_config_reload` timestamp
- **AND** includes `config_version` (current version number)
