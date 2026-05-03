use moka::future::Cache;
use std::sync::Arc;

use crate::kube_auth::TokenValidator;
use cmk_kube_types::{machine_sections, metadata};

#[derive(Clone)]
pub struct AppState<V: TokenValidator> {
    pub validator: V,
    pub reader_allowlist: Vec<String>,
    pub writer_allowlist: Vec<String>,
    pub metrics_cache_static_metadata: Arc<metadata::StaticMetadata>,
    pub machine_sections_cache: Cache<String, machine_sections::FetchResult>,
    pub metrics_fetcher_metadata_cache: Cache<String, metadata::metrics_fetcher::Metadata>,
}

// Intentionally public, provides util functions for other modules
#[cfg(test)]
pub mod tests {
    use anyhow::Result;
    use k8s_openapi::api::authentication::v1::TokenReview;
    use moka::future::Cache;
    use std::sync::Arc;

    use super::AppState;
    use crate::kube_auth::TokenValidator;
    use cmk_kube_types::metadata;

    #[derive(Clone)]
    pub struct MockValidator {
        pub response: std::result::Result<TokenReview, ()>,
    }

    impl TokenValidator for MockValidator {
        async fn validate(&self, _token: &str) -> Result<TokenReview> {
            self.response
                .clone()
                .map_err(|_| anyhow::anyhow!("mock error"))
        }
    }

    pub fn test_app_state_with_validator(validator: MockValidator) -> AppState<MockValidator> {
        AppState {
            validator,
            reader_allowlist: vec!["test-ns:test-reader".to_string()],
            writer_allowlist: vec!["test-ns:test-writer".to_string()],
            metrics_cache_static_metadata: Arc::new(metadata::StaticMetadata {
                node: "test-node".to_string(),
                host_name: "test-host".to_string(),
                container_platform: metadata::Platform {
                    os_name: "linux".to_string(),
                    os_version: "5.15".to_string(),
                    python_version: "3.9".to_string(),
                    python_compiler: "GCC 10.2".to_string(),
                },
                checkmk_kube_agent: metadata::CheckmkKubeAgent {
                    project_version: "1.0.0".to_string(),
                },
            }),
            machine_sections_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .build(),
            metrics_fetcher_metadata_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .max_capacity(10000)
                .build(),
        }
    }

    pub fn test_app_state() -> AppState<MockValidator> {
        test_app_state_with_validator(MockValidator {
            response: Ok(TokenReview::default()),
        })
    }
}
