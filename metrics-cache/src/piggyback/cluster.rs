use crate::host_settings::HostSettings;
use crate::piggyback::PiggybackHost;
use crate::section::writeable::{SectionError, WriteableSection};
use crate::snapshot::Snapshot;

pub struct Cluster<'a> {
    _snapshot: &'a Snapshot,
    settings: &'a HostSettings,
}

impl<'a> Cluster<'a> {
    pub fn new(_snapshot: &'a Snapshot, settings: &'a HostSettings) -> Cluster<'a> {
        Cluster {
            _snapshot,
            settings,
        }
    }
}

impl PiggybackHost for Cluster<'_> {
    fn emit(&self) -> Vec<Result<WriteableSection, SectionError>> {
        Vec::new()
    }
}
