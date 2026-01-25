//! MaptileService implementation

use std::collections::HashMap;
use std::sync::Arc;

use deadpool_postgres::Pool;
use futures::future::try_join_all;
use log::{error, info};
use martin_core::tiles::{BoxedSource, Tile, UrlQuery};
use martin_tile_utils::{
    Encoding, Format, TileCoord, decode_brotli, decode_gzip, encode_brotli, encode_gzip,
};
use tokio::sync::RwLock;
use volo_thrift::{MaybeException, ServerError};

use crate::config::{ConfigMetadata, MaptileConfig, load_sources_from_database};
use crate::handler::smart_routing::{has_filter_params, resolve_source_id};
use crate::volo_gen::maptile::r#gen::{
    MaptileService, MaptileServiceGetSourceInfoException, MaptileServiceGetTileException,
    MaptileServiceListSourcesException, TileCoord as ThriftTileCoord, TileError, TileInfo,
    TileRequest, TileResponse,
};

const MAX_ZOOM: i16 = 30;
const MAX_SOURCE_IDS: usize = 20;
const MVT_CONTENT_TYPE: &str = "application/vnd.mapbox-vector-tile";
const DEFAULT_PREFERRED_ENCODING: Encoding = Encoding::Gzip;

/// Compare If-None-Match header value against an ETag.
/// Handles both quoted and unquoted ETag formats.
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let etag_quoted = format!("\"{etag}\"");
    if_none_match == etag || if_none_match == etag_quoted
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcceptEncodingToken {
    Gzip,
    Brotli,
    Identity,
    Any,
    Other,
}

#[derive(Clone, Copy, Debug)]
struct AcceptEncodingItem {
    token: AcceptEncodingToken,
    quality: f32,
}

fn parse_accept_encoding(accept: &str) -> Vec<AcceptEncodingItem> {
    accept
        .split(',')
        .filter_map(|part| {
            let mut iter = part.split(';');
            let encoding = iter.next()?.trim().to_ascii_lowercase();
            if encoding.is_empty() {
                return None;
            }

            let mut quality = 1.0_f32;
            for param in iter {
                let param = param.trim().to_ascii_lowercase();
                if let Some(value) = param.strip_prefix("q=") {
                    quality = value.parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
                }
            }

            let token = match encoding.as_str() {
                "gzip" => AcceptEncodingToken::Gzip,
                "br" => AcceptEncodingToken::Brotli,
                "identity" => AcceptEncodingToken::Identity,
                "*" => AcceptEncodingToken::Any,
                _ => AcceptEncodingToken::Other,
            };

            Some(AcceptEncodingItem { token, quality })
        })
        .collect()
}

fn last_quality(items: &[AcceptEncodingItem], token: AcceptEncodingToken) -> Option<f32> {
    items
        .iter()
        .rev()
        .find(|item| item.token == token)
        .map(|item| item.quality)
}

fn identity_acceptable(items: &[AcceptEncodingItem]) -> bool {
    let identity_quality = last_quality(items, AcceptEncodingToken::Identity);
    let any_quality = last_quality(items, AcceptEncodingToken::Any);

    match (identity_quality, any_quality) {
        (Some(q), _) => q > 0.0,
        (None, Some(q)) => q > 0.0,
        (None, None) => true,
    }
}

fn accepts_explicit_encoding(accept: &str, encoding: Encoding) -> bool {
    let items = parse_accept_encoding(accept);
    let token = match encoding {
        Encoding::Gzip => AcceptEncodingToken::Gzip,
        Encoding::Brotli => AcceptEncodingToken::Brotli,
        Encoding::Uncompressed => AcceptEncodingToken::Identity,
        _ => AcceptEncodingToken::Other,
    };

    last_quality(&items, token).is_some_and(|q| q > 0.0)
}

