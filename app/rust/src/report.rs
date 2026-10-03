//! Frozen report contracts; formatting parity is owned by the report fill-in agent.

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
    format!("{}\r\n", "=".repeat(93))
}
/// Centers text using .NET UTF-16 string length and trailing CRLF.
pub fn centered(text: &str) -> String {
    format!(
        "{}{text}\r\n",
        " ".repeat(93_usize.saturating_sub(text.encode_utf16().count()) / 2)
    )
}
/// Formats the report header.
pub fn format_header(text: &str) -> String {
    format!("{}{}{}", separator(), centered(text), separator())
}
/// Formats a titled section and ensures a final CRLF.
pub fn format_section(title: &str, content: &str) -> String {
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
    format!("{label}: {value}\r\n")
}
/// Formats labeled values separated by the legacy pipe delimiter.
pub fn combined_line(items: &[(&str, &str)]) -> String {
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
    format!("{}\r\n", "-".repeat(40))
}
/// Formats device groups separated by the legacy item separator.
pub fn device_group(devices: &[Vec<(&str, &str)>]) -> String {
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
    let content = trim_net(content);
    if content.is_empty() {
        "No data available".to_owned()
    } else {
        content.to_owned()
    }
}
/// Formats sections for the main-window export, preserving provider order.
pub fn export_text(sections: &[Section]) -> String {
    sections
        .iter()
        .map(|section| format_section(section.title, &section_content(&section.body)))
        .collect()
}
/// Pads to the requested width counted in UTF-16 units.
pub fn pad_right_utf16(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.encode_utf16().count()))
    )
}
/// Trims whitespace using the plan's .NET-compatible character rule.
pub fn trim_net(text: &str) -> &str {
    text.trim_matches(char::is_whitespace)
}
/// Compares labels case-insensitively.
pub fn eq_ignore_case(left: &str, right: &str) -> bool {
    left.to_uppercase() == right.to_uppercase()
}
/// Searches labels case-insensitively.
pub fn contains_ignore_case(text: &str, part: &str) -> bool {
    text.to_uppercase().contains(&part.to_uppercase())
}
