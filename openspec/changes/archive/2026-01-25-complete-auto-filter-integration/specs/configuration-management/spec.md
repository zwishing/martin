# configuration-management Spec Delta

## Added Requirements

### Requirement: Auto-Generate Filtered Functions Configuration
Martin and Maptile SHALL support configuration options to automatically generate filtered tile functions for table sources.

#### Scenario: Auto-generation enabled in maptile
- **WHEN** `postgres.auto_generate_filters: true` is set in maptile config.yaml
- **THEN** maptile automatically generates filtered tile functions for all table sources at startup
- **AND** uses the suffix specified in `postgres.filter_function_suffix` (default: "filtered")
- **AND** logs the number of functions generated

#### Scenario: Auto-generation disabled (default)
- **WHEN** `postgres.auto_generate_filters` is not set or set to `false`
- **THEN** maptile does not generate any filtered functions
- **AND** behavior is identical to previous versions (backward compatible)

#### Scenario: Custom function suffix
- **WHEN** `postgres.filter_function_suffix: "custom_suffix"` is set
- **THEN** generated functions are named `{table_name}_custom_suffix`
- **AND** smart routing looks for sources with the custom suffix

#### Scenario: Generation failure at startup
- **WHEN** auto-generation is enabled but function creation fails
- **THEN** maptile logs a warning with the error details
- **AND** continues startup normally (failure is non-fatal)
- **AND** smart routing falls back to base table sources

### Requirement: Redis Stream Triggered Auto-Generation
Maptile SHALL automatically generate filtered functions when new table sources are added via Redis stream.

#### Scenario: New table source from Redis
- **WHEN** maptile receives a Redis stream message with a new vector table
- **AND** `postgres.auto_generate_filters: true` is set
- **THEN** maptile writes the table source to the database
- **AND** automatically generates a filtered function for the new table
- **AND** refreshes the source registry to include the new function
- **AND** logs the generated function name

#### Scenario: Generation failure during Redis processing
- **WHEN** auto-generation is enabled but function creation fails during Redis message processing
- **THEN** maptile logs a warning with the error details
- **AND** continues processing the message (failure is non-fatal)
- **AND** the table source is still added successfully
- **AND** smart routing falls back to the base table source

## Modified Requirements

### Requirement: Database Configuration Loading (Modified)
Martin SHALL load tile source configuration from PostgreSQL tables when in database mode, including auto-generated filtered functions.

#### Scenario: Load auto-generated function sources (NEW)
- **WHEN** Martin loads configuration from database
- **AND** auto-generated filtered functions exist (e.g., `cities_filtered`)
- **THEN** Martin discovers and loads these functions as function sources
- **AND** smart routing can use them when filter parameters are present

## Configuration Schema

### maptile config.yaml

```yaml
postgres:
  connection_string: "postgresql://..."
  pool_size: 10

  # Auto-generate filtered functions (default: false)
  auto_generate_filters: false

  # Filtered function suffix (default: "filtered")
  filter_function_suffix: "filtered"
```

### martin config.yaml

```yaml
postgres:
  connection_string: "postgresql://..."
  auto_publish:
    from_schemas: public

    # Auto-generate filtered functions (default: false)
    auto_generate_filters: false

    # Filtered function suffix (default: "filtered")
    filter_function_suffix: "filtered"
```

## Backward Compatibility

- All new configuration fields have default values
- Default behavior (`auto_generate_filters: false`) is identical to previous versions
- Existing configurations work without modification
- Smart routing works independently of auto-generation
