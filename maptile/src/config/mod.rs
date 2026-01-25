//! Configuration module

pub mod loader;
pub mod redis_consumer;
pub mod types;

pub use loader::{
    ConfigResult, create_config_pool, load_config, load_sources_from_database,
    query_config_metadata,
};
pub use redis_consumer::start_redis_consumer_task;
pub use types::*;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let yaml = "postgres:
  connection_string: postgresql://user:pass@localhost/db";
        let config: MaptileConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(
            config.postgres.connection_string,
            "postgresql://user:pass@localhost/db"
        );
        assert_eq!(config.server.listen_address, "0.0.0.0:8089");
        assert_eq!(config.postgres.pool_size, 10);
        assert_eq!(config.config.source, "database");
        assert!(config.redis.is_none());
    }

    #[test]
    fn test_full_config() {
        let yaml = "server:
  listen_address: 127.0.0.1:9090
postgres:
  connection_string: postgresql://host/db
  pool_size: 20
config:
  reload_interval_sec: 30
redis:
  url: redis://localhost:6379";
        let config: MaptileConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.server.listen_address, "127.0.0.1:9090");
        assert_eq!(config.postgres.pool_size, 20);
        assert_eq!(config.config.reload_interval_sec, Some(30));
        assert_eq!(
            config.redis.as_ref().map(|redis| redis.url.as_str()),
            Some("redis://localhost:6379")
        );
    }
}