fn decide_target_encoding(accept: &str) -> Result<Option<Encoding>, TileError> {
    let items = parse_accept_encoding(accept);
    if items.is_empty() {
        return Ok(Some(Encoding::Uncompressed));
    }

    let mut q_gzip = last_quality(&items, AcceptEncodingToken::Gzip);
    let mut q_brotli = last_quality(&items, AcceptEncodingToken::Brotli);
    let q_any = last_quality(&items, AcceptEncodingToken::Any);
    let identity_ok = identity_acceptable(&items);

    if q_gzip.is_none() {
        q_gzip = q_any;
    }
    if q_brotli.is_none() {
        q_brotli = q_any;
    }

    match (q_gzip, q_brotli) {
        (Some(q_gzip), Some(q_brotli)) if q_gzip == q_brotli => {
            if q_gzip > 0.0 {
                return Ok(Some(DEFAULT_PREFERRED_ENCODING));
            }
            if identity_ok {
                return Ok(None);
            }
            return Err(TileError {
                code: 406,
                message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
            });
        }
        (Some(q_gzip), Some(q_brotli)) if q_brotli > q_gzip => {
            if q_brotli > 0.0 {
                return Ok(Some(Encoding::Brotli));
            }
            if identity_ok {
                return Ok(None);
            }
            return Err(TileError {
                code: 406,
                message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
            });
        }
        (Some(q_gzip), Some(_)) => {
            if q_gzip > 0.0 {
                return Ok(Some(Encoding::Gzip));
            }
            if identity_ok {
                return Ok(None);
            }
            return Err(TileError {
                code: 406,
                message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
            });
        }
        (Some(q_gzip), None) => {
            if q_gzip > 0.0 {
                return Ok(Some(Encoding::Gzip));
            }
            if identity_ok {
                return Ok(None);
            }
            return Err(TileError {
                code: 406,
                message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
            });
        }
        (None, Some(q_brotli)) => {
            if q_brotli > 0.0 {
                return Ok(Some(Encoding::Brotli));
            }
            if identity_ok {
                return Ok(None);
            }
            return Err(TileError {
                code: 406,
                message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
            });
        }
        _ => {}
    }

    if identity_ok {
        Ok(Some(Encoding::Uncompressed))
    } else {
        Err(TileError {
            code: 406,
            message: "No acceptable encoding found. Supported: gzip, br, identity".into(),
        })
    }
}

fn decode_tile_data(data: Vec<u8>, encoding: Encoding) -> Result<Vec<u8>, TileError> {
    match encoding {
        Encoding::Uncompressed => Ok(data),
        Encoding::Gzip => decode_gzip(&data).map_err(|e| TileError {
            code: 500,
            message: format!("Failed to decode gzip: {e}").into(),
        }),
        Encoding::Brotli => decode_brotli(&data).map_err(|e| TileError {
            code: 500,
            message: format!("Failed to decode brotli: {e}").into(),
        }),
        _ => Ok(data),
    }
}

fn encode_tile_data(data: Vec<u8>, encoding: Encoding) -> Result<Vec<u8>, TileError> {
    match encoding {
        Encoding::Uncompressed => Ok(data),
        Encoding::Gzip => encode_gzip(&data).map_err(|e| TileError {
            code: 500,
            message: format!("Failed to encode gzip: {e}").into(),
        }),
        Encoding::Brotli => encode_brotli(&data).map_err(|e| TileError {
            code: 500,
            message: format!("Failed to encode brotli: {e}").into(),
        }),
        _ => Ok(data),
    }
}

/// Implementation of the MaptileService trait
pub struct MaptileServiceImpl {
    sources: Vec<BoxedSource>,
    source_map: HashMap<String, usize>,
    metadata: Option<ConfigMetadata>,
    #[allow(dead_code)]
    config: MaptileConfig,
}

