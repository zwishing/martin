//! MaptileService implementation

use std::collections::HashMap;
use std::sync::Arc;

use log::{debug, error, info};
use martin_core::tiles::{BoxedSource, UrlQuery};
use martin_tile_utils::TileCoord;
use tokio::sync::RwLock;
use volo_thrift::{MaybeException, ServerError};

use crate::config::{
    ConfigMetadata, MaptileConfig, create_config_pool, load_sources_from_database,
};
use crate::volo_gen::maptile::r#gen::{
    MaptileService, MaptileServiceGetSourceInfoException, MaptileServiceGetTileException,
    MaptileServiceListSourcesException, TileCoord as ThriftTileCoord, TileError, TileInfo,
    TileRequest, TileResponse,
};

const MAX_ZOOM: i16 = 30;

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
    pub async fn new(config: MaptileConfig) -> anyhow::Result<Self> {
        let pool = create_config_pool(&config.postgres).await?;
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
                    "Invalid zoom {}, expected 0..={}",
                    coord.z, MAX_ZOOM
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

        let zoom = coord.z as u32;
        let max_index = (1u64 << zoom) - 1;
        let x = coord.x as u64;
        let y = coord.y as u64;

        if x > max_index || y > max_index {
            return Err(TileError {
                code: 400,
                message: format!(
                    "Tile coordinates out of range for z={}: x/y must be 0..={}",
                    coord.z, max_index
                )
                .into(),
            });
        }

        Ok(TileCoord {
            z: coord.z as u8,
            x: x as u32,
            y: y as u32,
        })
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
        let (source, source_id) = {
            let service = self.read().await;
            let Some(source) = service.get_source(&request.source_id) else {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(TileError {
                        code: 404,
                        message: format!("Source '{}' not found", request.source_id).into(),
                    }),
                ));
            };
            (source.clone(), request.source_id.clone())
        };

        let coord = match MaptileServiceImpl::validate_tile_coord(&request.coord) {
            Ok(coord) => coord,
            Err(err) => {
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(err),
                ))
            }
        };

        // Check zoom level validity
        if !source.is_valid_zoom(coord.z) {
            debug!("Zoom {} out of range for source '{}'", coord.z, source_id);
            return Ok(MaybeException::Ok(TileResponse {
                data: pilota::Bytes::new(),
                content_type: "application/vnd.mapbox-vector-tile".into(),
                content_encoding: None,
                etag: None,
            }));
        }

        let url_query: Option<UrlQuery> = request.query_params.map(|params| {
            params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        });

        let tile_data = match source.get_tile(coord, url_query.as_ref()).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to get tile: {}", e);
                return Ok(MaybeException::Exception(
                    MaptileServiceGetTileException::Error(TileError {
                        code: 500,
                        message: format!("Failed to get tile: {}", e).into(),
                    }),
                ));
            }
        };

        let tile_info = source.get_tile_info();

        Ok(MaybeException::Ok(TileResponse {
            data: pilota::Bytes::from(tile_data),
            content_type: tile_info.format.content_type().to_string().into(),
            content_encoding: tile_info
                .encoding
                .content_encoding()
                .map(|s| s.to_string().into()),
            etag: None,
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
                    min_zoom: tj.minzoom.map(|z| z as i32),
                    max_zoom: tj.maxzoom.map(|z| z as i32),
                    bounds: tj
                        .bounds
                        .map(|b| format!("{},{},{},{}", b.left, b.bottom, b.right, b.top).into()),
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
                    message: format!("Source '{}' not found", source_id).into(),
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
            min_zoom: tj.minzoom.map(|z| z as i32),
            max_zoom: tj.maxzoom.map(|z| z as i32),
            bounds: tj
                .bounds
                .map(|b| format!("{},{},{},{}", b.left, b.bottom, b.right, b.top).into()),
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

    use super::{MaptileServiceImpl, ThriftTileCoord, MAX_ZOOM};
    use crate::config::{ConfigSettings, MaptileConfig, PostgresConfig, ServerConfig};
    use crate::volo_gen::maptile::r#gen::MaptileService;

    #[derive(Clone, Debug)]
    struct TestSource {
        id: String,
        tilejson: tilejson::TileJSON,
        tile_info: CoreTileInfo,
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
            Ok(Vec::new())
        }
    }

    fn build_service(sources: Vec<martin_core::tiles::BoxedSource>) -> std::sync::Arc<RwLock<MaptileServiceImpl>> {
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
            },
            config: ConfigSettings::default(),
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
        tj.minzoom = Some(2);
        tj.maxzoom = Some(5);
        tj.bounds = Some(bounds);
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
        let tilejson = build_tilejson();
        let expected_bounds = tilejson.bounds.as_ref().map(|b| {
            format!("{},{},{},{}", b.left, b.bottom, b.right, b.top)
        });
        let source = TestSource {
            id: "source".to_string(),
            tilejson,
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
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
    async fn get_source_info_returns_tileinfo() {
        let source = TestSource {
            id: "source".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
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
            id: "source".to_string(),
            tilejson: build_tilejson(),
            tile_info: CoreTileInfo::new(Format::Mvt, Encoding::Uncompressed),
        };
        let service = build_service(vec![Box::new(source)]);

        let response = MaptileService::get_source_info(&service, "missing".into())
            .await
            .unwrap();
        let err = match response {
            MaybeException::Ok(_) => panic!("expected error"),
            MaybeException::Exception(err) => err,
        };

        match err {
            crate::volo_gen::maptile::r#gen::MaptileServiceGetSourceInfoException::Error(
                tile_err,
            ) => {
                assert_eq!(tile_err.code, 404);
            }
        }
    }
}
