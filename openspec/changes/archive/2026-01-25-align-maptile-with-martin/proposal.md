# Change: Align maptile RPC with Martin tile semantics

## Why
Maptile RPC currently diverges from Martin HTTP in composite source support, ETag-based caching, and encoding negotiation. This makes client behavior inconsistent and prevents shared caching strategies across HTTP and RPC clients.

## What Changes
- Extend the RPC IDL to support composite sources, conditional requests, and encoding negotiation.
- Generate and return ETags using the same hashing as martin-core.
- Align empty-tile handling and error codes with Martin semantics.
- Add tests covering composite sources, ETag behavior, and encoding negotiation.

## Impact
- Affected specs: `maptile-rpc`
- Affected code: `maptile/idl/maptile.thrift`, `maptile/src/server/service.rs`, `maptile/src/config/types.rs`, `maptile/src/server/service.rs` tests