impl MaptileServiceImpl {
    /// Create a new MaptileServiceImpl
    ///
    /// # Errors
    ///
    /// Returns an error if sources cannot be loaded from the configuration database.
    pub async fn new(config: MaptileConfig, pool: Pool) -> anyhow::Result<Self> {
        let loaded = load_sources_from_database(&config, &pool).await?;

        let source_map = loaded
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.get_id().to_string(), i))
            .collect();

        info!(
            "Initialized MaptileService with {} sources",
            loaded.sources.len()
        );

        Ok(Self {
            sources: loaded.sources,
            source_map,
            metadata: Some(loaded.metadata),
            config,
        })
    }

    /// Update sources (used for hot reload)
    pub fn update_sources(&mut self, sources: Vec<BoxedSource>) {
        self.source_map = sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.get_id().to_string(), i))
            .collect();
        self.sources = sources;
        info!("Updated sources, now have {} sources", self.sources.len());
    }

    /// Update metadata (used for hot reload)
    pub fn update_metadata(&mut self, metadata: ConfigMetadata) {
        self.metadata = Some(metadata);
    }

    /// Get a source by ID
    fn get_source(&self, source_id: &str) -> Option<&BoxedSource> {
        self.source_map.get(source_id).map(|&i| &self.sources[i])
    }

    fn validate_tile_coord(coord: &ThriftTileCoord) -> Result<TileCoord, TileError> {
        if coord.z < 0 || coord.z > MAX_ZOOM {
            return Err(TileError {
                code: 400,
                message: format!(
                    "Invalid zoom {zoom}, expected 0..={MAX_ZOOM}",
                    zoom = coord.z
                )
                .into(),
            });
        }

        if coord.x < 0 || coord.y < 0 {
            return Err(TileError {
                code: 400,
                message: "Tile coordinates must be non-negative".into(),
            });
        }

        // Safe conversion: coord.z is guaranteed to be in [0, MAX_ZOOM]
        let zoom = u8::try_from(coord.z).map_err(|_| TileError {
            code: 400,
            message: format!("Zoom level {zoom} out of valid range", zoom = coord.z).into(),
        })?;

        // Safe checked arithmetic to prevent overflow
        let max_index = u32::checked_pow(2, u32::from(zoom))
            .and_then(|v| v.checked_sub(1))
            .ok_or_else(|| TileError {
                code: 400,
                message: format!("Zoom level {zoom} too large").into(),
            })?;

        // Safe conversion: x and y are non-negative
        let x = u32::try_from(coord.x).map_err(|_| TileError {
            code: 400,
            message: format!("X coordinate {x} out of valid range", x = coord.x).into(),
        })?;
        let y = u32::try_from(coord.y).map_err(|_| TileError {
            code: 400,
            message: format!("Y coordinate {y} out of valid range", y = coord.y).into(),
        })?;

        if x > max_index || y > max_index {
            return Err(TileError {
                code: 400,
                message: format!(
                    "Tile coordinates out of range for z={zoom}: x/y must be 0..={max_index}"
                )
                .into(),
            });
        }

        Ok(TileCoord { z: zoom, x, y })
    }
}

type GetTileResult =
    Result<MaybeException<TileResponse, MaptileServiceGetTileException>, ServerError>;
type ListSourcesResult =
    Result<MaybeException<Vec<TileInfo>, MaptileServiceListSourcesException>, ServerError>;
type GetSourceInfoResult =
    Result<MaybeException<TileInfo, MaptileServiceGetSourceInfoException>, ServerError>;

