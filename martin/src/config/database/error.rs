use std::fmt;

/// Result type for database configuration operations
pub type DatabaseConfigResult<T> = Result<T, DatabaseConfigError>;

/// Errors that can occur during database configuration operations
#[derive(Debug)]
pub enum DatabaseConfigError {
    /// Database connection failed
    ConnectionFailed(String),

    /// Schema validation failed (tables missing or incorrect structure)
    SchemaInvalid(String),

    /// Query execution failed
    QueryFailed(String),

    /// Row deserialization failed
    DeserializationFailed(String),

    /// Configuration validation failed
    ValidationFailed(String),

    /// No sources found in configuration
    NoSources,

    /// Generic error
    Other(String),
}

impl fmt::Display for DatabaseConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => {
                write!(f, "Database connection failed: {msg}")
            }
            Self::SchemaInvalid(msg) => {
                write!(f, "Configuration schema is invalid or missing: {msg}")
            }
            Self::QueryFailed(msg) => {
                write!(f, "Configuration query failed: {msg}")
            }
            Self::DeserializationFailed(msg) => {
                write!(f, "Failed to parse configuration row: {msg}")
            }
            Self::ValidationFailed(msg) => {
                write!(f, "Configuration validation failed: {msg}")
            }
            Self::NoSources => {
                write!(
                    f,
                    "No enabled sources found in configuration database. \
                    Add sources to martin_config.data_sources or martin_config.file_sources tables."
                )
            }
            Self::Other(msg) => write!(f, "Configuration error: {msg}"),
        }
    }
}

impl std::error::Error for DatabaseConfigError {}

impl From<DatabaseConfigError> for crate::MartinError {
    fn from(err: DatabaseConfigError) -> Self {
        crate::MartinError::ConfigError(err.to_string())
    }
}

#[cfg(feature = "postgres")]
impl From<deadpool_postgres::PoolError> for DatabaseConfigError {
    fn from(err: deadpool_postgres::PoolError) -> Self {
        Self::ConnectionFailed(err.to_string())
    }
}

#[cfg(feature = "postgres")]
impl From<deadpool_postgres::tokio_postgres::Error> for DatabaseConfigError {
    fn from(err: deadpool_postgres::tokio_postgres::Error) -> Self {
        Self::QueryFailed(err.to_string())
    }
}

impl From<serde_json::Error> for DatabaseConfigError {
    fn from(err: serde_json::Error) -> Self {
        Self::DeserializationFailed(err.to_string())
    }
}
