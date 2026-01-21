//! Maptile - High-performance Thrift RPC microservice for vector tile serving
//!
//! This crate provides a Volo Thrift-based RPC service for serving vector tiles
//! from PostgreSQL/PostGIS databases. It reuses `martin-core` for tile generation
//! and provides a microservice-friendly interface.

#![warn(clippy::pedantic)]

pub mod config;
pub mod server;

// Include the generated Thrift code
include!(concat!(env!("OUT_DIR"), "/volo_gen.rs"));

pub use config::{MaptileConfig, load_config};
pub use server::MaptileServiceImpl;
