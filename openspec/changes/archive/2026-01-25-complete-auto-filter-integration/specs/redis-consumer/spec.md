# redis-consumer Spec Delta

## Modified Requirements

### Requirement: Vector Dataset Message Processing (Modified)
Maptile SHALL process vector dataset messages from Redis stream and optionally auto-generate filtered functions.

#### Scenario: Process vector message with auto-generation enabled (NEW)
- **WHEN** maptile receives a Redis stream message with `kind: "vector"`
- **AND** `postgres.auto_generate_filters: true` is set in configuration
- **THEN** maptile parses the vector metadata from the payload
- **AND** writes the vector source to `martin_config.data_sources`
- **AND** automatically generates a filtered tile function for the table
- **AND** refreshes the source registry to include both the table and filtered function
- **AND** logs success with the source_id and generated function name
- **AND** acknowledges the Redis message

#### Scenario: Process vector message with auto-generation disabled (EXISTING)
- **WHEN** maptile receives a Redis stream message with `kind: "vector"`
- **AND** `postgres.auto_generate_filters` is `false` or not set
- **THEN** maptile parses the vector metadata from the payload
- **AND** writes the vector source to `martin_config.data_sources`
- **AND** refreshes the source registry to include the table source
- **AND** logs success with the source_id
- **AND** acknowledges the Redis message
- **AND** does NOT generate any filtered functions

#### Scenario: Auto-generation failure during message processing (NEW)
- **WHEN** maptile receives a Redis stream message with `kind: "vector"`
- **AND** `postgres.auto_generate_filters: true` is set
- **AND** filtered function generation fails (e.g., SQL error, permission denied)
- **THEN** maptile logs a warning with the error details
- **AND** continues processing the message normally
- **AND** writes the vector source to the database successfully
- **AND** refreshes the source registry (includes table source, excludes failed function)
- **AND** acknowledges the Redis message (message is not retried)
- **AND** smart routing falls back to the base table source

## Implementation Details

### Function Generation Flow

```
Redis Message Received
  ↓
Parse Vector Metadata
  ↓
Write Vector Source to DB
  ↓
[If auto_generate_filters enabled]
  ↓
Build TableInfo from VectorDataSource
  ↓
Call create_filtered_function()
  ↓
[On Success] Log function name
[On Failure] Log warning, continue
  ↓
Refresh Source Registry
  ↓
Acknowledge Message
```

### TableInfo Construction

When constructing `TableInfo` for auto-generation:

```rust
TableInfo {
    schema: data_source.schema_name,
    table: data_source.table_or_function_name,
    geometry_column: data_source.geometry_column,
    srid: data_source.srid.unwrap_or(4326),
    extent: Some(4096),
    buffer: Some(64),
    clip_geom: Some(true),
    geometry_type: None,
    properties: data_source.properties,
}
```

### Error Handling

- Function generation errors are logged as warnings, not errors
- Message processing continues even if generation fails
- Redis message is acknowledged (not retried)
- Source registry includes the base table source
- Smart routing automatically falls back to base source

## Configuration

Uses the same configuration fields as startup auto-generation:

```yaml
postgres:
  auto_generate_filters: true  # Enable auto-generation
  filter_function_suffix: "filtered"  # Function suffix
```

## Backward Compatibility

- Default behavior (`auto_generate_filters: false`) is unchanged
- Existing Redis message processing flow is preserved
- No breaking changes to message format or database schema
