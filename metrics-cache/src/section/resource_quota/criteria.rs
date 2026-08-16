//! ResourceQuota scope parsing and pod-matching criteria.

use k8s_openapi::api::core::v1::{Pod, ResourceQuota, ScopedResourceSelectorRequirement};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("unsupported quota scope: {0}")]
    UnsupportedQuotaScope(String),
    #[error("unsupported scope operator: {0}")]
    UnsupportedScopeOperator(String),
}

/// The scopes of a [`ScopeSelectorExpression`] that rustik evaluates.
///
/// These come from Kubernetes, the list is available here:
/// <https://kubernetes.io/docs/concepts/policy/resource-quotas/#quota-scopesscope>
///
/// These are intentionally elided:
///
/// - `CrossNamespacePodAffinity`: matches pods that mention other namespaces
///   in an affinity term. Kubernetes only permits pods in `hard` for this
///   scope, so there are no CPU or memory limits for usage aggregation to be
///   compared against. Could be implemented for completeness, but does not
///   provide much for us.
///
/// - `VolumeAttributesClass`: scopes `PersistentVolumeClaim`s rather than pods,
///   so it does not help with pod usage aggregation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuotaScope {
    BestEffort,
    NotBestEffort,
    Terminating,
    NotTerminating,
    PriorityClass,
}

impl QuotaScope {
    /// Return whether a pod belongs to this Kubernetes quota scope.
    fn matches(self, pod: &Pod) -> bool {
        let spec = pod.spec.as_ref();
        let status = pod.status.as_ref();
        match self {
            Self::PriorityClass => spec
                .and_then(|s| s.priority_class_name.as_deref())
                .is_some(),
            Self::Terminating => spec.and_then(|s| s.active_deadline_seconds).is_some(),
            Self::NotTerminating => spec.and_then(|s| s.active_deadline_seconds).is_none(),
            Self::BestEffort => status.and_then(|s| s.qos_class.as_deref()) == Some("BestEffort"),
            Self::NotBestEffort => {
                status.and_then(|s| s.qos_class.as_deref()) != Some("BestEffort")
            }
        }
    }
}

impl TryFrom<&str> for QuotaScope {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "BestEffort" => Ok(Self::BestEffort),
            "NotBestEffort" => Ok(Self::NotBestEffort),
            "Terminating" => Ok(Self::Terminating),
            "NotTerminating" => Ok(Self::NotTerminating),
            "PriorityClass" => Ok(Self::PriorityClass),
            other => Err(Error::UnsupportedQuotaScope(other.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

impl TryFrom<&str> for ScopeOperator {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "In" => Ok(Self::In),
            "NotIn" => Ok(Self::NotIn),
            "Exists" => Ok(Self::Exists),
            "DoesNotExist" => Ok(Self::DoesNotExist),
            other => Err(Error::UnsupportedScopeOperator(other.to_owned())),
        }
    }
}

/// A parsed scope-selector requirement. `values` borrows from the source
/// [`ResourceQuota`], so parse criteria once and reuse them while matching
/// pods.
#[derive(Debug)]
struct ScopeSelectorExpression<'a> {
    scope: QuotaScope,
    operator: ScopeOperator,
    values: &'a [String],
}

impl ScopeSelectorExpression<'_> {
    /// Evaluate this selector requirement against a pod.
    ///
    /// Only `PriorityClass` uses the operator and values. Kubernetes validates
    /// the other scopes to ordinary scope-membership checks.
    fn matches(&self, pod: &Pod) -> bool {
        fn handle_priority_class(pod: &Pod, operator: ScopeOperator, values: &[String]) -> bool {
            let priority_class = pod
                .spec
                .as_ref()
                .and_then(|s| s.priority_class_name.as_deref());

            match operator {
                ScopeOperator::Exists => priority_class.is_some(),
                ScopeOperator::DoesNotExist => priority_class.is_none(),
                ScopeOperator::In => {
                    priority_class.is_some_and(|pc| values.iter().any(|v| v == pc))
                }
                ScopeOperator::NotIn => {
                    priority_class.is_none_or(|pc| values.iter().all(|v| v != pc))
                }
            }
        }

        match self.scope {
            // Only PriorityClass is not API-validated, these 4 are.
            QuotaScope::BestEffort
            | QuotaScope::NotBestEffort
            | QuotaScope::Terminating
            | QuotaScope::NotTerminating => self.scope.matches(pod),
            QuotaScope::PriorityClass => handle_priority_class(pod, self.operator, self.values),
        }
    }
}

impl<'a> TryFrom<&'a ScopedResourceSelectorRequirement> for ScopeSelectorExpression<'a> {
    type Error = Error;

    fn try_from(req: &'a ScopedResourceSelectorRequirement) -> Result<Self, Self::Error> {
        Ok(Self {
            scope: QuotaScope::try_from(req.scope_name.as_str())?,
            operator: ScopeOperator::try_from(req.operator.as_str())?,
            values: req.values.as_deref().unwrap_or_default(),
        })
    }
}

