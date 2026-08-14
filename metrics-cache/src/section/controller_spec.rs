use serde::Serialize;

use crate::section::Section;

/// Controller spec. (`kube_controller_spec_v1`)
#[derive(Serialize)]
pub(crate) struct KubeControllerSpecV1 {
    pub min_ready_seconds: i32,
}

impl KubeControllerSpecV1 {
    pub(crate) fn new(min_ready_seconds: Option<i32>) -> Self {
        Self {
            min_ready_seconds: min_ready_seconds.unwrap_or(0),
        }
    }
}

impl Section for KubeControllerSpecV1 {
    const NAME: &'static str = "kube_controller_spec_v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kube_controller_spec_v1_present() {
        insta::assert_json_snapshot!(KubeControllerSpecV1::new(Some(30)));
    }

    #[test]
    fn kube_controller_spec_v1_absent() {
        insta::assert_json_snapshot!(KubeControllerSpecV1::new(None));
    }
}
