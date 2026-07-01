use k8s_openapi::api::core::v1::Node;
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
    pub excluded_node_role_patterns: Vec<String>,
}

impl HostSettings {
    /// Given a node, determine if it should be excluded from cluster metrics.
    ///
    /// This is primarily based on the roles the node has. We allow for a
    /// command-line argument `--excluded-node-role-patterns` which is a list of
    /// role substrings we exclude for nodes in cluster-level computations.
    pub fn is_node_excluded(&self, node: &Node) -> bool {
        // If no filter was given, don't exclude any nodes
        if self.excluded_node_role_patterns.is_empty() {
            return false;
        }

        let Some(labels) = &node.metadata.labels else {
            return false;
        };
        let roles: Vec<&str> = labels
            .keys()
            .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
            .collect();
        roles.iter().any(|r| {
            self.excluded_node_role_patterns
                .iter()
                .any(|p| r.contains(p))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Node;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn host_settings_with_excluded_roles(patterns: Vec<String>) -> HostSettings {
        HostSettings {
            cluster_name: "test-cluster".to_string(),
            cluster_host_name: "test-host".to_string(),
            annotation_key_pattern: AnnotationKeyPattern::IgnoreAll,
            excluded_node_role_patterns: patterns,
        }
    }

    fn node_with_role(role: &str) -> Node {
        Node {
            metadata: ObjectMeta {
                labels: Some(BTreeMap::from([(
                    format!("node-role.kubernetes.io/{role}"),
                    "".to_string(),
                )])),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_node_exclusion() {
        let exclude_c_plane = host_settings_with_excluded_roles(vec!["control-plane".to_string()]);
        let no_exclusion = host_settings_with_excluded_roles(vec![]);
        let control_plane = node_with_role("control-plane");
        let worker = node_with_role("worker");
        let silly_control_plane_node = node_with_role("silly-control-plane-node");

        assert!(exclude_c_plane.is_node_excluded(&control_plane));
        assert!(!exclude_c_plane.is_node_excluded(&worker));
        assert!(exclude_c_plane.is_node_excluded(&silly_control_plane_node));
        assert!(!no_exclusion.is_node_excluded(&control_plane));
        assert!(!no_exclusion.is_node_excluded(&worker));
        assert!(!no_exclusion.is_node_excluded(&silly_control_plane_node));
    }
}
