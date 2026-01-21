namespace rs maptile.gen

/// Tile coordinates (z/x/y)
struct TileCoord {
    1: required i16 z,
    2: required i64 x,
    3: required i64 y,
}

/// Request for a single tile
struct TileRequest {
    1: required string source_id,
    2: required TileCoord coord,
    3: optional map<string, string> query_params,
}

/// Response containing tile data
struct TileResponse {
    1: required binary data,
    2: required string content_type,
    3: optional string content_encoding,
    4: optional string etag,
}

/// Information about a tile source
struct TileInfo {
    1: required string source_id,
    2: required string name,
    3: optional i32 min_zoom,
    4: optional i32 max_zoom,
    5: optional string bounds,
    6: optional string description,
    7: optional string attribution,
}

/// Error returned by tile operations
exception TileError {
    1: required i32 code,
    2: required string message,
}

/// Maptile RPC service for vector tile serving
service MaptileService {
    /// Get a single tile by source ID and coordinates
    TileResponse get_tile(1: TileRequest request) throws (1: TileError error),
    
    /// List all available tile sources
    list<TileInfo> list_sources() throws (1: TileError error),
    
    /// Get information about a specific tile source
    TileInfo get_source_info(1: string source_id) throws (1: TileError error),
}
