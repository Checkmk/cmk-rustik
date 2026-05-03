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
