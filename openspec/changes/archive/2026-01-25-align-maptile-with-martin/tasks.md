## 1. Implementation
- [x] 1.1 Update `maptile/idl/maptile.thrift` (composite sources, if_none_match, accept_encoding, not_modified)
- [x] 1.2 Regenerate Volo bindings for the updated Thrift IDL by running `cargo build -p maptile` (build.rs invokes volo_build)
- [x] 1.3 Verify generated bindings compile via `maptile/src/lib.rs` include of `OUT_DIR/volo_gen.rs`
- [x] 1.4 Implement composite source parsing and merging for `get_tile`
- [x] 1.5 Compute and return ETags using `get_tile_with_etag`
- [x] 1.6 Implement conditional request behavior (`if_none_match` -> `not_modified`)
- [x] 1.7 Implement encoding negotiation and recompression logic
- [x] 1.8 Add/update tests for composite, ETag, encoding, and not_modified cases
- [x] 1.9 Update example client and RPC docs
