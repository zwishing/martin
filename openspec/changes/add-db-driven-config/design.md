# Design: Database-Driven Dynamic Configuration

## Context

Martin is a high-performance tile server that currently loads all configuration at startup from YAML files or CLI arguments. This works well for static deployments but creates friction in dynamic environments where:

- Data pipelines continuously add new spatial tables that should be automatically served as tile sources
- Multi-tenant systems need programmatic control over which sources are exposed
- System integrations require runtime configuration changes without service disruption
- Configuration management tools need a structured API rather than file manipulation

The current architecture separates concerns cleanly:
- **Base configuration** (ports, TLS, cache sizes, pool sizes) → Loaded once at startup
- **Data source configuration** (PostgreSQL tables/functions, file paths) → Auto-discovered at startup

This proposal extends the data source configuration layer to support database-driven storage and periodic refresh, while keeping base configuration file-based.

### Stakeholders
- **System operators**: Need dynamic configuration without restarts
- **Data engineers**: Want automated tile source registration from pipelines
- **Integration developers**: Require programmatic configuration API
- **End users**: Benefit from faster time-to-availability for new data sources

### Constraints
- Must maintain backward compatibility with existing file-based configuration
- Cannot introduce breaking changes to existing API contracts
- Must not degrade tile serving performance
- Should minimize PostgreSQL schema complexity

## Goals / Non-Goals

### Goals
1. **Enable runtime configuration updates** without Martin restarts
2. **Provide database storage** for tile source metadata (PostgreSQL tables/functions, file paths)
3. **Support periodic refresh** with configurable interval
4. **Maintain backward compatibility** - file-based config remains default
5. **Graceful error handling** - configuration errors don't crash tile serving
6. **Simple schema** - Minimal tables with clear data model

### Non-Goals
1. ~~Real-time configuration updates~~ - Polling with configurable interval (default 60s) is sufficient
2. ~~Configuration UI~~ - External tools can manage configuration tables directly
3. ~~Configuration versioning/rollback~~ - Simple current-state storage only
4. ~~Built-in authentication for admin endpoint~~ - Left to reverse proxy or future middleware; endpoint is disabled by default
5. ~~Hot-reload without cache clearing~~ - Full source reload acceptable for v1
6. ~~Multi-database configuration storage~~ - PostgreSQL only (no MBTiles/PMTiles for config)

## Decisions

### Decision 1: Hybrid Configuration Model
**What**: Keep base system configuration (ports, TLS, pool sizes, cache sizes) in YAML files; only move data source configuration (table/function definitions, file paths) to database.

