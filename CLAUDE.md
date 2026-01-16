# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Martin is a blazing fast tile server written in Rust that generates and serves vector tiles on the fly from multiple data sources:
- **PostgreSQL/PostGIS** tables and functions (dynamic tiles from live data)
- **MBTiles** files (pre-generated tile archives)
- **PMTiles** files (single-file, cloud-native archives with HTTP range request support)
- **Cloud Optimized GeoTIFF (COG)** files

Martin also generates sprites, font glyphs, and serves MapLibre styles on demand.

## Repository Structure

This is a Rust workspace with 4 main crates:

- **`martin/`** - Main tile server binary (Actix-Web HTTP server, CLI, configuration)
  - `src/bin/martin.rs` - Server entry point
  - `src/bin/martin-cp.rs` - Bulk tile copying tool
  - `src/srv/` - HTTP service handlers (tiles, fonts, sprites, styles)
  - `src/config/` - Configuration management (CLI args, env vars, config files)
  - `martin-ui/` - React/TypeScript web UI for tile inspection
- **`martin-core/`** - Core library (tile source implementations, PostgreSQL, MBTiles, PMTiles, COG, resource generation)
  - `src/tiles/` - Tile source implementations (postgres, mbtiles, pmtiles, cog)
  - `src/resources/` - Resource generation (sprites, fonts, styles)
- **`mbtiles/`** - MBTiles library and CLI tool (SQLite-based tile storage, validation, diff/patch operations)
- **`martin-tile-utils/`** - Low-level tile utilities (coordinate conversions, encoding/decoding)

## Common Development Commands

### Initial Setup
```bash
# Install Just task runner
cargo install just --locked

# Validate all required tools are installed
just validate-tools

# Start test PostgreSQL database (Docker required)
just start
```

### Development Workflow
```bash
# Start Martin server with test data
just run                    # Default: serves with WebUI enabled
just run --config path.yaml # Custom config

# Run specific binary
cargo run --bin martin -- [args]
cargo run --bin martin-cp -- [args]
cargo run --bin mbtiles -- [args]
```

### Testing

**CRITICAL**: Martin has strict scope isolation rules. Determine the scope before running tests:

#### Frontend-Only Changes (`martin/martin-ui/`)
```bash
cd martin/martin-ui
npm clean-install --no-fund
just test-frontend          # Run frontend tests
just type-check            # TypeScript checking
just biomejs-martin-ui     # Lint and format
```

**NEVER** run `cargo build`, `cargo test`, or `just start` for frontend-only changes.

#### Backend or Full-Stack Changes
```bash
# Build everything
cargo build --workspace

# Run all tests (requires test database)
just test                  # All tests: cargo test, doc tests, frontend, integration

# Run specific test types
just test-cargo            # Rust unit tests only
just test-cargo -- --test integration_test  # Single integration test
just test-doc              # Doc tests
just test-int              # Integration tests (bash scripts in tests/)

# Run tests as CI does
just ci-test               # Includes fmt, clippy, doc checks, all tests
```

#### Single Package Testing
```bash
cargo test --package martin-tile-utils
cargo test --package mbtiles --no-default-features
cargo test --package mbtiles
cargo test --package martin
cargo test --package martin-core
```

### Code Quality
```bash
just fmt                   # Format Rust code (uses nightly if available for import sorting)
just clippy                # Lint Rust code
just check                 # Quick compile check across all feature combinations
```

### Database Management
```bash
just start                 # Start test database (PostgreSQL 18 + PostGIS 3.6)
just restart               # Stop and restart test database
just stop                  # Stop test database
just psql                  # Connect to test database with psql
just pg_dump               # Dump test database schema

# Alternative databases
just start-ssl             # SSL-enabled database
just start-ssl-cert        # SSL with client certificates
just start-legacy          # Older PostgreSQL version
```

### Working with MBTiles
```bash
# When modifying SQL queries in mbtiles crate
just prepare-sqlite        # Update SQLx prepared query cache
```

### Integration Tests
```bash
# Bless tests (update expected output)
just bless                 # Bless all tests (unit + integration)
just bless-int             # Bless integration tests only
just bless-insta-martin    # Bless insta snapshots for martin binary
just bless-insta-mbtiles   # Bless insta snapshots for mbtiles
just bless-frontend        # Update frontend test snapshots
```

### Documentation
```bash
just book                  # Build and serve mdbook documentation (opens browser)
just docs                  # Build Rust API documentation
```

### Other Useful Commands
```bash
just help                  # Show common commands
just --list                # Show all available commands
just env-info              # Print environment information
just validate-tools        # Check that all required dev tools are installed
```

## Architecture Patterns

### Crate Organization
- **martin** depends on **martin-core** and **mbtiles**
- **martin-core** is the reusable library that can be embedded in other Rust projects
- **mbtiles** is standalone and can be used independently
- **martin-tile-utils** provides shared low-level utilities

### Key Technologies
- **Actix-Web** for HTTP server (async request handling, middleware)
- **deadpool-postgres** for PostgreSQL connection pooling
- **sqlx** for SQLite/MBTiles access
- **pmtiles** crate for PMTiles support
- **object_store** for S3/Azure/GCP cloud storage access
- **moka** for in-memory LRU tile caching
- **spreet** for sprite generation from SVGs
- **pbf_font_tools** for font glyph generation

