# Project Context

## Purpose
Martin is a blazing fast tile server written in Rust that generates and serves vector tiles on the fly from multiple data sources. It enables high-performance map tile delivery from PostgreSQL/PostGIS databases, MBTiles files, PMTiles archives, and Cloud Optimized GeoTIFF (COG) files. Martin is optimized for speed and heavy traffic, designed for production use in web mapping applications using MapLibre, Leaflet, OpenLayers, and other mapping libraries.

## Tech Stack
- **Language**: Rust (edition 2024, MSRV 1.88)
- **HTTP Framework**: Actix-Web 4 (async/await, Tokio runtime)
- **Database**: PostgreSQL 18+ with PostGIS 3.6+ extension
- **Connection Pooling**: deadpool-postgres
- **SQLite Access**: sqlx for MBTiles
- **Cloud Storage**: object_store (S3, Azure, GCP)
- **Caching**: moka (in-memory LRU cache)
- **Frontend**: React 19.2 + TypeScript 5.9 + Vite 7.3 + Tailwind CSS 4.1
- **Tile Formats**: MVT (Mapbox Vector Tiles), PMTiles, MBTiles
- **Build Tool**: Just (task runner)
- **Testing**: cargo test, vitest (frontend), testcontainers (integration)

## Project Conventions

### Code Style
- **Rust**: Follow Rust 2024 edition idioms, use `cargo fmt` (nightly preferred for import grouping)
  - Lint with `cargo clippy --workspace --all-targets`
  - Pedantic clippy lints enabled (see Cargo.toml `workspace.lints.clippy`)
  - No `unsafe` code except in `mbtiles` crate
  - Use type system aggressively (newtypes, builders, state encoding)
- **TypeScript/React**: Biome for linting and formatting
  - Run `just biomejs-martin-ui` for frontend code quality
  - Use `just type-check` for TypeScript validation
- **Naming**:
  - Crates: kebab-case (`martin-core`, `martin-tile-utils`)
  - Functions/variables: snake_case
  - Types/structs: PascalCase
  - Constants: SCREAMING_SNAKE_CASE
- **Error Handling**: Use `thiserror` for domain errors, `anyhow` for application errors
- **Async**: All I/O operations must be async (database, file, network)

### Architecture Patterns
- **Workspace Organization**: 4 crates with clear separation of concerns
  - `martin/` - HTTP server, CLI, configuration (depends on martin-core, mbtiles)
  - `martin-core/` - Reusable library with tile source abstractions and implementations
  - `mbtiles/` - Standalone MBTiles library and CLI tool
  - `martin-tile-utils/` - Shared low-level tile utilities
- **Dependency Direction**: martin → martin-core → martin-tile-utils; mbtiles is standalone
- **Source Discovery**: Auto-discovery of PostgreSQL tables/functions and file-based sources
- **Tile Sources**: Implement `Source` trait in martin-core for new tile source types
- **Configuration Precedence**: CLI flags > environment variables > YAML config file
- **Connection Pooling**: Reuse database connections via deadpool-postgres
- **Caching**: LRU cache for tiles and resources (default 512MB, configurable)
- **Feature Flags**: Use Cargo features for optional functionality (postgres, mbtiles, pmtiles, fonts, sprites, styles, webui, metrics, lambda)

### Testing Strategy
- **Scope Isolation (CRITICAL)**: Frontend changes MUST NOT trigger Rust builds
  - Frontend-only: Use npm commands in `martin/martin-ui/`, run `just test-frontend`
  - Backend/full-stack: Requires `just start` + `cargo build --workspace` + `just test`
- **Test Database**: PostgreSQL 18 + PostGIS 3.6 in Docker via `just start`
  - Connection: `postgres://postgres:postgres@localhost:5411/db`
  - Initialized with `tests/fixtures/initdb.sh`
- **Unit Tests**: `cargo test --workspace` (per-package testing supported)
- **Doc Tests**: `cargo test --doc`
- **Integration Tests**: Bash scripts in `tests/` directory
  - Expected output in `tests/expected/`, actual in `tests/output/`
  - Use `just bless-int` to update expected output after verification
- **Snapshot Testing**: Use `insta` crate, review with `cargo insta review` or `just bless-insta-*`
- **Frontend Tests**: Vitest for React components, run via `just test-frontend`
- **CI Parity**: Run `CI=true just ci-test` to match GitHub Actions checks
- **Coverage**: `just coverage` generates LLVM coverage reports
- **Quality Bar**: Tests are part of the feature - missing tests means incomplete change

### Git Workflow
- **Fork-Based Contributions**: ALL contributors (including core maintainers) must fork and create PRs from forks
- **Remote Setup**:
  - `upstream` → `maplibre/martin` (main repo)
  - `origin` → your fork
