//! Legacy report formatting and the identifier/source records carried with each body.

use crate::win::Error;

#[derive(Clone, Debug, Default)]
pub struct Section {
    pub title: &'static str,
    pub body: String,
    pub ids: Vec<String>,
    pub source: String,
    pub failures: Vec<String>,
    pub elapsed_ms: u128,
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
    use super::*;

    const RULE: &str = "=============================================================================================\r\n";
    const ITEM_RULE: &str = "----------------------------------------\r\n";

    #[test]
    fn formatter_header_literals_and_utf16_centering() {
        // TODO(1.9): confirm against GoldenDump --format.
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
        // TODO(1.9): confirm against GoldenDump --format.
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
        // TODO(1.9): confirm against GoldenDump --format.
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
        // TODO(1.9): confirm against GoldenDump --format.
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
        // TODO(1.9): confirm against GoldenDump --format.
        assert_eq!(pad_right_utf16("A😀é", 6), "A😀é  ");
        assert_eq!(pad_right_utf16("A😀é", 4), "A😀é");
        assert_eq!(pad_right_utf16("A😀é", 1), "A😀é");
        assert_eq!(pad_right_utf16("", 3), "   ");
    }

    #[test]
    fn out_keeps_identifiers_and_diagnostics_out_of_visible_text() {
        // TODO(1.9): confirm against GoldenDump --format.
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