### Async/Await Throughout
- All I/O operations are async (database queries, file reads, HTTP requests)
- Uses Tokio runtime
- Enables handling thousands of concurrent connections efficiently

### Configuration Sources (in order of precedence)
1. CLI flags (highest priority)
2. Environment variables
3. Configuration file (YAML)

### Database-Driven Configuration
- Enable with `config_source: database`
- Polling interval: `config_refresh_interval_seconds` (default 60, minimum 10)
- Admin reload: `admin_reload_enabled: true` to allow `POST /admin/config/reload` (default off)
- Optional `config_database` connection string for config storage (falls back to `postgres` when unset)

## Development Guidelines

### Scope Isolation (CRITICAL)

**Frontend-only scope** (`martin/martin-ui/`):
- Use: `npm` commands, `just test-frontend`, `just type-check`, `just biomejs-martin-ui`
- **NEVER** use: `cargo build`, `cargo run`, `just start`, any Rust compilation

**Backend or full-stack scope**:
- Requires full bootstrap: `just start` → `cargo build --workspace`
- Run all appropriate tests before submitting

### Quality Standards for LLM-Assisted Changes
- LLM-assisted contributions must aim for **higher standards** than human-only changes
- Use LLMs as a **quality multiplier, not a speed multiplier**
- Invest saved time into: additional edge case tests, clearer structure, better error messages

### Engineering Principles
- **Correctness over convenience**: Model the full error space, handle edge cases explicitly
- **User experience matters**: Errors must be actionable and specific, use structured contextual messages
- **Pragmatic incrementalism**: Prefer specific composable logic over over-generic abstractions
- **Production-grade Rust**: Use type system aggressively (newtypes, builders, state encoding), avoid shared mutable state
- **Tests are part of the feature**: Missing tests means the change is incomplete

### Command Timeouts
Be aware of expected command durations:
- `cargo build --workspace`: up to 20 minutes
- `just check`: up to 20 minutes
- `just test`: up to 30 minutes
- `just ci-test`: up to 45 minutes
- Frontend operations: typically under 5 minutes

**NEVER cancel** `cargo build`, `cargo test`, `just check`, or `just ci-test` once started.

### Pre-commit Hooks
The project uses `.pre-commit-config.yaml` for automated checks. When committing:
- If pre-commit hooks auto-modify files, include those modifications
- If commit **FAILS**, fix the issue and create a **NEW commit** (do not use `--amend` unless the commit was created by you in this conversation and hasn't been pushed)

## Testing Environment

### PostgreSQL Test Database
- Runs in Docker via `just start`
- PostgreSQL 18 with PostGIS 3.6
- Connection string: `postgres://postgres:postgres@localhost:5411/db`
- Controlled via `DATABASE_URL` environment variable
- Initialized with test fixtures from `tests/fixtures/initdb.sh`

### Integration Tests
- Located in `tests/` directory
- Use `tests/test.sh` bash script
- Expected output stored in `tests/expected/`
- Actual output generated in `tests/output/`
- Use `just bless-int` to update expected output after verifying changes

### Snapshot Testing
- Uses `insta` crate for snapshot testing
- Snapshots stored in `*/tests/snapshots/` directories
- Review with `cargo insta review` or use `just bless-insta-*` commands

## Feature Flags

Key Cargo features:
- `postgres` - PostgreSQL/PostGIS support
- `mbtiles` - MBTiles support
- `pmtiles` - PMTiles support
- `fonts` - Font glyph generation
- `sprites` - Sprite generation
- `styles` - MapLibre style serving
- `webui` - Embedded web UI
- `metrics` - Prometheus metrics endpoint
- `lambda` - AWS Lambda support
- `unstable-cog` - Cloud Optimized GeoTIFF support (unstable)
- `unstable-rendering` - Server-side rendering (Linux only, unstable)

Default features include most functionality. Use `--no-default-features` to build minimal version.

## Git Workflow

This project requires all contributors (including core maintainers) to:
1. Fork the repository under their own account
2. Configure local repo with two remotes:
   - `upstream` → `maplibre/martin` (main repo)
   - `origin` → your fork
3. Always create PRs from your fork, never from branches in the main repo

## Common Troubleshooting

- **Frontend dependency failures**: Remove `martin/martin-ui/node_modules` and reinstall
- **Integration test DB failures**: Run `just restart` to reset database
- **CI failures locally**: Use `CI=true just ci-test` to run with same flags as CI
- **SQLX query errors**: Run `just prepare-sqlite` after modifying SQL queries
- **Git repo not clean after tests**: Check `.gitignore` for missing patterns

## Performance Notes

- Release builds (`--release`) are **significantly faster** than debug builds
- Martin is optimized for high-throughput tile serving under heavy traffic
- Tile caching (in-memory LRU, configurable size) dramatically improves performance for repeated requests
- PostgreSQL connection pooling avoids per-request connection overhead

---

<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->
