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
    #[error("check_mk_agent timed out after {0:?}")]
    AgentTimeout(std::time::Duration),
    #[error("check_mk_agent exited with {status}: {stderr}")]
    AgentExitStatus {
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error("check_mk_agent produced empty output")]
    AgentEmptyOutput,
}
