use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes error")]
    Kube(#[from] kube::Error),
    #[error("error inferring Kubernetes config")]
    KubeInferConfig(#[from] kube::config::InferConfigError),
    #[error("error getting environment variable: {name}")]
    EnvVar {
        name: String,
        #[source]
        source: std::env::VarError,
    },
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("address parse error")]
    AddrParse(#[from] std::net::AddrParseError),
}