impl MaptileService for Arc<RwLock<MaptileServiceImpl>> {
    /// Get a single tile by source ID and coordinates
    async fn get_tile(&self, request: TileRequest) -> GetTileResult {
        let (sources, use_url_query) = {
            let service = self.read().await;

            // Convert query_params to HashMap for smart routing
            let query_params_map: HashMap<String, String> = request
                .query_params
                .as_ref()
                .map(|params| {
                    params
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            // Get list of available source IDs for smart routing
            let available_sources: Vec<String> =
                service.source_map.keys().map(|k| k.to_string()).collect();

            // Resolve source IDs: prefer source_ids (comma-separated), fallback to source_id
            let id_string = if let Some(ids) = &request.source_ids {
                ids.as_str()
            } else {
                request.source_id.as_str()
            };

            let mut sources = Vec::new();
            for id in id_string.split(',') {
                let id = id.trim();
                if id.is_empty() {
                    continue;
                }
                if sources.len() >= MAX_SOURCE_IDS {
                    return Ok(MaybeException::Exception(
                        MaptileServiceGetTileException::Error(TileError {
                            code: 400,
                            message: format!("Too many sources: maximum {MAX_SOURCE_IDS} allowed")
                                .into(),
                        }),
                    ));
                }

                // 🎯 SMART ROUTING: Automatically select filtered variant if query params exist
                let resolved_id = resolve_source_id(id, &query_params_map, &available_sources);

                if let Some(source) = service.get_source(&resolved_id) {
                    info!(
                        "Resolved source '{}' → '{}' (has_filters: {})",
                        id,
                        resolved_id,
                        has_filter_params(&query_params_map)
                    );
                    sources.push(source.clone());
                } else {
                    return Ok(MaybeException::Exception(
                        MaptileServiceGetTileException::Error(TileError {
                            code: 404,
                            message: format!("Source '{id}' not found").into(),
                        }),
                    ));
                }
            }

            if sources.is_empty() {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(TileError {
                        code: 404,
                        message: "No sources specified".into(),
                    }),
                ));
            }

            // Validate compatibility (all sources must match format and encoding)
            let first_info = sources[0].get_tile_info();
            for source in &sources[1..] {
                let info = source.get_tile_info();
                if info.format != first_info.format || info.encoding != first_info.encoding {
                    return Ok(MaybeException::Exception(
                        MaptileServiceGetTileException::Error(TileError {
                            code: 400,
                            message: format!(
                                "Cannot merge sources with different formats/encodings: {} vs {}",
                                first_info, info
                            )
                            .into(),
                        }),
                    ));
                }
            }

            // Check if URL query params are supported (Function sources typically do)
            let use_url_query = sources.iter().any(|s| s.support_url_query());

            (sources, use_url_query)
        };

        let coord = match MaptileServiceImpl::validate_tile_coord(&request.coord) {
            Ok(coord) => coord,
            Err(err) => {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(err),
                ));
            }
        };

        let any_valid_zoom = sources.iter().any(|s| s.is_valid_zoom(coord.z));
        if !any_valid_zoom {
            return Ok(MaybeException::Ok(TileResponse {
                data: pilota::Bytes::new(),
                content_type: MVT_CONTENT_TYPE.into(),
                content_encoding: None,
                etag: None,
                not_modified: None,
            }));
        }

        let url_query: Option<UrlQuery> = if use_url_query {
            request.query_params.map(|params| {
                params
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
        } else {
            None
        };

        let final_info = sources[0].get_tile_info();

        // Fetch tiles in parallel to match HTTP semantics
        let tiles = match try_join_all(sources.iter().map(|s| async {
            s.get_tile_with_etag(coord, url_query.as_ref())
                .await
                .map_err(|e| {
                    error!("Failed to get tile from source '{}': {e}", s.get_id());
                    TileError {
                        code: 500,
                        message: "Internal error while fetching tile".into(),
                    }
                })
        }))
        .await
        {
            Ok(tiles) => tiles,
            Err(err) => {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(err),
                ));
            }
        };

        // Filter out empty tiles
        let mut non_empty_tiles: Vec<Tile> =
            tiles.into_iter().filter(|t| !t.data.is_empty()).collect();

        if non_empty_tiles.is_empty() {
            let tile_info = sources[0].get_tile_info();
            let empty_tile = Tile::new_hash_etag(Vec::new(), tile_info);

            // Check If-None-Match for empty tile
            if let Some(if_none_match) = &request.if_none_match {
                if etag_matches(if_none_match, &empty_tile.etag) {
                    return Ok(MaybeException::Ok(TileResponse {
                        data: pilota::Bytes::new(),
                        content_type: tile_info.format.content_type().to_string().into(),
                        content_encoding: None,
                        etag: Some(empty_tile.etag.clone().into()),
                        not_modified: Some(true),
                    }));
                }
            }

            return Ok(MaybeException::Ok(TileResponse {
                data: pilota::Bytes::new(),
                content_type: tile_info.format.content_type().to_string().into(),
                content_encoding: None,
                etag: Some(empty_tile.etag.into()),
                not_modified: Some(false),
            }));
        }

        // Merge logic
        let (merged_data, merged_etag) = if non_empty_tiles.len() == 1 {
            let tile = non_empty_tiles
                .pop()
                .expect("non_empty_tiles has exactly 1 element");
            (tile.data, tile.etag)
        } else {
            // Validate merge capability (MVT only)
            let first_info = non_empty_tiles[0].info;
            let can_merge = first_info.format == Format::Mvt
                && (first_info.encoding == Encoding::Uncompressed
                    || first_info.encoding == Encoding::Gzip);

            if !can_merge {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(TileError {
                        code: 400,
                        message: format!(
                            "Cannot merge {} tiles. Ensure only one source has data or format is MVT.",
                            first_info
                        )
                        .into(),
                    }),
                ));
            }

            // Concatenate data and ETags
            let mut data_len = 0;
            let mut etag_len = 0;
            for t in &non_empty_tiles {
                data_len += t.data.len();
                etag_len += t.etag.len();
            }

            let mut merged_data = Vec::with_capacity(data_len);
            let mut merged_etag = String::with_capacity(etag_len);

            for t in non_empty_tiles {
                merged_data.extend_from_slice(&t.data);
                merged_etag.push_str(&t.etag);
            }

            (merged_data, merged_etag)
        };

        // Check If-None-Match (Conditional Request) - applies to both single and merged tiles
        if let Some(if_none_match) = &request.if_none_match {
            if etag_matches(if_none_match, &merged_etag) {
                return Ok(MaybeException::Ok(TileResponse {
                    data: pilota::Bytes::new(),
                    content_type: final_info.format.content_type().to_string().into(),
                    content_encoding: None,
                    etag: Some(merged_etag.into()),
                    not_modified: Some(true),
                }));
            }
        }

        // Encoding negotiation
        let mut final_data = merged_data;
        let mut final_encoding = sources[0].get_tile_info().encoding;

        if let Some(accept) = request.accept_encoding.as_deref() {
            let accept = accept.trim();

            if accept.is_empty() {
                if matches!(final_encoding, Encoding::Gzip | Encoding::Brotli) {
                    final_data = match decode_tile_data(final_data, final_encoding) {
                        Ok(data) => data,
                        Err(err) => {
                            return Ok(MaybeException::Exception(
                                MaptileServiceGetTileException::Error(err),
                            ));
                        }
                    };
                    final_encoding = Encoding::Uncompressed;
                }
            } else {
                if matches!(final_encoding, Encoding::Gzip | Encoding::Brotli)
                    && !accepts_explicit_encoding(accept, final_encoding)
                {
                    final_data = match decode_tile_data(final_data, final_encoding) {
                        Ok(data) => data,
                        Err(err) => {
                            return Ok(MaybeException::Exception(
                                MaptileServiceGetTileException::Error(err),
                            ));
                        }
                    };
                    final_encoding = Encoding::Uncompressed;
                }

                if final_encoding == Encoding::Uncompressed {
                    let target_encoding = match decide_target_encoding(accept) {
                        Ok(target) => target,
                        Err(err) => {
                            return Ok(MaybeException::Exception(
                                MaptileServiceGetTileException::Error(err),
                            ));
                        }
                    };

                    if let Some(target_encoding) = target_encoding {
                        if target_encoding != Encoding::Uncompressed {
                            final_data = match encode_tile_data(final_data, target_encoding) {
                                Ok(data) => data,
                                Err(err) => {
                                    return Ok(MaybeException::Exception(
                                        MaptileServiceGetTileException::Error(err),
                                    ));
                                }
                            };
                            final_encoding = target_encoding;
                        }
                    }
                }
            }
        } else if matches!(final_encoding, Encoding::Gzip | Encoding::Brotli) {
            final_data = match decode_tile_data(final_data, final_encoding) {
                Ok(data) => data,
                Err(err) => {
                    return Ok(MaybeException::Exception(
                        MaptileServiceGetTileException::Error(err),
                    ));
                }
            };
            final_encoding = Encoding::Uncompressed;
        }

        Ok(MaybeException::Ok(TileResponse {
            data: pilota::Bytes::from(final_data),
            content_type: final_info.format.content_type().to_string().into(),
            content_encoding: final_encoding
                .content_encoding()
                .map(|s| s.to_string().into()),
            etag: Some(merged_etag.into()),
            not_modified: Some(false),
        }))
    }

    /// List all available tile sources
    async fn list_sources(&self) -> ListSourcesResult {
        let service = self.read().await;

        let infos: Vec<TileInfo> = service
            .sources
            .iter()
            .map(|source| {
                let tj = source.get_tilejson();
                TileInfo {
                    source_id: source.get_id().to_string().into(),
                    name: tj
                        .name
                        .clone()
                        .unwrap_or_else(|| source.get_id().to_string())
                        .into(),
                    min_zoom: tj.minzoom.map(i32::from),
                    max_zoom: tj.maxzoom.map(i32::from),
                    bounds: tj.bounds.map(|b| {
                        format!(
                            "{left},{bottom},{right},{top}",
                            left = b.left,
                            bottom = b.bottom,
                            right = b.right,
                            top = b.top
                        )
                        .into()
                    }),
                    description: tj.description.clone().map(Into::into),
                    attribution: tj.attribution.clone().map(Into::into),
                }
            })
            .collect();

        Ok(MaybeException::Ok(infos))
    }

    /// Get information about a specific tile source
    async fn get_source_info(&self, source_id: ::pilota::FastStr) -> GetSourceInfoResult {
        let service = self.read().await;

        let Some(source) = service.get_source(&source_id) else {
            return Ok(MaybeException::Exception(
                MaptileServiceGetSourceInfoException::Error(TileError {
                    code: 404,
                    message: format!("Source '{source_id}' not found").into(),
                }),
            ));
        };
        let tj = source.get_tilejson();

        Ok(MaybeException::Ok(TileInfo {
            source_id: source.get_id().to_string().into(),
            name: tj
                .name
                .clone()
                .unwrap_or_else(|| source.get_id().to_string())
                .into(),
            min_zoom: tj.minzoom.map(i32::from),
            max_zoom: tj.maxzoom.map(i32::from),
            bounds: tj.bounds.map(|b| {
                format!(
                    "{left},{bottom},{right},{top}",
                    left = b.left,
                    bottom = b.bottom,
                    right = b.right,
                    top = b.top
                )
                .into()
            }),
            description: tj.description.clone().map(Into::into),
            attribution: tj.attribution.clone().map(Into::into),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use martin_core::tiles::Source;
    use martin_tile_utils::{Encoding, Format, TileInfo as CoreTileInfo};
    use tilejson::Bounds;
    use tokio::sync::RwLock;
    use volo_thrift::MaybeException;

    use super::{MAX_ZOOM, MaptileServiceImpl, ThriftTileCoord};
    use crate::config::{ConfigSettings, MaptileConfig, PostgresConfig, ServerConfig};
    use crate::volo_gen::maptile::r#gen::{MaptileService, TileRequest};

    #[derive(Clone, Debug)]
    struct TestSource {
        id: String,
        tilejson: tilejson::TileJSON,
        tile_info: CoreTileInfo,
        data: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl Source for TestSource {
        fn get_id(&self) -> &str {
            &self.id
        }

        fn get_tilejson(&self) -> &tilejson::TileJSON {
            &self.tilejson
        }

        fn get_tile_info(&self) -> CoreTileInfo {
            self.tile_info
        }

        fn clone_source(&self) -> martin_core::tiles::BoxedSource {
            Box::new(self.clone())
        }

        async fn get_tile(
            &self,
            _xyz: martin_tile_utils::TileCoord,
            _url_query: Option<&martin_core::tiles::UrlQuery>,
        ) -> martin_core::tiles::MartinCoreResult<martin_tile_utils::TileData> {
            Ok(self.data.clone())
        }
    }

    fn build_service(
        sources: Vec<martin_core::tiles::BoxedSource>,
    ) -> std::sync::Arc<RwLock<MaptileServiceImpl>> {
        let source_map = sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.get_id().to_string(), i))
            .collect();
        let config = MaptileConfig {
            server: ServerConfig::default(),
            postgres: PostgresConfig {
                connection_string: "postgres://user@localhost/db".to_string(),
                pool_size: 1,
                ssl_cert: None,
                ssl_key: None,
                ssl_root_cert: None,
                auto_generate_filters: false,
                filter_function_suffix: "filtered".to_string(),
            },
            config: ConfigSettings::default(),
            redis: None,
        };
        std::sync::Arc::new(RwLock::new(MaptileServiceImpl {
            sources,
            source_map,
            metadata: None,
            config,
        }))
    }

    fn build_tilejson() -> tilejson::TileJSON {
        let bounds = Bounds::from_str("-1.0,-2.0,3.0,4.0").unwrap();
        let mut tj = tilejson::tilejson! {
            tiles: vec![],
            name: "Pretty".to_string(),
            description: "Desc".to_string(),
            attribution: "Attr".to_string(),
        };
        tj.minzoom = Some(0);
        tj.maxzoom = Some(10);
        tj.bounds = Some(bounds);
        tj
    }

    fn build_tilejson_with_zoom(min: u8, max: u8) -> tilejson::TileJSON {
        let mut tj = build_tilejson();
        tj.minzoom = Some(min);
        tj.maxzoom = Some(max);
        tj
    }

    #[test]
    fn validate_tile_coord_rejects_invalid_zoom() {
        let coord = ThriftTileCoord { z: -1, x: 0, y: 0 };
        let err = MaptileServiceImpl::validate_tile_coord(&coord).unwrap_err();
        assert_eq!(err.code, 400);

        let coord = ThriftTileCoord {
            z: MAX_ZOOM + 1,
            x: 0,
            y: 0,
        };
        let err = MaptileServiceImpl::validate_tile_coord(&coord).unwrap_err();
        assert_eq!(err.code, 400);
    }

    #[test]
    fn validate_tile_coord_rejects_negative_xy() {
        let coord = ThriftTileCoord { z: 0, x: -1, y: 0 };
        let err = MaptileServiceImpl::validate_tile_coord(&coord).unwrap_err();
        assert_eq!(err.code, 400);
    }

    #[test]
    fn validate_tile_coord_rejects_out_of_range_xy() {
        let coord = ThriftTileCoord { z: 1, x: 2, y: 0 };
        let err = MaptileServiceImpl::validate_tile_coord(&coord).unwrap_err();
        assert_eq!(err.code, 400);
    }

    #[tokio::test]
    async fn list_sources_returns_tileinfo() {
        let tilejson = build_tilejson_with_zoom(2, 5);
        let expected_bounds = tilejson.bounds.as_ref().map(|b| {
            format!(
                "{left},{bottom},{right},{top}",
                left = b.left,
                bottom = b.bottom,
                right = b.right,
                top = b.top
            )
        });
        let source = TestSource {
            id: "source".to_string(),
            tilejson,
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: Vec::new(),
        };
        let service = build_service(vec![Box::new(source)]);

        let response = MaptileService::list_sources(&service).await.unwrap();
        let sources = match response {
            MaybeException::Ok(sources) => sources,
            MaybeException::Exception(err) => panic!("unexpected error: {err:?}"),
        };

        assert_eq!(sources.len(), 1);
        let info = &sources[0];
        assert_eq!(info.source_id.as_str(), "source");
        assert_eq!(info.name.as_str(), "Pretty");
        assert_eq!(info.min_zoom, Some(2));
        assert_eq!(info.max_zoom, Some(5));
        assert_eq!(info.description.as_deref(), Some("Desc"));
        assert_eq!(info.attribution.as_deref(), Some("Attr"));

        assert_eq!(info.bounds.as_ref().map(|b| b.to_string()), expected_bounds);
    }

    #[tokio::test]
    async fn get_source_info_returns_source_metadata() {
        let source = TestSource {
            id: "source".to_string(),
            tilejson: build_tilejson_with_zoom(2, 5),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: Vec::new(),
        };

        let service = build_service(vec![Box::new(source)]);

        let response = MaptileService::get_source_info(&service, "source".into())
            .await
            .unwrap();
        let info = match response {
            MaybeException::Ok(info) => info,
            MaybeException::Exception(err) => panic!("unexpected error: {err:?}"),
        };

        assert_eq!(info.source_id.as_str(), "source");
        assert_eq!(info.name.as_str(), "Pretty");
        assert_eq!(info.min_zoom, Some(2));
        assert_eq!(info.max_zoom, Some(5));
        assert_eq!(info.description.as_deref(), Some("Desc"));
        assert_eq!(info.attribution.as_deref(), Some("Attr"));
    }

    #[tokio::test]
    async fn get_source_info_missing_returns_404() {
        let source = TestSource {
            id: "existing".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: Vec::new(),
        };

        let service = build_service(vec![Box::new(source)]);

        let response = MaptileService::get_source_info(&service, "nonexistent".into())
            .await
            .unwrap();

        match response {
            MaybeException::Ok(_) => panic!("expected 404 error"),
            MaybeException::Exception(err) => {
                let tile_error = match err {
                    crate::volo_gen::maptile::r#gen::MaptileServiceGetSourceInfoException::Error(
                        e,
                    ) => e,
                };
                assert_eq!(tile_error.code, 404);
                assert!(tile_error.message.contains("not found"));
            }
        }
    }

    #[tokio::test]
    async fn get_tile_merged() {
        let s1 = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let s2 = TestSource {
            id: "s2".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![4, 5, 6],
        };
        let service = build_service(vec![Box::new(s1), Box::new(s2)]);

        let req = TileRequest {
            source_id: "".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: Some("s1,s2".into()),
            accept_encoding: None,
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        let tile = match resp {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };

        assert_eq!(tile.data.as_ref(), &[1, 2, 3, 4, 5, 6]);
        assert!(tile.etag.is_some());
    }

    #[tokio::test]
    async fn get_tile_conditional_match() {
        let s1 = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let service = build_service(vec![Box::new(s1)]);

        // 1. Get ETag
        let req1 = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: None,
        };
        let resp1 = service.get_tile(req1).await.unwrap();
        let tile1 = match resp1 {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };
        let etag = tile1.etag.unwrap();

        // 2. Conditional Request
        let req2 = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: Some(etag.clone()),
        };
        let resp2 = service.get_tile(req2).await.unwrap();
        let tile2 = match resp2 {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };

        assert_eq!(tile2.not_modified, Some(true));
        assert!(tile2.data.is_empty());
        assert_eq!(tile2.etag, Some(etag));
    }

    #[tokio::test]
    async fn get_tile_encoding_negotiation() {
        // Source is Uncompressed
        let s1 = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let service = build_service(vec![Box::new(s1)]);

        // Request Gzip
        let req = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: Some("gzip".into()),
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        let tile = match resp {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };

        assert_eq!(tile.content_encoding, Some("gzip".into()));
        assert_ne!(tile.data.as_ref(), &[1, 2, 3]); // Should be compressed
    }

    #[tokio::test]
    async fn get_tile_source_not_found_returns_404() {
        let source = TestSource {
            id: "existing".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let service = build_service(vec![Box::new(source)]);

        let req = TileRequest {
            source_id: "nonexistent".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        match resp {
            MaybeException::Ok(_) => panic!("expected 404 error"),
            MaybeException::Exception(err) => {
                let tile_error = match err {
                    crate::volo_gen::maptile::r#gen::MaptileServiceGetTileException::Error(e) => e,
                };
                assert_eq!(tile_error.code, 404);
            }
        }
    }

    #[tokio::test]
    async fn get_tile_incompatible_formats_returns_400() {
        let s1 = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let s2 = TestSource {
            id: "s2".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Brotli),
            data: vec![4, 5, 6],
        };
        let service = build_service(vec![Box::new(s1), Box::new(s2)]);

        let req = TileRequest {
            source_id: "".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: Some("s1,s2".into()),
            accept_encoding: None,
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        match resp {
            MaybeException::Ok(_) => panic!("expected 400 error"),
            MaybeException::Exception(err) => {
                let tile_error = match err {
                    crate::volo_gen::maptile::r#gen::MaptileServiceGetTileException::Error(e) => e,
                };
                assert_eq!(tile_error.code, 400);
            }
        }
    }

    #[tokio::test]
    async fn get_tile_encoding_negotiation_failure_returns_406() {
        let source = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let service = build_service(vec![Box::new(source)]);

        let req = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: Some("gzip;q=0, br;q=0, identity;q=0, *;q=0".into()),
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        match resp {
            MaybeException::Ok(_) => panic!("expected 406 error"),
            MaybeException::Exception(err) => {
                let tile_error = match err {
                    crate::volo_gen::maptile::r#gen::MaptileServiceGetTileException::Error(e) => e,
                };
                assert_eq!(tile_error.code, 406);
            }
        }
    }

    #[tokio::test]
    async fn get_tile_too_many_sources_returns_400() {
        use super::MAX_SOURCE_IDS;

        let sources: Vec<martin_core::tiles::BoxedSource> = (0..=MAX_SOURCE_IDS)
            .map(|i| {
                Box::new(TestSource {
                    id: format!("s{i}"),
                    tilejson: build_tilejson(),
                    tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
                    data: vec![1],
                }) as martin_core::tiles::BoxedSource
            })
            .collect();

        let service = build_service(sources);

        let source_ids: String = (0..=MAX_SOURCE_IDS)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(",");

        let req = TileRequest {
            source_id: "".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: Some(source_ids.into()),
            accept_encoding: None,
            if_none_match: None,
        };

        let resp = service.get_tile(req).await.unwrap();
        match resp {
            MaybeException::Ok(_) => panic!("expected 400 error for too many sources"),
            MaybeException::Exception(err) => {
                let tile_error = match err {
                    crate::volo_gen::maptile::r#gen::MaptileServiceGetTileException::Error(e) => e,
                };
                assert_eq!(tile_error.code, 400);
                assert!(tile_error.message.contains("Too many sources"));
            }
        }
    }

    #[tokio::test]
    async fn get_tile_single_source_conditional_match() {
        let source = TestSource {
            id: "s1".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
            data: vec![1, 2, 3],
        };
        let service = build_service(vec![Box::new(source)]);

        let req1 = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: None,
        };
        let resp1 = service.get_tile(req1).await.unwrap();
        let tile1 = match resp1 {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };
        let etag = tile1.etag.unwrap();

        let req2 = TileRequest {
            source_id: "s1".into(),
            coord: ThriftTileCoord { z: 0, x: 0, y: 0 },
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: Some(etag.clone()),
        };
        let resp2 = service.get_tile(req2).await.unwrap();
        let tile2 = match resp2 {
            MaybeException::Ok(t) => t,
            _ => panic!("Expected Ok"),
        };

        assert_eq!(tile2.not_modified, Some(true));
        assert!(tile2.data.is_empty());
        assert_eq!(tile2.etag, Some(etag));
    }
}
