use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, LabelSelectorRequirement};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tracing::debug;

#[derive(Serialize)]
pub(crate) struct LabelRef<'a> {
    name: &'a str,
    value: &'a str,
}

impl<'a> LabelRef<'a> {
    pub fn from_map(map: &'a BTreeMap<String, String>) -> BTreeMap<&'a str, Self> {
        map.iter()
            .map(|(name, value)| (name.as_str(), Self { name, value }))
            .collect()
    }
}

#[derive(Serialize)]
pub(crate) struct Controller<'a> {
    pub(crate) type_: &'a str,
    pub(crate) name: &'a str,
}

#[derive(Serialize)]
pub(crate) struct MatchExpression<'a> {
    key: &'a str,
    operator: &'a str,
    values: Vec<&'a str>,
}

impl<'a> MatchExpression<'a> {
    fn from_requirement(requirement: &'a LabelSelectorRequirement) -> Self {
        Self {
            key: &requirement.key,
            operator: &requirement.operator,
            values: requirement
                .values
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(String::as_str)
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Selector<'a> {
    match_labels: BTreeMap<&'a str, &'a str>,
    match_expressions: Vec<MatchExpression<'a>>,
}

impl<'a> Selector<'a> {
    pub fn from_label_selector(selector: &'a LabelSelector) -> Self {
        Self {
            match_labels: selector
                .match_labels
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect())
                .unwrap_or_default(),
            match_expressions: selector
                .match_expressions
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(MatchExpression::from_requirement)
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ThinContainers<'a> {
    images: BTreeSet<&'a str>,
    names: Vec<&'a str>,
}

impl<'a> ThinContainers<'a> {
    pub fn from_pods(pods: impl Iterator<Item = &'a Arc<Pod>>) -> Self {
        let mut images = BTreeSet::new();
        let mut names = Vec::new();
        for pod in pods {
            if let Some(statuses) =
                pod.status.as_ref().and_then(|s| s.container_statuses.as_ref())
            {
                for status in statuses {
                    images.insert(status.image.as_str());
                    names.push(status.name.as_str());
                }
            }
        }
        Self { images, names }
    }
}

/// Parse a Kubernetes quantity string.
///
/// This is almost a direct port of the Python version.
pub fn parse_quantity(quantity: &str) -> Option<f64> {
    for (unit, factor) in [
        ("Ki", 1024.0),
        ("Mi", f64::powi(1024.0, 2)),
        ("Gi", f64::powi(1024.0, 3)),
        ("Ti", f64::powi(1024.0, 4)),
        ("Pi", f64::powi(1024.0, 5)),
        ("Ei", f64::powi(1024.0, 6)),
        ("K", 1e3),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
        ("m", 1e-3),
        ("u", 1e-6),
        ("n", 1e-9),
    ] {
        if let Some(value_str) = quantity.strip_suffix(unit)
            && let Ok(value) = value_str.parse::<f64>()
        {
            return Some(value * factor);
        }
    }
    match quantity.parse::<f64>() {
        Ok(float) => Some(float),
        Err(e) => {
            debug!(error = %e, %quantity, "could not parse quantity");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quantity_binary_units() {
        assert_eq!(parse_quantity("1Ki"), Some(1024.0));
        assert_eq!(parse_quantity("2Mi"), Some(2.0 * 1024.0 * 1024.0));
    }

    #[test]
    fn parse_quantity_small_si_units() {
        assert_eq!(parse_quantity("3G"), Some(3.0 * 1e9));
        assert_eq!(parse_quantity("62e3"), Some(62.0 * 1e3));
        assert_eq!(parse_quantity("3u"), Some(3.0 * 1e-6));
        assert_eq!(parse_quantity("4n"), Some(4.0 * 1e-9));
        assert_eq!(
            parse_quantity("42.e3Ti"),
            Some(42.0 * 1e3 * f64::powi(1024.0, 4))
        );
    }

    #[test]
    fn parse_quantity_bare_numbers() {
        assert_eq!(parse_quantity("1"), Some(1.0));
        assert_eq!(parse_quantity("2.2"), Some(2.2));
    }

    #[test]
    fn parse_quantity_invalid_is_none() {
        assert!(parse_quantity("hello").is_none());
        assert!(parse_quantity("").is_none());
        assert!(parse_quantity(" 45").is_none());
        assert!(parse_quantity("Ki").is_none());
        assert!(parse_quantity("1 Ki").is_none());
        assert!(parse_quantity("1fb").is_none());
    }
}
