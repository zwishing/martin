# Database System for Martin Configuration

Martin supports a database-driven configuration model that allows you to store tile source definitions (PostgreSQL tables/functions and file sources like MBTiles/PMTiles) in a PostgreSQL database. This enables dynamic configuration updates without restarting the server or modifying configuration files.

## 1. Setup

### 1.1 Create Configuration Schema

First, you need to set up the necessary tables in your PostgreSQL database. Martin provides a CLI command to handle this automatically as a migration.

**Command:**

```bash
# Assumes DATABASE_URL environment variable is set
martin --create-config-schema
```

Or using an explicit connection string:

```bash
martin postgres://postgres:password@localhost:5432/db --create-config-schema
```

This will create a `martin_config` schema with the following tables:

- `martin_config.metadata`: Tracks configuration version.
- `martin_config.data_sources`: Stores PostgreSQL table and function source definitions.
- `martin_config.file_sources`: Stores file-based source definitions (MBTiles, PMTiles, COG).

### 1.2 Export Existing Configuration (Optional)

If you have an existing YAML configuration file, you can export it to the database.

**Command:**

```bash
martin --config config.yaml --export-config-to-db
```

Use `--overwrite` to replace existing entries if you are re-exporting.

```bash
martin --config config.yaml --export-config-to-db --overwrite
```

## 2. Running in Database Mode

To run Martin in database-driven mode, you must enable the database configuration source in your `config.yaml` or provide appropriate CLI flags.

### 2.1 Configuration File

Create or update your `config.yaml`:

```yaml
# config.yaml (example)
config_source: database
config_refresh_interval_seconds: 60 # Check for DB updates every 60s

postgres:
  - connection_string: "postgresql://postgres:password@localhost/db"
    # Note: auto_publish is disabled in database mode; sources must be explicitly defined in DB
```

### 2.2 Start Martin

```bash
martin --config config.yaml
```

Martin will now:

1. Connect to the database.
2. Load configuration from `martin_config` tables.
3. Start the tile server.
4. Periodically (every 60s) check `martin_config.metadata.version` and reload if changed.

## 3. Managing Configuration

You can manage configuration by inserting or updating rows in the `martin_config` tables using standard SQL.

### 3.1 Adding a Table Source via SQL

```sql
INSERT INTO martin_config.data_sources (
    source_id, source_type, schema_name, table_or_function_name, geometry_column, srid
) VALUES (
    'my_points', 'table', 'public', 'points', 'geom', 4326
);

-- IMPORTANT: Increment version to trigger reload on Martin servers
SELECT martin_config.increment_version();
```

### 3.2 Adding a File Source via SQL

```sql
INSERT INTO martin_config.file_sources (
    source_id, source_type, file_path
) VALUES (
    'world_cities', 'mbtiles', '/path/to/files/cities.mbtiles'
);

-- Trigger reload
SELECT martin_config.increment_version();
```

## 4. Verification & Testing

### 4.1 CLI Verification

Validate the database configuration without starting the server:

```bash
martin --config config.yaml --validate-db-config
```

### 4.2 Admin API Reload

If you have `admin_reload_enabled: true` in your config config, you can force a reload via HTTP:

```bash
curl -X POST http://localhost:3000/admin/config/reload
```

### 4.3 Health Check

Check the `/health` endpoint to see the current config version:

```bash
curl http://localhost:3000/health
```

Output should include `config_source`, `config_version`, and `last_config_reload`.
