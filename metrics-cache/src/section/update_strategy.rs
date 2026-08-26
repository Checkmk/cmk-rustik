use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde::Serialize;

use crate::section::Section;

#[derive(Serialize)]
#[serde(tag = "type_")]
enum UpdateStrategy {
    OnDelete,
    Recreate,
    StatefulSetRollingUpdate {
        partition: i32,
        max_unavailable: Option<IntOrString>,
    },
    RollingUpdate {
        max_surge: IntOrString,
        max_unavailable: IntOrString,
    },
}

/// Update strategy. (`kube_update_strategy_v1`)
#[derive(Serialize)]
pub(crate) struct KubeUpdateStrategyV1 {
    strategy: UpdateStrategy,
}

impl KubeUpdateStrategyV1 {
    pub(crate) fn from_statefulset(statefulset: &StatefulSet) -> Option<Self> {
        let strategy = statefulset.spec.as_ref()?.update_strategy.as_ref()?;
        let strategy = match strategy.type_.as_deref()? {
            "OnDelete" => UpdateStrategy::OnDelete,
            "RollingUpdate" => {
                let rolling = strategy.rolling_update.as_ref()?;
                UpdateStrategy::StatefulSetRollingUpdate {
                    partition: rolling.partition?,
                    max_unavailable: rolling.max_unavailable.clone(),
                }
            }
            _ => return None,
        };
        Some(Self { strategy })
    }

    pub(crate) fn from_deployment(deployment: &Deployment) -> Option<Self> {
        let strategy = deployment.spec.as_ref()?.strategy.as_ref()?;
        let strategy = match strategy.type_.as_deref()? {
            "Recreate" => UpdateStrategy::Recreate,
            "RollingUpdate" => {
                let rolling = strategy.rolling_update.as_ref()?;
                UpdateStrategy::RollingUpdate {
                    max_surge: rolling.max_surge.clone()?,
                    max_unavailable: rolling.max_unavailable.clone()?,
                }
            }
            _ => return None,
        };
        Some(Self { strategy })
    }

    pub(crate) fn from_daemonset(daemonset: &DaemonSet) -> Option<Self> {
        let strategy = daemonset.spec.as_ref()?.update_strategy.as_ref()?;
        let strategy = match strategy.type_.as_deref()? {
            "OnDelete" => UpdateStrategy::OnDelete,
            "RollingUpdate" => {
                let rolling = strategy.rolling_update.as_ref()?;
                UpdateStrategy::RollingUpdate {
                    max_surge: rolling.max_surge.clone()?,
                    max_unavailable: rolling.max_unavailable.clone()?,
                }
            }
            _ => return None,
        };
        Some(Self { strategy })
    }
}

impl Section for KubeUpdateStrategyV1 {
    const NAME: &'static str = "kube_update_strategy_v1";
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::apps::v1::{
        DaemonSetUpdateStrategy, DeploymentSpec, DeploymentStrategy, RollingUpdateDaemonSet,
        RollingUpdateStatefulSetStrategy, StatefulSetSpec, StatefulSetUpdateStrategy,
    };
    use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

    use crate::test_support::daemonset;

    #[test]
    fn deployment_update_strategy() {
        let deployment = Deployment {
            spec: Some(DeploymentSpec {
                strategy: Some(DeploymentStrategy {
                    type_: Some("Recreate".to_owned()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        insta::assert_json_snapshot!(KubeUpdateStrategyV1::from_deployment(&deployment));
    }

    #[test]
    fn daemonset_update_strategy() {
        let mut daemonset = daemonset("node-agent");
        daemonset.spec.as_mut().unwrap().update_strategy = Some(DaemonSetUpdateStrategy {
            type_: Some("RollingUpdate".to_owned()),
            rolling_update: Some(RollingUpdateDaemonSet {
                max_surge: Some(IntOrString::Int(0)),
                max_unavailable: Some(IntOrString::Int(1)),
            }),
        });

        insta::assert_json_snapshot!(KubeUpdateStrategyV1::from_daemonset(&daemonset));
    }

    #[test]
    fn statefulset_update_strategy() {
        let statefulset = StatefulSet {
            spec: Some(StatefulSetSpec {
                update_strategy: Some(StatefulSetUpdateStrategy {
                    type_: Some("RollingUpdate".to_owned()),
                    rolling_update: Some(RollingUpdateStatefulSetStrategy {
                        partition: Some(1),
                        max_unavailable: Some(IntOrString::Int(1)),
                    }),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        insta::assert_json_snapshot!(KubeUpdateStrategyV1::from_statefulset(&statefulset));
    }
}
