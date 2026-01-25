use std::net::SocketAddr;
use std::str::FromStr;

use maptile::volo_gen::maptile::r#gen::{
    MaptileServiceClient, MkMaptileServiceGenericClient, TileCoord, TileRequest,
};
use volo_thrift::MaybeException;
use volo_thrift::client::ClientBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut addr: SocketAddr = "127.0.0.1:8089".parse().unwrap();
    let mut source_id: Option<String> = None;
    let mut coord: Option<TileCoord> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--addr" => {
                if let Some(value) = args.next() {
                    addr = SocketAddr::from_str(&value)?;
                }
            }
            "--source" => {
                if let Some(value) = args.next() {
                    source_id = Some(value);
                }
            }
            "--z" => {
                if let Some(value) = args.next() {
                    let z: i16 = value.parse()?;
                    let coord_value = coord.get_or_insert(TileCoord { z, x: 0, y: 0 });
                    coord_value.z = z;
                }
            }
            "--x" => {
                if let Some(value) = args.next() {
                    let x: i64 = value.parse()?;
                    let coord_value = coord.get_or_insert(TileCoord { z: 0, x, y: 0 });
                    coord_value.x = x;
                }
            }
            "--y" => {
                if let Some(value) = args.next() {
                    let y: i64 = value.parse()?;
                    let coord_value = coord.get_or_insert(TileCoord { z: 0, x: 0, y });
                    coord_value.y = y;
                }
            }
            _ => {}
        }
    }

    let client: MaptileServiceClient = {
        ClientBuilder::new("maptile", MkMaptileServiceGenericClient)
            .address(addr)
            .build()
    };

    if let (Some(source_id), Some(coord)) = (source_id, coord) {
        println!(
            "Fetching tile {} / {},{},{}",
            source_id, coord.z, coord.x, coord.y
        );
        let req = TileRequest {
            source_id: source_id.clone().into(),
            coord,
            query_params: None,
            source_ids: None,
            accept_encoding: None,
            if_none_match: None,
        };
        match client.get_tile(req).await {
            Ok(MaybeException::Ok(resp)) => {
                println!(
                    "Got tile: {} bytes, type: {}, etag: {:?}, not_modified: {:?}",
                    resp.data.len(),
                    resp.content_type,
                    resp.etag,
                    resp.not_modified
                );
            }
            Ok(MaybeException::Exception(ex)) => {
                println!("Error getting tile: {:?}", ex);
            }
            Err(e) => println!("Failed to get tile (transport): {:?}", e),
        }
        return Ok(());
    }

    println!("Listing sources...");
    match client.list_sources().await {
        Ok(MaybeException::Ok(sources)) => {
            println!("Found {} sources:", sources.len());
            for source in sources {
                println!(
                    "- {} (zoom: {:?}-{:?})",
                    source.name, source.min_zoom, source.max_zoom
                );

                // Try to get a tile from the first source
                println!("Fetching tile 0/0/0 from {}", source.source_id);
                let req = TileRequest {
                    source_id: source.source_id.clone(),
                    coord: TileCoord { z: 0, x: 0, y: 0 },
                    query_params: None,
                    source_ids: None,
                    accept_encoding: None,
                    if_none_match: None,
                };

                match client.get_tile(req).await {
                    Ok(MaybeException::Ok(resp)) => {
                        println!(
                            "Got tile: {} bytes, type: {}, etag: {:?}, not_modified: {:?}",
                            resp.data.len(),
                            resp.content_type,
                            resp.etag,
                            resp.not_modified
                        );
                    }
                    Ok(MaybeException::Exception(ex)) => {
                        println!("Error getting tile: {:?}", ex);
                    }
                    Err(e) => println!("Failed to get tile (transport): {:?}", e),
                }
            }
        }
        Ok(MaybeException::Exception(ex)) => {
            println!("Error listing sources: {:?}", ex);
        }
        Err(e) => println!("Failed to list sources (transport): {:?}", e),
    }

    Ok(())
}