**Why**:
- Base configuration is infrastructure-level and changes infrequently
- Data source configuration changes frequently and benefits from database storage
- Separates concerns: infrastructure ops manage YAML, data teams manage database
- Reduces bootstrap complexity (don't need database to start server)

**Alternatives considered**:
- **Full database-driven config**: Rejected - circular dependency (need config to connect to database that stores config)
- **Full file-based only**: Current state - doesn't solve the dynamic update problem

### Decision 2: Polling-Based Refresh (Not Event-Driven)
**What**: Poll configuration tables at fixed intervals (default 60s, configurable) rather than using PostgreSQL LISTEN/NOTIFY or triggers.

**Why**:
- Simpler implementation - no persistent connection management for notifications
- More predictable resource usage - no surprise reloads during high traffic
- Easier to test and debug - deterministic timing
- Sufficient for use case - 60s refresh latency is acceptable for most scenarios

**Alternatives considered**:
- **PostgreSQL LISTEN/NOTIFY**: Rejected - adds complexity, requires dedicated connection, harder to scale
- **File system watching (inotify)**: Rejected - doesn't solve the system integration problem
- **HTTP webhook callbacks**: Rejected - requires network configuration, firewall rules

### Decision 3: Simple Schema (3 Tables)
**What**: Use 3 configuration tables in a dedicated schema:
1. `martin_config.data_sources` - PostgreSQL table/function source definitions
2. `martin_config.file_sources` - MBTiles/PMTiles/COG file source definitions
3. `martin_config.metadata` - Configuration version tracking and last update timestamp

**Why**:
- Mirrors existing in-memory data structures (`Config` struct)
- Easy to understand and query
- Supports all current source types
- Minimal JOIN complexity

**Schema details**:
```sql
CREATE SCHEMA IF NOT EXISTS martin_config;

-- Tracks configuration version for cache invalidation
CREATE TABLE martin_config.metadata (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1), -- Singleton table
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- PostgreSQL table and function source configurations
CREATE TABLE martin_config.data_sources (
    source_id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('table', 'function')),
    schema_name TEXT NOT NULL,
    table_or_function_name TEXT NOT NULL,
    geometry_column TEXT,
    srid INTEGER,
    id_column TEXT, -- For tables
    properties JSONB, -- Additional TileJSON metadata (name, description, attribution, etc.)
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_data_sources_enabled ON martin_config.data_sources (enabled) WHERE enabled;
CREATE INDEX idx_data_sources_schema ON martin_config.data_sources (schema_name, table_or_function_name);

-- File-based source configurations (MBTiles, PMTiles, COG)
CREATE TABLE martin_config.file_sources (
    source_id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('mbtiles', 'pmtiles', 'cog')),
    file_path TEXT NOT NULL,
    properties JSONB, -- Additional TileJSON metadata
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_file_sources_enabled ON martin_config.file_sources (enabled) WHERE enabled;
```

**Alternatives considered**:
- **Single unified table**: Rejected - NULL columns confusing, hard to enforce constraints
- **Separate table per source type (5+ tables)**: Rejected - over-engineered for initial version
- **Store entire TileJSON blob**: Rejected - harder to query and validate

### Decision 4: Configuration Mode Flag
**What**: Add `config_source: file | database` in YAML config (default: `file`). When set to `database`, Martin queries configuration tables instead of using YAML data source definitions.

**Why**:
- Explicit opt-in for new behavior
- Clear separation between modes
- Easy to document and understand
- Allows A/B testing and gradual migration

**Config example**:
```yaml
# Base configuration (always from file)
listening_address: 0.0.0.0:3000
cache_size_mb: 512
pool_size: 20

# Configuration mode
config_source: database  # or 'file' (default)
config_refresh_interval_seconds: 60  # Only used when config_source=database
admin_reload_enabled: false  # Explicitly enable /admin/config/reload

# PostgreSQL connection for tile data AND configuration
postgres:
  connection_string: "postgresql://user:pass@localhost/db"
```

### Decision 5: Admin Reload Endpoint Opt-In
**What**: Register `POST /admin/config/reload` only when `admin_reload_enabled: true`.

**Why**:
- Avoid exposing an unauthenticated admin surface by default
- Lets operators explicitly opt in based on their security posture

**Alternatives considered**:
- **Always enabled**: Rejected - unsafe defaults

### Decision 6: Full Source Reload (Not Hot-Swap)
**What**: On configuration refresh, rebuild entire source registry from scratch rather than incrementally adding/removing sources.

**Why**:
- Simpler implementation - no complex diff logic
- Easier to reason about - config matches database exactly after reload
- Avoids edge cases (renamed sources, circular dependencies)
- Performance acceptable - source instantiation is fast (<1s for typical setups)

**Trade-off**: Tile cache is preserved across reloads, so performance impact is minimal.

**Alternatives considered**:
- **Incremental updates (diff and apply)**: Rejected - complex, error-prone, harder to test
- **Zero-downtime hot-swap**: Future enhancement - requires more sophisticated state management

## Architecture

### Component Interaction

```
┌─────────────────────────────────────────────────────────────┐
│                         Martin Server                        │
│                                                              │
│  ┌──────────────┐         ┌─────────────────────────────┐  │
│  │  Config File │─────┬──>│   ServerConfig (base)       │  │
│  │  (YAML)      │     │   │  - ports, TLS, cache, pools │  │
│  └──────────────┘     │   └─────────────────────────────┘  │
│                       │                                      │
│                       │   ┌─────────────────────────────┐  │
│                       └──>│  ConfigSource Enum          │  │
│                           │   - File(sources)           │  │
│                           │   - Database(conn, interval)│  │
│                           └──────┬──────────────────────┘  │
│                                  │                          │
│         ┌────────────────────────┴────────┐                │
│         │                                  │                │
│    ┌────▼────┐                     ┌──────▼───────┐       │
│    │  File   │                     │  Database    │       │
│    │  Mode   │                     │  Mode        │       │
│    └────┬────┘                     └──────┬───────┘       │
│         │                                  │                │
│         │                          ┌───────▼───────┐       │
│         │                          │ Config Poller │       │
│         │                          │  (background  │       │
│         │                          │   tokio task) │       │
│         │                          └───────┬───────┘       │
│         │                                  │                │
│         │                          ┌───────▼────────┐      │
│         │                          │  Query Config  │      │
│         │                          │    Tables      │      │
│         │                          └───────┬────────┘      │
│         │                                  │                │
│    ┌────▼──────────────────────────────────▼──────┐       │
│    │      SourceRegistry (in-memory)              │       │
│    │  HashMap<SourceId, Box<dyn Source>>          │       │
│    └──────────────────┬───────────────────────────┘       │
│                       │                                    │
│                       ▼                                    │
│              ┌────────────────┐                           │
│              │  Tile Serving  │                           │
│              │   Endpoints    │                           │
│              └────────────────┘                           │
└─────────────────────────────────────────────────────────────┘
                       │
                       ▼
              ┌────────────────┐
              │  PostgreSQL    │
              │  - Tile data   │
              │  - Config data │
              └────────────────┘
```

### State Management

1. **Startup (Database Mode)**:
   - Load base config from YAML
   - Detect `config_source: database`
   - Query `martin_config.*` tables for source definitions
   - Build initial `SourceRegistry`
   - Spawn background poller task

2. **Background Poller**:
   - Sleep for `config_refresh_interval_seconds`
   - Query `martin_config.metadata` for version change
   - If version changed: Query all source tables, rebuild registry
   - Update in-memory registry atomically using `Arc<RwLock<>>`
   - External systems are responsible for incrementing `metadata.version` after configuration changes

3. **Manual Reload (when enabled)**:
   - HTTP `POST /admin/config/reload` triggers immediate refresh
   - Uses same rebuild logic as poller

### Data Flow

```
Configuration Update Flow:
┌─────────────────┐
│ External System │
│ (data pipeline, │
│  admin tool)    │
└────────┬────────┘
         │ INSERT/UPDATE
         ▼
┌─────────────────────┐
│ martin_config.*     │
│ tables              │
│ + UPDATE metadata   │
│   SET version =     │
│       version + 1   │
└────────┬────────────┘
         │
         │ (up to 60s delay)
         │
         ▼
┌─────────────────────┐
│ Config Poller       │
│ detects version     │
│ change              │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ Query all sources   │
│ from config tables  │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ Rebuild Source      │
│ Registry (atomic)   │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ New sources live    │
│ Old sources removed │
└─────────────────────┘
```

## Risks / Trade-offs

### Risk 1: Configuration Database Downtime
**Risk**: If configuration database becomes unavailable, can Martin continue serving tiles?

**Mitigation**:
- Configuration queries are separate from tile data queries
- If config refresh fails, log error but keep serving with last known good configuration
- File-based fallback: Can restart in file mode if database is down long-term
- Use separate connection pool for configuration queries to isolate failures

### Risk 2: Configuration Churn (Rapid Updates)
**Risk**: Frequent configuration updates could cause excessive source rebuilding and connection churn.

**Mitigation**:
- Minimum refresh interval (e.g., 10 seconds enforced)
- Version-based polling avoids unnecessary rebuilds when nothing changed
- Consider rate limiting on `/admin/config/reload` endpoint

### Risk 3: Schema Evolution
**Risk**: Future schema changes require database migrations, which are hard to coordinate with Martin deployments.

**Mitigation**:
- Keep schema simple and stable
- Use JSONB `properties` column for extensibility (avoid schema changes for new fields)
- Include schema version in `metadata` table for forward compatibility
- Provide SQL migration scripts in Martin repository

### Risk 4: Tile Cache Invalidation
**Risk**: When configuration changes, existing cached tiles may become stale or invalid.

**Mitigation**:
- Tile cache is keyed by `(source_id, z, x, y)` - cache automatically misses for new/changed sources
- Old source tiles remain cached until LRU eviction - acceptable for most use cases
- Future enhancement: Option to flush cache on config reload

### Risk 5: Configuration Complexity
**Risk**: Dual configuration sources (file + database) increase mental model complexity.

**Mitigation**:
- Clear documentation with decision tree: "When to use file vs database mode"
- Mutually exclusive modes: `config_source` flag makes behavior deterministic
- Validation warnings if both file-based sources and database mode are configured

## Migration Plan

### Phase 1: Schema Setup
1. Run Martin with `--create-config-schema` flag to create `martin_config.*` tables
2. Optionally populate configuration from existing YAML using provided migration script:
   ```bash
   martin --export-config-to-db --config martin.yaml
   ```

### Phase 2: Parallel Operation (Optional)
1. Keep running in file mode
2. Test database configuration by inserting test sources
3. Validate configuration with `martin --validate-db-config`

### Phase 3: Cutover
1. Update YAML: Set `config_source: database`
2. Restart Martin
3. Verify all sources loaded correctly via `/catalog` endpoint

### Rollback
1. Change YAML: Set `config_source: file`
2. Restart Martin

## Open Questions

1. **Q: Should configuration connection string be separate from tile data connection?**
   - **A**: Yes, for security - config connection needs write access (for external tools), tile data connection should be read-only. Add `config_database` field in YAML.

2. **Q: How to handle source ID conflicts between configured sources and auto-discovered sources?**
   - **A**: In database mode, disable auto-discovery (require explicit source definitions). Auto-discovery is file-mode only.

3. **Q: Should we support partial database configuration (some sources from DB, some from file)?**
   - **A**: No for v1 - too complex. Mode is all-or-nothing. Future enhancement if needed.

4. **Q: What happens if configuration table query returns 0 sources?**
   - **A**: Error and refuse to start (or refuse to reload). Prevent accidental "serve nothing" state.

5. **Q: Should metadata table track per-source update times for granular cache invalidation?**
   - **A**: No for v1 - use global version counter. Per-source tracking is future enhancement.
