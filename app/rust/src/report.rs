//! Legacy report formatting and the identifier/source records carried with each body.

use crate::win::Error;

pub mod compare;

#[derive(Clone, Debug, Default)]
pub struct Section {
    pub title: &'static str,
    pub body: String,
    pub ids: Vec<String>,
    pub source: String,
    pub failures: Vec<String>,
    pub elapsed_ms: u128,
}

const MASK_MIN_LEN: usize = 4;

/// Masks ASCII letters and digits without changing separators or character widths.
pub fn mask_value(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { 'X' } else { c })
        .collect()
}

/// Masks only whole-token occurrences of provider-marked identifiers in a copy.
pub fn masked(section: &Section) -> Section {
    let mut values: Vec<&str> = section
        .ids
        .iter()
        .map(String::as_str)
        .filter(|value| value.chars().count() >= MASK_MIN_LEN)
        .collect();
    values.sort_unstable_by_key(|value| (std::cmp::Reverse(value.len()), *value));
    values.dedup();
    let mut result = section.clone();
    result.body.clear();
    // Match against the original text, so an overlapping shorter ID cannot consume an
    // already-masked replacement (or change the boundaries used for a later match).
    let mut offset = 0;
    while offset < section.body.len() {
        let rest = &section.body[offset..];
        let start = !section.body[..offset]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let found = values.iter().find(|value| {
            start
                && rest.starts_with(**value)
                && !rest[value.len()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
        });
        if let Some(value) = found {
            result.body.push_str(&mask_value(value));
            offset += value.len();
        } else if let Some(c) = rest.chars().next() {
            result.body.push(c);
            offset += c.len_utf8();
        }
    }
    for value in &mut result.ids {
        if value.chars().count() >= MASK_MIN_LEN {
            *value = mask_value(value);
        }
    }
    result
}

/// Serializes already-prepared sections as pretty CRLF JSON without diagnostic fields.
pub fn export_json(
    sections: &[Section],
    exported: &str,
    masked: bool,
) -> crate::win::Result<String> {
    #[derive(serde::Serialize)]
    struct ExportSection<'a> {
        title: &'a str,
        lines: Vec<&'a str>,
        ids: &'a [String],
    }
    #[derive(serde::Serialize)]
    struct Export<'a> {
        app: &'static str,
        version: &'static str,
        exported: &'a str,
        masked: bool,
        sections: Vec<ExportSection<'a>>,
    }
    let export = Export {
        app: "HWIDChecker",
        version: env!("CARGO_PKG_VERSION"),
        exported,
        masked,
        sections: sections
            .iter()
            .map(|section| ExportSection {
                title: section.title,
                lines: section.body.split_terminator("\r\n").collect(),
                ids: &section.ids,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&export)
        .map(|json| json.replace('\n', "\r\n"))
        .map_err(|error| Error::msg("Serialize export JSON", error.to_string()))
}

#[derive(Default)]
pub struct Out {
    section: Section,
}

impl Out {
    /// Creates an empty output builder with identifier and source records.
    pub fn new() -> Self {
        Self::default()
    }
    /// Appends a normal labeled value.
    pub fn info(&mut self, label: &str, value: &str) -> &mut Self {
        self.section.body.push_str(&info_line(label, value));
        self
    }
    /// Appends a labeled identifier and records its value.
    pub fn id(&mut self, label: &str, value: &str) -> &mut Self {
        self.id_value(value);
        self.info(label, value)
    }
    /// Appends combined labeled values, recording those marked as identifiers.
    pub fn combined(&mut self, items: &[(&str, &str, bool)]) -> &mut Self {
        for &(_, value, is_id) in items {
            if is_id {
                self.id_value(value);
            }
        }
        let values: Vec<_> = items
            .iter()
            .map(|&(label, value, _)| (label, value))
            .collect();
        self.section.body.push_str(&combined_line(&values));
        self
    }
    /// Appends the legacy item separator.
    pub fn separator(&mut self) -> &mut Self {
        self.section.body.push_str(&item_separator());
        self
    }
    /// Appends a blank CRLF line.
    pub fn blank(&mut self) -> &mut Self {
        self.section.body.push_str("\r\n");
        self
    }
    /// Appends one free-layout line followed by CRLF.
    pub fn text(&mut self, line: &str) -> &mut Self {
        self.section.body.push_str(line);
        self.section.body.push_str("\r\n");
        self
    }
    /// Records an identifier embedded in a free-layout line.
    pub fn id_value(&mut self, value: &str) -> &mut Self {
        self.section.ids.push(value.to_owned());
        self
    }
    /// Records the source that produced the current section's data.
    pub fn source(&mut self, name: &str) -> &mut Self {
        self.section.source = name.to_owned();
        self
    }
    /// Records a failed fallback without adding it to visible report text.
    pub fn fallback_failed(&mut self, source: &str, error: &Error) -> &mut Self {
        self.section.failures.push(format!("{source}: {error}"));
        self
    }
    /// Trims trailing body whitespace, preserving identifier, source and failure records.
    pub fn trim_end(&mut self) -> &mut Self {
        self.section.body.truncate(
            self.section
                .body
                .trim_end_matches(char::is_whitespace)
                .len(),
        );
        self
    }
    /// Finishes the body and its identifier, source and failure records.
    pub fn finish(self) -> Section {
        self.section
    }
}

/// Returns the main report separator with CRLF.
pub fn separator() -> String {
    // C# parity: TextFormattingService.cs:40-43.
    format!("{}\r\n", "=".repeat(93))
}
/// Centers text using .NET UTF-16 string length and trailing CRLF.
pub fn centered(text: &str) -> String {
    // C# parity: TextFormattingService.cs:60-63. PadLeft never truncates long text.
    format!(
        "{}{text}\r\n",
        " ".repeat(93_usize.saturating_sub(text.encode_utf16().count()) / 2)
    )
}
/// Formats the report header.
pub fn format_header(text: &str) -> String {
    // C# parity: TextFormattingService.cs:13-20.
    format!("{}{}{}", separator(), centered(text), separator())
}
/// Formats a titled section and ensures a final CRLF.
pub fn format_section(title: &str, content: &str) -> String {
    // C# parity: TextFormattingService.cs:22-38. Preserve the body's existing whitespace.
    format!(
        "{}{content}{}",
        format_header(title),
        if content.ends_with("\r\n") {
            ""
        } else {
            "\r\n"
        }
    )
}
/// Formats a single labeled value.
pub fn info_line(label: &str, value: &str) -> String {
    // C# parity: TextFormattingService.cs:50-53.
    format!("{label}: {value}\r\n")
}
/// Formats labeled values separated by the legacy pipe delimiter.
pub fn combined_line(items: &[(&str, &str)]) -> String {
    // C# parity: TextFormattingService.cs:55-58. An empty list still appends a line.
    format!(
        "{}\r\n",
        items
            .iter()
            .map(|(label, value)| format!("{label}: {value}"))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}
/// Returns the legacy item separator with CRLF.
pub fn item_separator() -> String {
    // C# parity: TextFormattingService.cs:45-48.
    format!("{}\r\n", "-".repeat(40))
}
/// Formats device groups separated by the legacy item separator.
pub fn device_group(devices: &[Vec<(&str, &str)>]) -> String {
    // C# parity: TextFormattingService.cs:65-81. Separators go between groups only.
    devices
        .iter()
        .map(|items| {
            items
                .iter()
                .map(|(label, value)| info_line(label, value))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(&item_separator())
}
/// Applies the section-view trim and empty-body placeholder.
pub fn section_content(content: &str) -> String {
    // C# parity: SectionedViewForm.cs:544-548.
    let content = trim_net(content);
    if content.is_empty() {
        "No data available".to_owned()
    } else {
        content.to_owned()
    }
}
/// Formats sections for the main-window export, preserving provider order.
pub fn export_text(sections: &[Section]) -> String {
    // C# parity: SectionedViewForm.cs:717-721. Export has its own short headings.
    sections
        .iter()
        .map(|section| {
            format!(
                "===== {} =====\r\n{}\r\n\r\n",
                section.title,
                section_content(&section.body)
            )
        })
        .collect()
}
/// Pads to the requested width counted in UTF-16 units.
pub fn pad_right_utf16(text: &str, width: usize) -> String {
    // C# parity: RamInfo.cs:48-54. Left-aligned columns count UTF-16 units.
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.encode_utf16().count()))
    )
}
/// Trims whitespace using the plan's .NET-compatible character rule.
pub fn trim_net(text: &str) -> &str {
    // C# parity: SectionedViewForm.cs:544. Rust and .NET use Unicode whitespace here.
    text.trim_matches(char::is_whitespace)
}
/// Compares labels using .NET's ordinal case-insensitive, non-expanding casing.
pub fn eq_ignore_case(left: &str, right: &str) -> bool {
    // C# parity: HardwareInfoManager.cs:112; SectionedViewForm.cs:557.
    let mut right = right.chars();
    left.chars().all(|left| {
        right
            .next()
            .is_some_and(|right| ordinal_upper(left) == ordinal_upper(right))
    }) && right.next().is_none()
}
/// Searches labels without expanding Unicode characters into multiple characters.
pub fn contains_ignore_case(text: &str, part: &str) -> bool {
    part.is_empty()
        || text.char_indices().any(|(start, _)| {
            let mut text = text[start..].chars();
            part.chars().all(|part| {
                text.next()
                    .is_some_and(|text| ordinal_upper(text) == ordinal_upper(part))
            })
        })
}

fn ordinal_upper(ch: char) -> char {
    if ch.is_ascii() {
        return ch.to_ascii_uppercase();
    }
    // Unicode simple uppercase retains the Greek iota subscript. Rust's full
    // uppercase expands it, whereas .NET ordinal comparison keeps one character.
    match ch {
        '\u{1f80}'..='\u{1f87}' | '\u{1f90}'..='\u{1f97}' | '\u{1fa0}'..='\u{1fa7}' => {
            return char::from_u32(ch as u32 + 8).unwrap_or(ch);
        }
        '\u{1fb3}' => return '\u{1fbc}',
        '\u{1fc3}' => return '\u{1fcc}',
        '\u{1ff3}' => return '\u{1ffc}',
        _ => {}
    }
    let mut upper = ch.to_uppercase();
    match (upper.next(), upper.next()) {
        // Ordinal casing does not equate non-ASCII dotless i or long s with ASCII.
        (Some(upper), None) if !upper.is_ascii() => upper,
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mask_respects_provider_ids_boundaries_overlap_and_unicode() {
        let mut section = super::Section {
            title: "FIXTURE",
            body:
                "AB12 AB12-CD34 xAB12 AB12z éAB12 AB12é (AB12)\r\nN/A 0 1 éß😀9\r\nUnmarked: XY99"
                    .into(),
            ids: ["AB12", "AB12-CD34", "AB12", "N/A", "0", "1", "éß😀9"]
                .map(str::to_owned)
                .into(),
            source: "native".into(),
            failures: vec!["diagnostic AB12".into()],
            elapsed_ms: 7,
        };
        let masked = super::masked(&section);
        assert_eq!(
            masked.body,
            "XXXX XXXX-XXXX xAB12 AB12z éAB12 AB12é (XXXX)\r\nN/A 0 1 éß😀X\r\nUnmarked: XY99"
        );
        assert_eq!(
            masked.ids,
            ["XXXX", "XXXX-XXXX", "XXXX", "N/A", "0", "1", "éß😀X"]
        );
        assert_eq!(
            masked.body.encode_utf16().count(),
            section.body.encode_utf16().count()
        );
        assert_eq!(
            (
                masked.title,
                masked.source,
                masked.failures,
                masked.elapsed_ms
            ),
            (
                section.title,
                section.source.clone(),
                section.failures.clone(),
                section.elapsed_ms
            )
        );
        assert!(section.body.starts_with("AB12"));
        section.ids.clear();
        assert_eq!(super::masked(&section).body, section.body);
        assert_eq!(
            super::mask_value("{ab12-34CD} eui.0000_0001. \\ é9"),
            "{XXXX-XXXX} XXX.XXXX_XXXX. \\ éX"
        );
    }

    #[test]
    fn json_export_preserves_rows_ids_and_excludes_diagnostics() {
        let sections = [
            super::Section {
                title: "FIXTURE",
                body: "  Serial: AB12\r\nquote: \"\\é😀\r\n\r\n".into(),
                ids: vec!["AB12".into(), "AB12".into()],
                source: "PRIVATE SOURCE".into(),
                failures: vec!["PRIVATE FAILURE".into()],
                elapsed_ms: 17,
            },
            super::Section {
                title: "EMPTY",
                ..Default::default()
            },
        ];
        let raw = super::export_json(&sections, "2026-10-04T09:30:00", false).unwrap();
        assert!(raw.starts_with("{\r\n  \"app\": \"HWIDChecker\",\r\n"));
        assert!(!raw.replace("\r\n", "").contains('\n'));
        assert!(!raw.contains("PRIVATE"));
        let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 5);
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["exported"], "2026-10-04T09:30:00");
        assert_eq!(json["masked"], false);
        assert_eq!(
            json["sections"][0]["lines"],
            serde_json::json!(["  Serial: AB12", "quote: \"\\é😀", ""])
        );
        assert_eq!(
            json["sections"][0]["ids"],
            serde_json::json!(["AB12", "AB12"])
        );
        assert_eq!(json["sections"][0].as_object().unwrap().len(), 3);
        assert_eq!(json["sections"][1]["lines"], serde_json::json!([]));
        assert_eq!(json["sections"][1]["title"], "EMPTY");
        let masked = sections.iter().map(super::masked).collect::<Vec<_>>();
        let raw = super::export_json(&masked, "2026-10-04T09:30:00", true).unwrap();
        assert!(!raw.contains("AB12"));
        assert!(raw.contains("\"masked\": true"));
    }
    use super::*;

    const RULE: &str = "=============================================================================================\r\n";
    const ITEM_RULE: &str = "----------------------------------------\r\n";

    #[test]
    fn out_trim_end_preserves_leading_whitespace_and_all_records() {
        for (body, expected) in [
            ("", ""),
            (" \t\r\n\u{2003}", ""),
            (
                "  Name: Écran 😀\r\nID: SN7F29D4  \t\u{0085}\u{2003}\r\n",
                "  Name: Écran 😀\r\nID: SN7F29D4",
            ),
        ] {
            let mut out = Out::new();
            out.section.title = "FIXTURE";
            out.section.elapsed_ms = 7;
            out.section.body = body.to_owned();
            out.id_value("SN7F29D4")
                .source("native")
                .fallback_failed("WMI", &Error::msg("query", "fabricated failure"));
            out.trim_end().trim_end();
            let section = out.finish();
            assert_eq!(section.body, expected);
            assert_eq!(section.ids, ["SN7F29D4"]);
            assert_eq!(section.source, "native");
            assert_eq!(
                section.failures,
                ["WMI: query failed: 0x00000000 fabricated failure"]
            );
            assert_eq!((section.title, section.elapsed_ms), ("FIXTURE", 7));
        }
    }

    #[test]
    fn formatter_header_literals_and_utf16_centering() {
        assert_eq!(separator(), RULE);
        assert_eq!(item_separator(), ITEM_RULE);
        assert_eq!(
            centered("GPU"),
            "                                             GPU\r\n"
        );
        assert_eq!(
            centered("A😀é"),
            "                                            A😀é\r\n"
        );
        assert_eq!(
            centered(""),
            "                                              \r\n"
        );
        assert_eq!(
            format_header("GPU"),
            [
                RULE,
                "                                             GPU\r\n",
                RULE
            ]
            .concat()
        );
        let long = "X".repeat(94);
        assert_eq!(centered(&long), format!("{long}\r\n"));
    }

    #[test]
    fn formatter_section_preserves_body_and_only_adds_missing_crlf() {
        let heading = [
            RULE,
            "                                             GPU\r\n",
            RULE,
        ]
        .concat();
        for (body, expected) in [
            ("value", "value\r\n"),
            ("value\r\n", "value\r\n"),
            ("value\n", "value\n\r\n"),
            ("", "\r\n"),
            ("  value  \r\n\r\n", "  value  \r\n\r\n"),
        ] {
            assert_eq!(format_section("GPU", body), format!("{heading}{expected}"));
        }
    }

    #[test]
    fn formatter_info_combined_and_device_group_literals() {
        assert_eq!(info_line("Série", "SN8D4C2A9"), "Série: SN8D4C2A9\r\n");
        assert_eq!(info_line("", ""), ": \r\n");
        assert_eq!(
            combined_line(&[("Name", "Écran 😀"), ("Serial", "SN8D4C2A9")]),
            "Name: Écran 😀 | Serial: SN8D4C2A9\r\n"
        );
        assert_eq!(combined_line(&[]), "\r\n");
        assert_eq!(
            device_group(&[
                vec![("Name", "Écran 😀"), ("Serial", "SN8D4C2A9")],
                vec![("Name", "Second")]
            ]),
            "Name: Écran 😀\r\nSerial: SN8D4C2A9\r\n----------------------------------------\r\nName: Second\r\n"
        );
        assert_eq!(device_group(&[]), "");
        assert_eq!(
            device_group(&[vec![], vec![], vec![]]),
            [ITEM_RULE, ITEM_RULE].concat()
        );
    }

    #[test]
    fn section_content_and_export_literals() {
        assert_eq!(
            section_content(" \t\r\n\u{85}\u{a0}\u{2003}\u{2028}\u{3000}"),
            "No data available"
        );
        assert_eq!(section_content("\u{2003} value 😀 \r\n"), "value 😀");
        assert_eq!(trim_net("\u{200b}value\u{feff}"), "\u{200b}value\u{feff}");
        let sections = [
            Section {
                title: "GPU INFO",
                body: "  Name: Écran 😀\r\n\r\n".to_owned(),
                ..Section::default()
            },
            Section {
                title: "EMPTY",
                body: "\t\r\n".to_owned(),
                ..Section::default()
            },
            Section {
                title: "CPU",
                body: "Loading...".to_owned(),
                ..Section::default()
            },
        ];
        assert_eq!(
            export_text(&sections),
            "===== GPU INFO =====\r\nName: Écran 😀\r\n\r\n===== EMPTY =====\r\nNo data available\r\n\r\n===== CPU =====\r\nLoading...\r\n\r\n"
        );
        assert_eq!(export_text(&[]), "");
    }

    #[test]
    fn utf16_padding_literals() {
        assert_eq!(pad_right_utf16("A😀é", 6), "A😀é  ");
        assert_eq!(pad_right_utf16("A😀é", 4), "A😀é");
        assert_eq!(pad_right_utf16("A😀é", 1), "A😀é");
        assert_eq!(pad_right_utf16("", 3), "   ");
    }

    #[test]
    fn out_keeps_identifiers_and_diagnostics_out_of_visible_text() {
        let mut out = Out::new();
        out.info("Name", "Écran 😀")
            .id("Serial", "SN8D4C2A9")
            .combined(&[("Mode", "native", false), ("ID", "ID7F29D4", true)])
            .separator()
            .blank()
            .text("  Embedded ID7F29D4")
            .id_value("ID7F29D4")
            .source("native")
            .fallback_failed("WMI", &Error::msg("query", "fabricated failure"));
        let section = out.finish();
        assert_eq!(
            section.body,
            "Name: Écran 😀\r\nSerial: SN8D4C2A9\r\nMode: native | ID: ID7F29D4\r\n----------------------------------------\r\n\r\n  Embedded ID7F29D4\r\n"
        );
        assert_eq!(section.ids, ["SN8D4C2A9", "ID7F29D4", "ID7F29D4"]);
        assert_eq!(section.source, "native");
        assert_eq!(
            section.failures,
            ["WMI: query failed: 0x00000000 fabricated failure"]
        );
        assert!(Out::new().finish().body.is_empty());
    }

    #[test]
    fn ordinal_matching_does_not_expand_unicode_or_cross_ascii_boundary() {
        assert!(eq_ignore_case("GPU INFO", "gpu info"));
        assert!(eq_ignore_case("Écran 😀 ᾀ", "éCRAN 😀 ᾈ"));
        for (left, right) in [
            ("ß", "SS"),
            ("ß", "ẞ"),
            ("ı", "I"),
            ("ſ", "S"),
            ("K", "k"),
            ("é", "e"),
            ("GPU", "GPU INFO"),
        ] {
            assert!(!eq_ignore_case(left, right), "{left} / {right}");
        }
        assert!(contains_ignore_case("Écran 😀 GPU INFO", "éCRAN 😀 gpu"));
        assert!(!contains_ignore_case("Straße", "SSE"));
        assert!(!contains_ignore_case("İ", "i"));
        assert!(contains_ignore_case("", ""));
        assert!(!contains_ignore_case("", "GPU"));
    }
}
