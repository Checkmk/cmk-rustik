use serde::Serialize;
use std::collections::BTreeMap;
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
