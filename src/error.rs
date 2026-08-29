use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "no config file found at {path}\nhint: run `exc init` to create a starter config there, or pass --config <path>",
        path = .0.display()
    )]
    NotFound(PathBuf),

    #[error("failed to read config at {path}: {source}", path = .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}\n\n{source}", path = .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

impl ConfigError {
    /// Exit code contract: config not found / unreadable / unparsable -> 2.
    pub fn exit_code(&self) -> i32 {
        2
    }
}
