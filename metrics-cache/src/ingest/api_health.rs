use http::Request;
use kube::Client;
use kube::client::Body;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::watch::Sender;
use tokio::time;
use tokio::time::Duration;
use tracing::error;

pub(crate) type ApiHealthUpdate = Option<Arc<ApiHealth>>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes error")]
    Kube(#[from] kube::Error),
    #[error("http error")]
    HttpError(#[from] http::Error),
    #[error("utf-8 error")]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error("error sending result")]
    Send(#[from] tokio::sync::watch::error::SendError<ApiHealthUpdate>),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct ApiHealth {
    pub live: HealthResponse,
    pub ready: HealthResponse,
}

#[derive(Debug)]
pub struct HealthResponse {
    pub status_code: u16,
    pub body: String,
}

async fn query_health(client: &Client, path: &str) -> Result<HealthResponse> {
    let request = match Request::get(path).body(Body::empty()) {
        Ok(request) => request,
        Err(error) => {
            error!(?error, path, "could not construct API health request");
            return Err(error.into());
        }
    };
    let response = client.send(request).await?;
    let status_code = response.status().as_u16();
    let body = response.into_body().collect_bytes().await?;
    let body = String::from_utf8(body.to_vec())?;

    Ok(HealthResponse { status_code, body })
}

pub async fn loop_query_health(
    client: Client,
    sender: Sender<ApiHealthUpdate>,
    poll_interval: Duration,
) -> Result<()> {
    let mut interval = time::interval(poll_interval);
    loop {
        interval.tick().await; // note: The very first tick() is no-op
        let (live, ready) = tokio::join!(
            query_health(&client, "/livez"),
            query_health(&client, "/readyz"),
        );
        let update = match (live, ready) {
            (Ok(live), Ok(ready)) => Some(Arc::new(ApiHealth { live, ready })),
            (live, ready) => {
                if let Err(error) = live {
                    error!(?error, "failed to fetch /livez");
                }
                if let Err(error) = ready {
                    error!(?error, "failed to fetch /readyz");
                }
                None
            }
        };
        sender.send(update)?;
    }
}
