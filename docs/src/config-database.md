# Database-Driven Configuration

Martin can load tile source definitions from PostgreSQL instead of YAML by setting
`config_source: database` in the config file. This is optional and disabled by default.

## Overview

Database mode stores tile source metadata in PostgreSQL tables under the
`martin_config` schema and reloads them periodically. Base server settings (ports,
cache sizes, TLS, etc.) are still read from YAML.

## Setup

1. Create the configuration schema:

```bash
martin --create-config-schema
```

2. Populate configuration tables (examples in `martin/sql/README.md`).
3. Update your config:

```yaml
config_source: database
config_refresh_interval_seconds: 60
admin_reload_enabled: false
```

4. Restart Martin.

## Version Updates

Martin reloads configuration when `martin_config.metadata.version` changes.
External tooling MUST increment this value after any insert, update, or delete:

```sql
SELECT martin_config.increment_version();
```

## Manual Reload

If you enable `admin_reload_enabled: true`, you can trigger reloads manually:

```bash
curl -X POST http://localhost:3000/admin/config/reload
```

This endpoint returns 404 when disabled.

## Polling Interval

`config_refresh_interval_seconds` defaults to 60 seconds with a minimum of 10 seconds.

## Validation and Export

Use the CLI helpers to validate or migrate file-based sources into the database:

```bash
martin --validate-db-config
martin --export-config-to-db --config martin.yaml
```
