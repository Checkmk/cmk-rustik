use std::collections::BTreeMap;

use crate::sections::Section;

pub(crate) struct WriteableSection {
    pub piggyback_hostname: String,
    pub name: &'static str,
    /// The JSON serialized body of the section
    pub body: String,
}

pub(crate) struct SectionError {
    pub name: &'static str,
    pub source: serde_json::Error,
}

impl WriteableSection {
    pub fn of<S: Section>(piggyback_hostname: String, section: &S) -> Result<Self, SectionError> {
        let body = serde_json::to_string(section).map_err(|source| SectionError {
            name: S::NAME,
            source,
        })?;
        Ok(Self {
            piggyback_hostname,
            name: S::NAME,
            body,
        })
    }
}

/// Take a collection of [`WriteableSection`]s and render them.
pub(crate) fn frame(sections: Vec<WriteableSection>) -> String {
    let mut by_host: BTreeMap<&str, Vec<&WriteableSection>> = BTreeMap::new();
    for section in &sections {
        by_host
            .entry(&section.piggyback_hostname)
            .or_default()
            .push(section);
    }
    let mut out = String::new();
    for (host, host_sections) in by_host {
        out.push_str(&format!("<<<<{host}>>>>\n"));
        for section in host_sections {
            out.push_str(&format!("<<<{}:sep(0)>>>\n", section.name));
            out.push_str(&section.body);
            out.push('\n');
        }
        out.push_str("<<<<>>>>\n");
    }
    out
}
