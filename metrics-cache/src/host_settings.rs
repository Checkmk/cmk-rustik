use regex::Regex;
use std::collections::BTreeMap;

#[derive(Clone)]
pub enum AnnotationKeyPattern {
    IgnoreAll,
    ImportAll,
    Pattern(Regex),
}

impl AnnotationKeyPattern {
    pub fn new(import_all_annotations: bool, annotation_key_pattern: Option<Regex>) -> Self {
        match (import_all_annotations, annotation_key_pattern) {
            (false, None) => Self::IgnoreAll,
            (true, _) => Self::ImportAll,
            (_, Some(re)) => Self::Pattern(re),
        }
    }

    pub fn should_import(&self, input: &str) -> bool {
        match self {
            Self::IgnoreAll => false,
            Self::ImportAll => true,
            Self::Pattern(re) => re.is_match(input),
        }
    }

    pub fn filter<'a>(&self, map: &'a BTreeMap<String, String>) -> BTreeMap<&'a str, &'a str> {
        map.iter()
            .filter(|(k, _)| self.should_import(k.as_str()))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

#[derive(Clone)]
pub struct HostSettings {
    pub cluster_name: String,
    pub cluster_host_name: String,
    pub annotation_key_pattern: AnnotationKeyPattern,
}
