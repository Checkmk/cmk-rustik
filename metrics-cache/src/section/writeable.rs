use bytes::Bytes;
use std::collections::BTreeMap;
use std::io::Write;

use crate::section::Section;

#[derive(Debug)]
pub struct WriteableSection {
    pub piggyback_hostname: String,
    pub body: SectionBody,
}

#[derive(Debug)]
pub enum SectionBody {
    Json { name: &'static str, body: String },
    Raw(Bytes),
}

#[derive(Debug)]
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
            body: SectionBody::Json {
                name: S::NAME,
                body,
            },
        })
    }

    pub fn raw(piggyback_hostname: &str, raw: Bytes) -> Self {
        Self {
            piggyback_hostname: piggyback_hostname.to_string(),
            body: SectionBody::Raw(raw),
        }
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
            match &section.body {
                SectionBody::Json { name, body } => {
                    writeln!(writer, "<<<{name}:sep(0)>>>")?;
                    writer.write_all(body.as_bytes())?;
                    writeln!(writer)?;
                }
                SectionBody::Raw(raw) => {
                    writer.write_all(raw)?;
                    if !raw.ends_with(b"\n") && !raw.is_empty() {
                        writeln!(writer)?;
                    }
                }
            }
        }
        if !bare {
            writeln!(writer, "<<<<>>>>")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Serialize)]
    struct TestSectionV1 {
        value: u8,
    }

    impl Section for TestSectionV1 {
        const NAME: &'static str = "test_section_v1";
    }

    #[test]
    fn frame_piggyback_and_bare() {
        let sections = vec![
            WriteableSection::of("piggyback-host", &TestSectionV1 { value: 26 }).unwrap(),
            WriteableSection::of("", &TestSectionV1 { value: 42 }).unwrap(),
        ];
        let mut out = Vec::new();
        frame(&mut out, sections).unwrap();
        insta::assert_snapshot!(String::from_utf8(out).unwrap());
    }

    #[test]
    fn frame_raw_and_json_mixed() {
        let sections = vec![
            WriteableSection::of("json-host", &TestSectionV1 { value: 7 }).unwrap(),
            WriteableSection::raw(
                "raw-host",
                Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0\n"),
            ),
        ];
        let mut out = Vec::new();
        frame(&mut out, sections).unwrap();
        insta::assert_snapshot!(String::from_utf8(out).unwrap());
    }

    #[test]
    fn frame_raw_without_trailing_newline_gets_one_added() {
        let sections = vec![WriteableSection::raw(
            "raw-host",
            Bytes::from_static(b"<<<check_mk>>>\nVersion: 2.5.0"),
        )];
        let mut out = Vec::new();
        frame(&mut out, sections).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<<<<raw-host>>>>\n<<<check_mk>>>\nVersion: 2.5.0\n<<<<>>>>\n"
        );
    }

    #[test]
    fn frame_raw_empty_produces_no_spurious_blank_line() {
        let sections = vec![WriteableSection::raw("raw-host", Bytes::new())];
        let mut out = Vec::new();
        frame(&mut out, sections).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<<<<raw-host>>>>\n<<<<>>>>\n"
        );
    }
}
