use k8s_openapi::api::core::v1::Node;
use regex::Regex;
use std::collections::BTreeMap;

use crate::cli_args::CliArgs;

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

/// Holds information (based on CLI arguments) about which resources should
/// always be emitted. The ones which are false here will only be emitted if
/// explicitly promoted to piggyback host via an annotation.
#[derive(Clone, Default)]
pub struct AlwaysEmitted {
    pub pods: bool,
    pub namespaces: bool,
    pub nodes: bool,
    pub deployments: bool,
    pub daemonsets: bool,
    pub statefulsets: bool,
    pub cronjobs: bool,
}

impl AlwaysEmitted {
    pub fn from_cli_args(args: &CliArgs) -> Self {
        Self {
            pods: args.all_pods,
            namespaces: args.all_namespaces,
            nodes: args.all_nodes,
            deployments: args.all_deployments,
            daemonsets: args.all_daemonsets,
            statefulsets: args.all_statefulsets,
            cronjobs: args.all_cronjobs,
        }
    }
}

#[derive(Clone)]
pub struct HostSettings {
    pub cluster_name: String,
    pub cluster_host_name: String,
    pub annotation_key_pattern: AnnotationKeyPattern,
    pub excluded_node_role_patterns: Vec<Regex>,
    pub always_emitted: AlwaysEmitted,
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
                .any(|p| p.is_match(r))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Node;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn host_settings_with_excluded_roles(patterns: Vec<Regex>) -> HostSettings {
        HostSettings {
            cluster_name: "test-cluster".to_string(),
            cluster_host_name: "test-host".to_string(),
            annotation_key_pattern: AnnotationKeyPattern::IgnoreAll,
            excluded_node_role_patterns: patterns,
            always_emitted: AlwaysEmitted::default(),
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
        let pattern = Regex::new("control-plane").unwrap();
        let pattern_exact = Regex::new("^control-plane$").unwrap();

        let exclude_c_plane = host_settings_with_excluded_roles(vec![pattern]);
        let exclude_exact = host_settings_with_excluded_roles(vec![pattern_exact]);
        let no_exclusion = host_settings_with_excluded_roles(vec![]);

        let control_plane = node_with_role("control-plane");
        let worker = node_with_role("worker");
        let silly_control_plane_node = node_with_role("silly-control-plane-node");

        // effectively, substring-search
        assert!(exclude_c_plane.is_node_excluded(&control_plane));
        assert!(!exclude_c_plane.is_node_excluded(&worker));
        assert!(exclude_c_plane.is_node_excluded(&silly_control_plane_node));

        // empty patterns list
        assert!(!no_exclusion.is_node_excluded(&control_plane));
        assert!(!no_exclusion.is_node_excluded(&worker));
        assert!(!no_exclusion.is_node_excluded(&silly_control_plane_node));

        // regex with anchors ^/$ should not match substring
        assert!(exclude_exact.is_node_excluded(&control_plane));
        assert!(!exclude_exact.is_node_excluded(&worker));
        assert!(!exclude_exact.is_node_excluded(&silly_control_plane_node));
    }
}
