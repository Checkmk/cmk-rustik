use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("reqwest error")]
    Reqwest(#[from] reqwest::Error),
    #[error("IO error {0}")]
    Io(#[from] std::io::Error),
    #[error("error getting environment variable: {name}")]
    EnvVar {
        name: String,
        #[source]
        source: std::env::VarError,
    },
}
