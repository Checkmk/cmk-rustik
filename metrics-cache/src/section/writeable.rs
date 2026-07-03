use std::collections::BTreeMap;
use std::io::Write;

use crate::section::Section;

pub struct WriteableSection {
    pub piggyback_hostname: String,
    pub name: &'static str,
    /// The JSON serialized body of the section
    pub body: String,
}

pub struct SectionError {
    pub name: &'static str,
    pub source: serde_json::Error,
}

impl WriteableSection {
    pub fn of<S: Section>(piggyback_hostname: &str, section: &S) -> Result<Self, SectionError> {
        let body = serde_json::to_string(section).map_err(|source| SectionError {
            name: S::NAME,
            source,
        })?;
        Ok(Self {
            piggyback_hostname: piggyback_hostname.to_string(),
            name: S::NAME,
            body,
        })
    }
}

/// Take a collection of [`WriteableSection`]s and render them into something
/// that can write them out.
pub fn frame<W: Write>(writer: &mut W, sections: Vec<WriteableSection>) -> std::io::Result<()> {
    let mut by_host: BTreeMap<&str, Vec<&WriteableSection>> = BTreeMap::new();
    for section in &sections {
        by_host
            .entry(&section.piggyback_hostname)
            .or_default()
            .push(section);
    }
    for (host, host_sections) in by_host {
        let bare = host.is_empty();
        if !bare {
            writeln!(writer, "<<<<{host}>>>>")?;
        }
        for section in host_sections {
            writeln!(writer, "<<<{}:sep(0)>>>", section.name)?;
            writer.write_all(section.body.as_bytes())?;
            writeln!(writer)?;
        }
        if !bare {
            writeln!(writer, "<<<<>>>>")?;
        }
    }
    Ok(())
}
