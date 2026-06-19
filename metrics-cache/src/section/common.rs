use serde::Serialize;
use std::collections::BTreeMap;

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
