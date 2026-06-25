pub mod register;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("rcgen error")]
    Rcgen(#[from] rcgen::Error),
    #[error("push-mode error")]
    PushMode(String),
}
