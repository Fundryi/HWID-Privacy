//! C2 export parsing and comparison. No hardware collection and no file writes.

use super::{eq_ignore_case, pad_right_utf16};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum ReadError {
    Empty(String),
    Read(String),
}

impl ReadError {
    /// Exact C2 error-box body (the UI supplies the icon and title).
    pub fn message(&self) -> &str {
        match self {
            Self::Empty(message) | Self::Read(message) => message,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Export {
    sections: Vec<Section>,
}

#[derive(Debug, PartialEq)]
struct Section {
    title: String,
    rows: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct JsonExport {
    sections: Vec<JsonSection>,
}

#[derive(Deserialize)]
struct JsonSection {
    title: String,
    lines: Vec<String>,
}

/// Reads text exports or the C3 JSON format; metadata and identifier lists are not diff keys.
pub fn read(path: &Path) -> Result<Export, ReadError> {
    let text = std::fs::read_to_string(path).map_err(|error| read_error(path, &error))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let json = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let export = parse(text, json).map_err(|error| read_error(path, &error))?;
    if export
        .sections
        .iter()
        .all(|section| section.rows.is_empty())
    {
        return Err(ReadError::Empty(format!(
            "No hardware values found in: {}",
            path.display()
        )));
    }
    Ok(export)
}

fn read_error(path: &Path, error: &dyn std::fmt::Display) -> ReadError {
    ReadError::Read(format!("Error reading {}: {error}", path.display()))
}

fn parse(text: &str, json: bool) -> Result<Export, serde_json::Error> {
    let mut export = Export::default();
    if json {
        let data: JsonExport = serde_json::from_str(text)?;
        for section in data.sections {
            let index = export.section(&section.title);
            for line in section.lines {
                export.line(index, &line);
            }
        }
    } else {
        let mut current = None;
        for line in text.lines() {
            if let Some(title) = line
                .strip_prefix("===== ")
                .and_then(|line| line.strip_suffix(" ====="))
            {
                current = Some(export.section(title));
            } else if let Some(index) = current {
                export.line(index, line);
            }
        }
    }
    Ok(export)
}

impl Export {
    fn section(&mut self, title: &str) -> usize {
        if let Some(index) = self
            .sections
            .iter()
            .position(|section| eq_ignore_case(&section.title, title))
        {
            return index;
        }
        self.sections.push(Section {
            title: title.to_owned(),
            rows: Vec::new(),
        });
        self.sections.len() - 1
    }

    fn line(&mut self, index: usize, line: &str) {
        let line = line.trim_start_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '│' | '├' | '└' | '─')
        });
        for piece in line.split(" | ") {
            if let Some((label, value)) = piece.split_once(": ") {
                let label = label.trim();
                if label.chars().any(char::is_alphabetic) {
                    self.sections[index]
                        .rows
                        .push((label.to_owned(), value.trim().to_owned()));
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Changed,
    Removed,
    Added,
    Same,
}

impl Kind {
    fn text(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Removed => "removed",
            Self::Added => "added",
            Self::Same => "same",
        }
    }
}

/// Ready-to-display result; both strings use the spec's wording and the well uses CRLF.
pub struct Comparison {
    pub summary: String,
    pub text: String,
}

/// Compares by section title, exact label and occurrence, preserving order within each kind.
pub fn compare(
    before: &Export,
    after: &Export,
    before_path: &Path,
    after_path: &Path,
) -> Comparison {
    let mut text = format!(
        "Before: {}\r\nAfter:  {}\r\n\r\n",
        before_path.display(),
        after_path.display()
    );
    let mut counts = [0usize; 4];
    let mut sections = Vec::new();
    for section in &before.sections {
        sections.push((
            section.title.as_str(),
            Some(section),
            after
                .sections
                .iter()
                .find(|other| eq_ignore_case(&section.title, &other.title)),
        ));
    }
    for section in &after.sections {
        if !before
            .sections
            .iter()
            .any(|other| eq_ignore_case(&section.title, &other.title))
        {
            sections.push((section.title.as_str(), None, Some(section)));
        }
    }
    for (title, before, after) in sections {
        // ponytail: Device reordering is reported as changed occurrences, not moves.
        // Upgrade path: match device blocks by their first identifier.
        let before = before.map_or(&[][..], |section| section.rows.as_slice());
        let after = after.map_or(&[][..], |section| section.rows.as_slice());
        let mut occurrences = HashMap::new();
        let mut after_by_key = HashMap::new();
        for (index, (label, value)) in after.iter().enumerate() {
            let occurrence = occurrences.entry(label).or_insert(0usize);
            after_by_key.insert((label, *occurrence), (index, value));
            *occurrence += 1;
        }
        occurrences.clear();
        let mut matched = vec![false; after.len()];
        let mut rows = Vec::new();
        for (label, value) in before {
            let occurrence = occurrences.entry(label).or_insert(0usize);
            let other = after_by_key.get(&(label, *occurrence));
            *occurrence += 1;
            let (kind, value) = match other {
                Some((index, other)) => {
                    matched[*index] = true;
                    if value == *other {
                        (Kind::Same, value.clone())
                    } else {
                        (Kind::Changed, format!("{value}  ->  {other}"))
                    }
                }
                None => (Kind::Removed, value.clone()),
            };
            rows.push((kind, label, value));
        }
        for (index, (label, value)) in after.iter().enumerate() {
            if !matched[index] {
                rows.push((Kind::Added, label, value.clone()));
            }
        }
        if rows.is_empty() {
            continue;
        }
        text.push_str(title);
        text.push_str("\r\n");
        if rows.iter().all(|(kind, _, _)| *kind == Kind::Same) {
            counts[3] += rows.len();
            text.push_str(&format!(
                "  same     {} values, none changed\r\n",
                rows.len()
            ));
        } else {
            let width = rows
                .iter()
                .map(|(_, label, _)| label.encode_utf16().count())
                .max()
                .unwrap_or(0)
                + 2;
            for (index, kind) in [Kind::Changed, Kind::Removed, Kind::Added, Kind::Same]
                .into_iter()
                .enumerate()
            {
                for (_, label, value) in rows.iter().filter(|(row, _, _)| *row == kind) {
                    counts[index] += 1;
                    text.push_str(&format!(
                        "  {:8}{}{value}\r\n",
                        kind.text(),
                        pad_right_utf16(label, width)
                    ));
                }
            }
        }
        text.push_str("\r\n");
    }
    Comparison {
        summary: format!(
            "Changed {} · Added {} · Removed {} · Same {}",
            counts[0], counts[2], counts[1], counts[3]
        ),
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_formats_and_occurrence_matching() {
        let before = parse(
            "ignored: value\r\n===== DISK DRIVES =====\r\n  ├─ Serial: S1 | Model: Drive\r\n│ └─ Serial: S2\r\nSize: 2: 4\r\nGone: old\r\n123: ignored\r\n===== BOARD =====\r\nVendor: Acme\r\n",
            false,
        ).unwrap();
        let after = parse(
            r#"{"app":"HWIDChecker","version":"2.0.0","exported":"2026-10-04T09:30:00","masked":false,"sections":[{"title":"disk drives","lines":["Serial: S2 | Model: Drive","Serial: S1","Size: 2: 4","New: XXXX"],"ids":["S2","S1"]},{"title":"BOARD","lines":["Vendor: Acme"],"ids":[]},{"title":"USB","lines":["Name: Device"],"ids":[]}] }"#,
            true,
        ).unwrap();
        let diff = compare(
            &before,
            &after,
            Path::new("before.txt"),
            Path::new("after.json"),
        );
        assert_eq!(diff.summary, "Changed 2 · Added 2 · Removed 1 · Same 3");
        assert_eq!(
            diff.text,
            concat!(
                "Before: before.txt\r\nAfter:  after.json\r\n\r\n",
                "DISK DRIVES\r\n  changed Serial  S1  ->  S2\r\n  changed Serial  S2  ->  S1\r\n",
                "  removed Gone    old\r\n  added   New     XXXX\r\n",
                "  same    Model   Drive\r\n  same    Size    2: 4\r\n\r\n",
                "BOARD\r\n  same     1 values, none changed\r\n\r\n",
                "USB\r\n  added   Name  Device\r\n\r\n"
            )
        );
        let same = compare(&before, &before, Path::new("a"), Path::new("a"));
        assert_eq!(same.summary, "Changed 0 · Added 0 · Removed 0 · Same 6");
        let removed = compare(&after, &before, Path::new("a"), Path::new("b"));
        assert!(removed.text.contains("USB\r\n  removed Name  Device"));
    }

    #[test]
    fn rejects_raw_report_and_malformed_json_and_preserves_exact_labels() {
        let raw = super::super::format_section("DISK DRIVES", "Serial: S1\r\n");
        assert!(parse(&raw, false).unwrap().sections.is_empty());
        assert!(
            parse(
                " ===== DISK =====\nSerial: S1\n===== DISK ===== \nSerial: S2",
                false
            )
            .unwrap()
            .sections
            .is_empty()
        );
        for invalid in [
            "{",
            "{}",
            r#"{"sections":[{"title":"CPU","lines":"Serial: x"}]}"#,
        ] {
            assert!(parse(invalid, true).is_err());
        }
        let empty = parse("===== Empty =====\r\nNo data available\r\n123: x", false).unwrap();
        assert!(empty.sections[0].rows.is_empty());
        let left = parse("===== CPU =====\nSerial: AbC\nLabel: \n", false).unwrap();
        let right = parse("===== cpu =====\nserial: AbC\nLabel: X\n", false).unwrap();
        assert_eq!(
            compare(&left, &right, Path::new("a"), Path::new("b")).summary,
            "Changed 1 · Added 1 · Removed 1 · Same 0"
        );
        let right = parse("===== cpu =====\nSerial: abc\nLabel: \n", false).unwrap();
        assert_eq!(
            compare(&left, &right, Path::new("a"), Path::new("b")).summary,
            "Changed 1 · Added 0 · Removed 0 · Same 1"
        );
    }
}
