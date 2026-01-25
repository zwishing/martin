## Context
Maptile RPC is a Thrift-based tile service built on martin-core, but it currently lacks several behaviors that exist in the Martin HTTP tile endpoint. The gap prevents consistent caching and interoperability across clients.

## Goals / Non-Goals
- Goals: parity with Martin for composite sources, ETag generation, conditional requests, and encoding negotiation.
- Goals: retain existing DB-driven config and hot-reload behavior.
- Non-Goals: add non-Postgres sources (MBTiles/PMTiles/COG) to maptile.
- Non-Goals: redesign the config schema in `martin_config`.

## Decisions
- Decision: Extend the Thrift IDL with optional fields to support parity.
  - TileRequest: add optional `source_ids` (comma-separated, same as HTTP), optional `accept_encoding`, optional `if_none_match`.
  - TileResponse: add optional `not_modified` boolean and always include `etag` when data is computed.
  - Rationale: keeps wire changes minimal while mirroring HTTP behavior.
- Decision: Use martin-core ETag generation (`Tile::new_hash_etag`) for consistency.
- Decision: Reuse Martin encoding negotiation rules from `martin/src/srv/tiles/content.rs`.
- Decision: Composite merging follows the HTTP rules (MVT only; uncompressed or gzip). ETags are concatenated.

## Risks / Trade-offs
- Changing the Thrift schema requires regenerating Volo bindings and coordinating clients.
- ETag generation uses native-endian hashing in martin-core; cross-architecture consistency is unchanged.

## Migration Plan
1. Update Thrift IDL and regenerate bindings.
2. Implement server changes behind new optional fields, keeping old request shape working.
3. Update example client and documentation.

## Open Questions
- Should `not_modified` responses still return `content_type`/`content_encoding`, or only `etag`?

## References
- RFC 7232 conditional requests: https://datatracker.ietf.org/doc/html/rfc7232
- Mapbox Vector Tile specification: https://mapbox.github.io/vector-tile-spec/
