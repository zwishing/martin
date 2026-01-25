# server-initialization Spec Delta

## Modified Requirements

### Requirement: Maptile Service Initialization (Modified)
Maptile SHALL initialize the RPC service with database connection pool, load sources, and optionally auto-generate filtered functions.

#### Scenario: Initialize with auto-generation enabled (NEW)
- **WHEN** maptile starts with `postgres.auto_generate_filters: true` in configuration
- **THEN** maptile creates the PostgreSQL connection pool
- **AND** initializes the MaptileServiceImpl with loaded sources
- **AND** queries all table sources from the database
- **AND** automatically generates filtered tile functions for each table
- **AND** logs the number of functions generated successfully
- **AND** reloads the source registry to include the new functions
- **AND** starts the RPC server
- **AND** continues startup even if some function generations fail

#### Scenario: Initialize with auto-generation disabled (EXISTING)
- **WHEN** maptile starts with `postgres.auto_generate_filters: false` or not set
- **THEN** maptile creates the PostgreSQL connection pool
- **AND** initializes the MaptileServiceImpl with loaded sources
- **AND** starts the RPC server
- **AND** does NOT generate any filtered functions

#### Scenario: Auto-generation failure at startup (NEW)
- **WHEN** maptile starts with `postgres.auto_generate_filters: true`
- **AND** some or all function generations fail
- **THEN** maptile logs warnings for each failure with error details
- **AND** continues startup normally (failures are non-fatal)
- **AND** successfully generated functions are available
- **AND** failed functions are not available (smart routing falls back to base tables)
- **AND** RPC server starts and accepts requests

#### Scenario: Auto-generation with no table sources (NEW)
- **WHEN** maptile starts with `postgres.auto_generate_filters: true`
- **AND** no table sources exist in the database
- **THEN** maptile logs that no tables were found for auto-generation
- **AND** continues startup normally
- **AND** RPC server starts and accepts requests

## Implementation Details

### Startup Sequence

```
1. Load configuration from config.yaml
2. Create PostgreSQL connection pool
3. Initialize MaptileServiceImpl (loads sources)
4. [If auto_generate_filters enabled]
   4a. Query available tables from database
   4b. Generate filtered function for each table
   4c. Log generation results
   4d. Reload source registry
5. Start RPC server
6. Start config reload task (if enabled)
7. Start Redis consumer task (if enabled)
```

### Auto-Generation Timing

- Auto-generation happens AFTER service initialization
- Auto-generation happens BEFORE RPC server starts accepting requests
- Source registry is reloaded AFTER all functions are generated
- Failures do not prevent server startup

### Logging

Success:
```
[INFO] Auto-generating filtered functions...
[INFO] Generated filtered function: public.cities_filtered
[INFO] Generated filtered function: public.roads_filtered
[INFO] Generated 2 filtered functions
[INFO] Reloaded source registry after auto-generation
```

Failure:
```
[INFO] Auto-generating filtered functions...
[WARN] Failed to generate filtered function for 'cities': permission denied
[INFO] Generated 0 filtered functions
```

No tables:
```
[INFO] Auto-generating filtered functions...
[WARN] No tables found for auto-generation
```

## Configuration

```yaml
postgres:
  connection_string: "postgresql://..."
  pool_size: 10

  # Enable auto-generation at startup
  auto_generate_filters: true

  # Function suffix (default: "filtered")
  filter_function_suffix: "filtered"
```

## Performance Considerations

- Auto-generation is synchronous during startup
- Each function generation requires:
  - 1 query to get table columns
  - 1 DDL statement to create function
- For N tables, startup time increases by approximately N * 50ms
- Example: 100 tables → ~5 seconds additional startup time
- Failures are logged but do not block startup

## Backward Compatibility

- Default behavior (`auto_generate_filters: false`) is unchanged
- Startup sequence is identical when auto-generation is disabled
- No breaking changes to configuration format
- Existing deployments work without modification