/// The pod predicate defined by a [`ResourceQuota`]. All scopes and selector
/// expressions are ANDed; empty criteria match every pod.
#[derive(Debug, Default)]
pub(crate) struct ResourceQuotaCriteria<'a> {
    scopes: Vec<QuotaScope>,
    expressions: Vec<ScopeSelectorExpression<'a>>,
}

impl ResourceQuotaCriteria<'_> {
    /// Return whether a pod satisfies every scope and selector expression.
    pub(crate) fn matches(&self, pod: &Pod) -> bool {
        let all_scopes_match = self.scopes.iter().copied().all(|scope| scope.matches(pod));

        let all_expressions_match = self.expressions.iter().all(|e| e.matches(pod));

        all_scopes_match && all_expressions_match
    }
}

impl<'a> TryFrom<&'a ResourceQuota> for ResourceQuotaCriteria<'a> {
    type Error = Error;

    fn try_from(quota: &'a ResourceQuota) -> Result<Self, Self::Error> {
        let Some(spec) = &quota.spec else {
            return Ok(ResourceQuotaCriteria::default());
        };
        let scopes = spec
            .scopes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|scope| QuotaScope::try_from(scope.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let expressions = spec
            .scope_selector
            .as_ref()
            .and_then(|s| s.match_expressions.as_deref())
            .unwrap_or_default()
            .iter()
            .map(ScopeSelectorExpression::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResourceQuotaCriteria {
            scopes,
            expressions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{ResourceQuotaSpec, ScopeSelector};

    use crate::test_support::*;

    #[test]
    fn resource_quota_criteria_from_resource_quota() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec {
                scopes: Some(vec![s("NotTerminating"), s("NotBestEffort")]),
                scope_selector: Some(ScopeSelector {
                    match_expressions: Some(vec![ScopedResourceSelectorRequirement {
                        scope_name: s("PriorityClass"),
                        operator: s("In"),
                        values: Some(vec![s("high"), s("medium")]),
                    }]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let criteria = ResourceQuotaCriteria::try_from(&quota).unwrap();

        assert_eq!(
            criteria.scopes,
            vec![QuotaScope::NotTerminating, QuotaScope::NotBestEffort]
        );
        let [expression] = criteria.expressions.as_slice() else {
            panic!("expected exactly one scope selector expression");
        };
        assert_eq!(expression.scope, QuotaScope::PriorityClass);
        assert_eq!(expression.operator, ScopeOperator::In);
        assert_eq!(expression.values, &[s("high"), s("medium")]);
    }

    #[test]
    fn resource_quota_without_scopes_matches_every_pod() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec::default()),
            ..Default::default()
        };
        let criteria = ResourceQuotaCriteria::try_from(&quota).unwrap();

        assert!(criteria.scopes.is_empty());
        assert!(criteria.expressions.is_empty());
        assert!(criteria.matches(&pod("yay-its-a-pod", Some("node"))));
    }

    #[test]
    fn resource_quota_criteria_matches_all_scopes_and_expressions() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec {
                scopes: Some(vec![s("NotTerminating")]),
                scope_selector: Some(ScopeSelector {
                    match_expressions: Some(vec![ScopedResourceSelectorRequirement {
                        scope_name: s("PriorityClass"),
                        operator: s("In"),
                        values: Some(vec![s("high")]),
                    }]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let criteria = ResourceQuotaCriteria::try_from(&quota).unwrap();
        let mut pod = pod("yay-its-a-pod", Some("node"));

        pod.spec.as_mut().unwrap().priority_class_name = Some(s("high"));
        assert!(criteria.matches(&pod));

        pod.spec.as_mut().unwrap().priority_class_name = Some(s("low"));
        assert!(!criteria.matches(&pod));

        let spec = pod.spec.as_mut().unwrap();
        spec.priority_class_name = Some(s("high"));
        spec.active_deadline_seconds = Some(1);
        assert!(!criteria.matches(&pod));
    }

    #[test]
    fn resource_quota_criteria_matches_all_scopes() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec {
                scopes: Some(vec![s("Terminating"), s("BestEffort")]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let criteria = ResourceQuotaCriteria::try_from(&quota).unwrap();
        let mut pod = pod("yay-its-a-pod", Some("node"));
        pod.spec.as_mut().unwrap().active_deadline_seconds = Some(1);
        pod.status.get_or_insert_with(Default::default).qos_class = Some(s("BestEffort"));

        assert!(criteria.matches(&pod));

        pod.status.as_mut().unwrap().qos_class = Some(s("Guaranteed"));
        assert!(!criteria.matches(&pod));

        pod.status.as_mut().unwrap().qos_class = Some(s("BestEffort"));
        pod.spec.as_mut().unwrap().active_deadline_seconds = None;
        assert!(!criteria.matches(&pod));
    }

    #[test]
    fn resource_quota_criteria_matches_all_scope_selector_expressions() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec {
                scope_selector: Some(ScopeSelector {
                    match_expressions: Some(vec![
                        ScopedResourceSelectorRequirement {
                            scope_name: s("Terminating"),
                            operator: s("Exists"),
                            values: None,
                        },
                        ScopedResourceSelectorRequirement {
                            scope_name: s("BestEffort"),
                            operator: s("Exists"),
                            values: None,
                        },
                    ]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let criteria = ResourceQuotaCriteria::try_from(&quota).unwrap();
        let mut pod = pod("yay-its-a-pod", Some("node"));
        pod.spec.as_mut().unwrap().active_deadline_seconds = Some(1);
        pod.status.get_or_insert_with(Default::default).qos_class = Some(s("BestEffort"));

        assert!(criteria.matches(&pod));

        pod.status.as_mut().unwrap().qos_class = Some(s("Guaranteed"));
        assert!(!criteria.matches(&pod));

        pod.status.as_mut().unwrap().qos_class = Some(s("BestEffort"));
        pod.spec.as_mut().unwrap().active_deadline_seconds = None;
        assert!(!criteria.matches(&pod));
    }

    #[test]
    fn unsupported_cross_namespace_pod_affinity_scope_is_rejected() {
        let quota = ResourceQuota {
            spec: Some(ResourceQuotaSpec {
                scopes: Some(vec![s("CrossNamespacePodAffinity")]),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(matches!(
            ResourceQuotaCriteria::try_from(&quota),
            Err(Error::UnsupportedQuotaScope(scope)) if scope == "CrossNamespacePodAffinity"
        ));
    }

    #[test]
    fn default_pod_scope_membership() {
        let pod = pod("yay-its-a-pod", Some("node"));
        for false_scope in [
            QuotaScope::PriorityClass,
            QuotaScope::Terminating,
            QuotaScope::BestEffort,
        ] {
            assert!(!false_scope.matches(&pod));
        }

        for true_scope in [QuotaScope::NotTerminating, QuotaScope::NotBestEffort] {
            assert!(true_scope.matches(&pod));
        }
    }

    #[test]
    fn pod_scope_best_effort() {
        let mut pod = pod("yay-its-a-pod", Some("node"));
        pod.status.get_or_insert_with(Default::default).qos_class = Some(s("BestEffort"));
        assert!(QuotaScope::BestEffort.matches(&pod));
        assert!(!QuotaScope::NotBestEffort.matches(&pod));
    }

    #[test]
    fn pod_scope_terminating() {
        let mut pod = pod("yay-its-a-pod", Some("node"));
        pod.spec.as_mut().unwrap().active_deadline_seconds = Some(1);
        assert!(QuotaScope::Terminating.matches(&pod));
        assert!(!QuotaScope::NotTerminating.matches(&pod));
    }

    #[test]
    fn pod_scope_priority_class() {
        let mut pod = pod("yay-its-a-pod", Some("node"));
        pod.spec.as_mut().unwrap().priority_class_name = Some(s("high"));
        assert!(QuotaScope::PriorityClass.matches(&pod));
    }

    #[test]
    fn matches_scope_selector_expression_priority_class() {
        fn expression_matches(pod: &Pod, operator: ScopeOperator, values: &[String]) -> bool {
            ScopeSelectorExpression {
                scope: QuotaScope::PriorityClass,
                operator,
                values,
            }
            .matches(pod)
        }

        let mut pod = pod("yay-its-a-pod", Some("node"));

        // At first it does not exist and as such should match DoesNotExist
        assert!(expression_matches(&pod, ScopeOperator::DoesNotExist, &[]));
        assert!(!expression_matches(&pod, ScopeOperator::Exists, &[]));

        // And inclusion
        assert!(!expression_matches(
            &pod,
            ScopeOperator::In,
            &[s("foo"), s("high"), s("bar")]
        ));
        assert!(expression_matches(
            &pod,
            ScopeOperator::NotIn,
            &[s("foo"), s("bar")]
        ));

        // But then it does exist
        pod.spec.as_mut().unwrap().priority_class_name = Some(s("high"));

        // Existence
        assert!(expression_matches(&pod, ScopeOperator::Exists, &[]));
        assert!(!expression_matches(&pod, ScopeOperator::DoesNotExist, &[]));

        // Inclusion
        assert!(expression_matches(
            &pod,
            ScopeOperator::In,
            &[s("foo"), s("high"), s("bar")]
        ));
        assert!(!expression_matches(
            &pod,
            ScopeOperator::In,
            &[s("foo"), s("bar")]
        ));
        assert!(expression_matches(
            &pod,
            ScopeOperator::NotIn,
            &[s("foo"), s("bar")]
        ));
        assert!(!expression_matches(
            &pod,
            ScopeOperator::NotIn,
            &[s("foo"), s("high"), s("bar")]
        ));
    }
}