- **Branch Protection**: Cannot create PRs from branches in main repo
- **Main Branch**: `main` tracks `upstream/main`
- **Pre-commit Hooks**: `.pre-commit-config.yaml` runs automated checks
  - If hooks auto-modify files, include those in the commit
  - If commit FAILS, fix issue and create NEW commit (no `--amend` unless commit not pushed)
- **Never Cancel Builds**: NEVER cancel `cargo build`, `cargo test`, `just check`, or `just ci-test` once started
- **Command Timeouts**:
  - `cargo build --workspace`: 20m
  - `just test`: 30m
  - `just ci-test`: 45m

## Domain Context
- **Vector Tiles**: Martin serves MVT (Mapbox Vector Tiles) format, a compact binary format for map data
- **Tile Coordinates**: Uses Z/X/Y addressing (zoom level, x coordinate, y coordinate)
- **PostGIS**: Spatial extension for PostgreSQL, Martin auto-discovers geometry columns and MVT-returning functions
- **MBTiles**: SQLite-based archive format for storing pre-generated tiles (static datasets)
- **PMTiles**: Cloud-native single-file archive optimized for HTTP range requests (S3-friendly)
- **COG (Cloud Optimized GeoTIFF)**: Raster format with tiled structure for efficient remote access
- **TileJSON**: JSON metadata format describing tile sources (bounds, zoom levels, attribution)
- **Sprites**: Collections of map icons packaged as PNG sprite sheets with JSON index
- **Font Glyphs**: Map label fonts encoded as PBF (Protocol Buffer) format
- **MapLibre Styles**: JSON-based style specifications for rendering vector tiles
- **Tile Caching**: In-memory LRU cache to avoid regenerating frequently requested tiles
- **Auto-Discovery**: Martin scans PostgreSQL `geometry_columns` and `pg_proc` to find tile sources

## Important Constraints
- **Performance First**: Martin is optimized for high-throughput tile serving under heavy load
- **Rust Safety**: Memory safety and thread safety enforced by Rust type system
- **Async Required**: All I/O operations must be async to handle thousands of concurrent connections
- **Scope Isolation**: Frontend and backend development paths must remain strictly separated
- **Production Grade**: All changes must be production-ready with proper error handling and tests
- **Breaking Changes**: Must be clearly marked and require careful consideration
- **LLM Quality Bar**: LLM-assisted contributions must exceed human-only quality standards
- **Type System Usage**: Prefer compile-time guarantees over runtime checks
- **Connection Management**: Must use connection pooling, never create connections per request
- **Cache Coherency**: Tile cache must be properly invalidated when source data changes
- **Resource Generation**: Sprites, fonts, and styles generated on-the-fly (no pre-processing)

## External Dependencies
- **PostgreSQL/PostGIS**: Primary database for dynamic tile generation
  - Version: PostgreSQL 14+ with PostGIS 3.0+
  - Production: PostgreSQL 18+ with PostGIS 3.6+ recommended
  - SSL/TLS support for secure connections
  - Read-only database user recommended for security
- **Docker**: Required for local development (test database, docker-compose)
- **Cloud Storage Providers**: Optional for remote tile sources
  - AWS S3 (via object_store crate with AWS credentials)
  - Azure Blob Storage (via object_store crate with Azure credentials)
  - Google Cloud Storage (via object_store crate with GCP credentials)
- **HTTP Clients**: MapLibre GL JS, Leaflet, OpenLayers, Deck.gl (client-side rendering)
- **Reverse Proxy**: NGINX or Apache for production (SSL termination, caching, load balancing)
- **Monitoring**: Prometheus (metrics endpoint at `/metrics`)
- **CI/CD**: GitHub Actions for automated testing and releases
- **Package Registries**: crates.io (Rust), npm (frontend)
- **Just**: Task runner for development commands (install via `cargo install just --locked`)

## Key Architectural Decisions
- **Actix-Web Choice**: High-performance async HTTP framework with mature middleware ecosystem
- **Workspace Structure**: Clear separation between server (martin), core library (martin-core), and tools (mbtiles)
- **Feature Flags**: Optional functionality via Cargo features for minimal deployments
- **Async/Await**: Tokio runtime for efficient concurrent request handling
- **Connection Pooling**: deadpool-postgres for database connection reuse
- **In-Memory Caching**: moka LRU cache for tile performance (avoids database/file I/O)
- **Automatic Discovery**: Zero-config operation for common PostgreSQL/PostGIS setups
- **Multi-Format Support**: Support for PostgreSQL, MBTiles, PMTiles, COG allows operators to choose best storage model
- **On-Demand Resources**: Sprites, fonts, styles generated dynamically (simpler deployments)
- **Modular Configuration**: CLI, environment variables, and config files for flexibility
