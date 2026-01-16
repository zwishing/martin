-- Martin Configuration Schema
--
-- This schema enables database-driven dynamic configuration for Martin tile server.
-- It stores tile source definitions (PostgreSQL tables/functions, file-based sources)
-- that can be updated at runtime without restarting the server.
--
-- Usage:
--   psql -d your_database -f config_schema.sql
--
-- Or via Martin CLI:
--   martin --create-config-schema

-- Create dedicated schema for Martin configuration
CREATE SCHEMA IF NOT EXISTS martin_config;

-- ============================================================================
-- Metadata Table (Singleton)
-- ============================================================================
-- Tracks configuration version for change detection and caching
CREATE TABLE IF NOT EXISTS martin_config.metadata (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1), -- Enforce singleton
    version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT metadata_single_row CHECK (id = 1)
);

-- Initialize metadata table with default values
INSERT INTO martin_config.metadata (id, version, updated_at)
VALUES (1, 1, NOW())
ON CONFLICT (id) DO NOTHING;

-- Helper function to increment version (use in triggers or manual updates)
CREATE OR REPLACE FUNCTION martin_config.increment_version()
RETURNS void AS $$
BEGIN
    UPDATE martin_config.metadata
    SET version = version + 1, updated_at = NOW()
    WHERE id = 1;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Data Sources Table (PostgreSQL tables and functions)
-- ============================================================================
CREATE TABLE IF NOT EXISTS martin_config.data_sources (
    source_id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('table', 'function')),
    schema_name TEXT NOT NULL,
    table_or_function_name TEXT NOT NULL,
    geometry_column TEXT,
    srid INTEGER,
    id_column TEXT, -- For tables only
    properties JSONB, -- Additional TileJSON metadata
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT data_sources_source_id_not_empty CHECK (char_length(source_id) > 0),
    CONSTRAINT data_sources_schema_not_empty CHECK (char_length(schema_name) > 0),
    CONSTRAINT data_sources_name_not_empty CHECK (char_length(table_or_function_name) > 0)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_data_sources_enabled
    ON martin_config.data_sources (enabled)
    WHERE enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_data_sources_schema
    ON martin_config.data_sources (schema_name, table_or_function_name);

CREATE INDEX IF NOT EXISTS idx_data_sources_type
    ON martin_config.data_sources (source_type)
    WHERE enabled = TRUE;

-- ============================================================================
-- File Sources Table (MBTiles, PMTiles, COG)
-- ============================================================================
CREATE TABLE IF NOT EXISTS martin_config.file_sources (
    source_id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('mbtiles', 'pmtiles', 'cog')),
    file_path TEXT NOT NULL,
    properties JSONB, -- Additional TileJSON metadata
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT file_sources_source_id_not_empty CHECK (char_length(source_id) > 0),
    CONSTRAINT file_sources_path_not_empty CHECK (char_length(file_path) > 0)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_file_sources_enabled
    ON martin_config.file_sources (enabled)
    WHERE enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_file_sources_type
    ON martin_config.file_sources (source_type)
    WHERE enabled = TRUE;

-- ============================================================================
-- Version Management
-- ============================================================================
-- Configuration updates require manual version increments.
-- External tooling should update martin_config.metadata.version when sources change.
--
-- Optional trigger-based automation can be added by operators if desired.

-- ============================================================================
-- Helper Views
-- ============================================================================

-- View: All active sources (both data and file sources)
CREATE OR REPLACE VIEW martin_config.active_sources AS
SELECT
    source_id,
    'data' as source_category,
    source_type,
    enabled,
    updated_at
FROM martin_config.data_sources
UNION ALL
SELECT
    source_id,
    'file' as source_category,
    source_type,
    enabled,
    updated_at
FROM martin_config.file_sources;

-- View: Summary statistics
CREATE OR REPLACE VIEW martin_config.config_summary AS
SELECT
    (SELECT version FROM martin_config.metadata WHERE id = 1) as current_version,
    (SELECT updated_at FROM martin_config.metadata WHERE id = 1) as last_updated,
    (SELECT COUNT(*) FROM martin_config.data_sources WHERE enabled = TRUE) as active_data_sources,
    (SELECT COUNT(*) FROM martin_config.file_sources WHERE enabled = TRUE) as active_file_sources,
    (SELECT COUNT(*) FROM martin_config.data_sources WHERE enabled = FALSE) as disabled_data_sources,
    (SELECT COUNT(*) FROM martin_config.file_sources WHERE enabled = FALSE) as disabled_file_sources;

-- ============================================================================
-- Example Data (Comment out for production)
-- ============================================================================

-- Example: PostgreSQL table source
-- INSERT INTO martin_config.data_sources (
--     source_id, source_type, schema_name, table_or_function_name,
--     geometry_column, srid, id_column, properties
-- ) VALUES (
--     'my_points',
--     'table',
--     'public',
--     'points_table',
--     'geom',
--     4326,
--     'id',
--     '{"name": "My Points", "description": "Example point layer"}'::jsonb
-- );

-- Example: PostgreSQL function source
-- INSERT INTO martin_config.data_sources (
--     source_id, source_type, schema_name, table_or_function_name,
--     properties
-- ) VALUES (
--     'my_function',
--     'function',
--     'public',
--     'get_tiles',
--     '{"name": "My Function", "description": "MVT-returning function"}'::jsonb
-- );

-- Example: MBTiles file source
-- INSERT INTO martin_config.file_sources (
--     source_id, source_type, file_path, properties
-- ) VALUES (
--     'countries',
--     'mbtiles',
--     '/path/to/countries.mbtiles',
--     '{"name": "World Countries", "attribution": "© OpenStreetMap"}'::jsonb
-- );

-- ============================================================================
-- Permissions (Adjust based on your security requirements)
-- ============================================================================

-- Grant read access to Martin application user
-- GRANT USAGE ON SCHEMA martin_config TO martin_user;
-- GRANT SELECT ON ALL TABLES IN SCHEMA martin_config TO martin_user;

-- Grant write access to configuration management tools
-- GRANT INSERT, UPDATE, DELETE ON martin_config.data_sources TO config_admin;
-- GRANT INSERT, UPDATE, DELETE ON martin_config.file_sources TO config_admin;
-- GRANT UPDATE ON martin_config.metadata TO config_admin;
-- GRANT EXECUTE ON FUNCTION martin_config.increment_version() TO config_admin;

-- ============================================================================
-- Schema Validation
-- ============================================================================

-- Query to verify schema installation
DO $$
DECLARE
    table_count INTEGER;
    view_count INTEGER;
    function_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables
    WHERE table_schema = 'martin_config' AND table_type = 'BASE TABLE';

    SELECT COUNT(*) INTO view_count
    FROM information_schema.views
    WHERE table_schema = 'martin_config';

    SELECT COUNT(*) INTO function_count
    FROM information_schema.routines
    WHERE routine_schema = 'martin_config';

    RAISE NOTICE 'Martin configuration schema installed successfully:';
    RAISE NOTICE '  - Tables: %', table_count;
    RAISE NOTICE '  - Views: %', view_count;
    RAISE NOTICE '  - Functions: %', function_count;
    RAISE NOTICE '';
    RAISE NOTICE 'Query config_summary view to see current status:';
    RAISE NOTICE '  SELECT * FROM martin_config.config_summary;';
END $$;
