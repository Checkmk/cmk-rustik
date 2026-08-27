use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;
use crate::auth::kubernetes::TokenValidator;

pub async fn livez() -> StatusCode {
    StatusCode::OK
}

pub async fn readyz(State(state): State<AppState<impl TokenValidator>>) -> StatusCode {
    if state.readiness.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::state::tests::test_app_state;

    #[tokio::test]
    async fn health_endpoints_reflect_readiness() {
        let state = test_app_state();

        assert_eq!(livez().await, StatusCode::OK);
        assert_eq!(
            readyz(State(state.clone())).await,
            StatusCode::SERVICE_UNAVAILABLE
        );

        state.readiness.mark_ready();

        assert_eq!(readyz(State(state)).await, StatusCode::OK);
    }
}
