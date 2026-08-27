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

/// Controls which namespaces produce namespaced piggyback hosts.
#[derive(Clone, Default)]
pub struct NamespaceFilter {
    include_patterns: Vec<Regex>,
    exclude_patterns: Vec<Regex>,
}

impl NamespaceFilter {
    pub fn new(include_patterns: Vec<Regex>, exclude_patterns: Vec<Regex>) -> Self {
        Self {
            include_patterns,
            exclude_patterns,
        }
    }

    pub fn is_included(&self, namespace: &str) -> bool {
        (self.include_patterns.is_empty()
            || self
                .include_patterns
                .iter()
                .any(|pattern| pattern.is_match(namespace)))
            && !self
                .exclude_patterns
                .iter()
                .any(|pattern| pattern.is_match(namespace))
    }
}

#[derive(Clone)]
pub struct HostSettings {
    pub cluster_name: String,
    pub cluster_host_name: String,
    pub annotation_key_pattern: AnnotationKeyPattern,
    pub excluded_node_role_patterns: Vec<Regex>,
    pub namespace_filter: NamespaceFilter,
    pub always_emitted: AlwaysEmitted,
    /// Whether CronJob-owned Pods may become standalone piggyback hosts.
    pub include_cronjob_pods: bool,
    /// Whether to emit PVC-related sections on Pods and workload hosts.
    pub emit_pvc_sections: bool,
    // Not configuration but belongs with other static facts about the cluster
    pub cluster_version: String,
}

/// The roles of a node, derived from its `node-role.kubernetes.io/<role>`
/// labels. A node can carry any number of roles, including none at all.
pub fn node_roles(node: &Node) -> impl Iterator<Item = &str> {
    node.metadata
        .labels
        .iter()
        .flatten()
        .filter_map(|(label, _)| label.strip_prefix("node-role.kubernetes.io/"))
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

        node_roles(node).any(|role| {
            self.excluded_node_role_patterns
                .iter()
                .any(|p| p.is_match(role))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{host_settings, node, node_with_roles, s};

    fn host_settings_with_excluded_roles(patterns: Vec<Regex>) -> HostSettings {
        HostSettings {
            annotation_key_pattern: AnnotationKeyPattern::IgnoreAll,
            excluded_node_role_patterns: patterns,
            ..host_settings()
        }
    }

    #[test]
    fn test_node_roles() {
        let mut node = node("node01");
        assert_eq!(node_roles(&node).count(), 0);

        node.metadata.labels = Some(BTreeMap::from([
            (s("node-role.kubernetes.io/control-plane"), s("")),
            (s("node-role.kubernetes.io/worker"), s("")),
            // Neither of these is a role, despite looking the part.
            (s("node-role.kubernetes.io"), s("")),
            (s("kubernetes.io/arch"), s("amd64")),
        ]));
        assert_eq!(
            node_roles(&node).collect::<Vec<_>>(),
            vec!["control-plane", "worker"]
        );
    }

    #[test]
    fn namespace_filter_uses_infix_patterns() {
        let include = NamespaceFilter::new(vec![Regex::new("prod").unwrap()], vec![]);
        assert!(include.is_included("my-production"));
        assert!(!include.is_included("development"));

        let exclude = NamespaceFilter::new(vec![], vec![Regex::new("^kube-").unwrap()]);
        assert!(!exclude.is_included("kube-system"));
        assert!(exclude.is_included("my-kube-project"));

        assert!(NamespaceFilter::default().is_included("anything"));
    }

    #[test]
    fn test_node_exclusion() {
        let pattern = Regex::new("control-plane").unwrap();
        let pattern_exact = Regex::new("^control-plane$").unwrap();

        let exclude_c_plane = host_settings_with_excluded_roles(vec![pattern]);
        let exclude_exact = host_settings_with_excluded_roles(vec![pattern_exact]);
        let no_exclusion = host_settings_with_excluded_roles(vec![]);

        let control_plane = node_with_roles("control-1", &["control-plane"]);
        let worker = node_with_roles("worker-1", &["worker"]);
        let silly_control_plane_node = node_with_roles("silly-1", &["silly-control-plane-node"]);

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
