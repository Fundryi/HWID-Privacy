//! C2 export parsing and comparison. No hardware collection and no file writes.

use super::{eq_ignore_case, pad_right_utf16};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// Comparison retains its wider not-unique rule, independent of display and masking.
pub(crate) fn generic_value(value: &str) -> bool {
    if super::placeholder_value(value) {
        return true;
    }
    let value = value.trim().to_ascii_lowercase();
    if let Some(payload) = value.strip_prefix("gpu-") {
        return generic_value(payload);
    }
    let compact: String = value.chars().filter(|c| c.is_alphanumeric()).collect();
    let digits = compact.strip_prefix("0x").unwrap_or(&compact);
    !digits.is_empty()
        && (digits.chars().all(|c| c == 'x')
            || digits.len() >= 4 && digits.chars().all(|c| digits.starts_with(c)))
}
const RETIRED_SECTIONS: &[&str] = &["AUDIO DEVICES"];
const LABEL_ALIASES: &[(&str, &str)] = &[
    ("Serial Number (Product ID)", "Product ID"),
    ("SMBIOS Version", "BIOS Version"),
];

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
    pub masked: bool,
    ids: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct Section {
    title: String,
    entities: Vec<Entity>,
    header: Entity,
    blocked: bool,
}

#[derive(Debug, Default, PartialEq)]
struct Entity {
    // Keep independent sources and device kinds apart even when values coincide.
    family: String,
    name: String,
    heading: String,
    rows: Vec<(String, String)>,
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct JsonExport {
    sections: Vec<JsonSection>,
    #[serde(default)]
    masked: bool,
}

#[derive(Deserialize)]
struct JsonSection {
    title: String,
    lines: Vec<String>,
    #[serde(default)]
    ids: Vec<String>,
}

/// Reads text exports or the C3 JSON format; metadata and identifier lists are not diff keys.
pub fn read(path: &Path) -> Result<Export, ReadError> {
    if path.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".diag.txt")
    }) {
        return Err(ReadError::Empty(format!(
            "No hardware values found in: {}",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(path).map_err(|error| read_error(path, &error))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let json = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let mut export = parse(text, json).map_err(|error| read_error(path, &error))?;
    export.masked |= path.file_stem().is_some_and(|stem| {
        stem.to_string_lossy()
            .to_ascii_uppercase()
            .ends_with("-MASKED")
    });
    if export.sections.iter().all(|section| {
        section.header.rows.is_empty() && section.entities.iter().all(|e| e.rows.is_empty())
    }) {
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
        export.masked = data.masked;
        for section in data.sections {
            export.push(&section.title, &section.lines.join("\n"), &section.ids);
        }
    } else {
        // Diagnostics can be renamed in a picker. Their format is not a
        // hardware export, even if a helper error contains export-like text.
        if text.lines().any(|line| line == "[helpers]")
            && text.lines().any(|line| line.starts_with("Time: "))
            && text.lines().any(|line| line.starts_with("Source: "))
        {
            return Ok(export);
        }
        let mut current = None;
        let mut body = String::new();
        for line in text.lines() {
            if let Some(title) = line
                .strip_prefix("===== ")
                .and_then(|line| line.strip_suffix(" ====="))
            {
                if let Some(title) = current {
                    export.push(title, &body, &[]);
                    body.clear();
                }
                current = Some(title);
            } else if current.is_some() {
                body.push_str(line);
                body.push('\n');
            }
        }
        if let Some(title) = current {
            export.push(title, &body, &[]);
        }
    }
    Ok(export)
}

impl Export {
    /// All source identifiers, including IDs embedded inside a larger displayed value.
    pub(crate) fn ids(&self) -> &[String] {
        &self.ids
    }

    /// Whether parsing found any hardware values (also used for the live snapshot).
    pub(crate) fn is_empty(&self) -> bool {
        self.sections.iter().all(|section| {
            section.header.rows.is_empty() && section.entities.iter().all(|e| e.rows.is_empty())
        })
    }

    /// Display metadata for a device; unblocked sections have flat rows instead.
    pub(crate) fn device(&self, title: &str, position: Option<usize>) -> Option<(&str, &str)> {
        let section = self
            .sections
            .iter()
            .find(|s| eq_ignore_case(&s.title, title))?;
        if !section.blocked {
            return None;
        }
        let entity = section.entities.get(position?)?;
        let name = entity
            .rows
            .iter()
            .find(|(label, _)| identity_weight(label) == 12)
            .map_or("", |(_, value)| value.as_str());
        Some((&entity.heading, name))
    }

    fn push(&mut self, title: &str, body: &str, ids: &[String]) {
        if RETIRED_SECTIONS
            .iter()
            .any(|retired| eq_ignore_case(title, retired))
        {
            return;
        }
        self.ids.extend_from_slice(ids);
        let section = split_section(title, body, ids);
        if let Some(existing) = self
            .sections
            .iter_mut()
            .find(|s| eq_ignore_case(&s.title, title))
        {
            existing.entities.extend(section.entities);
            existing.header.rows.extend(section.header.rows);
            existing.blocked |= section.blocked;
        } else {
            self.sections.push(section);
        }
    }
}

/// Builds the file-comparison model from live, unmasked provider sections.
/// When requested, masks a copy using the same ID list as the export command.
pub fn from_sections(sections: &[super::Section], masked: bool) -> Export {
    let mut export = Export {
        masked,
        ..Export::default()
    };
    for section in sections {
        let prepared;
        let section = if masked {
            prepared = super::masked(section);
            &prepared
        } else {
            section
        };
        export.push(section.title, &section.body, &section.ids);
    }
    export
}

fn content(line: &str) -> &str {
    line.trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, '│' | '├' | '└' | '─'))
}

fn gpu_prefix(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("GPU ")?;
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    (end > 0).then(|| (&line[..4 + end], rest[end..].trim_start()))
}

fn split_section(title: &str, body: &str, ids: &[String]) -> Section {
    let mut section = Section {
        title: title.to_owned(),
        entities: Vec::new(),
        header: Entity::default(),
        blocked: matches!(
            title.to_ascii_uppercase().as_str(),
            "DISK DRIVES"
                | "USB DEVICES"
                | "GPU INFO"
                | "MONITOR INFORMATION"
                | "NETWORK ADAPTERS (NIC'S)"
                | "BLUETOOTH ADAPTERS"
                | "BATTERY"
                | "RAM MODULES"
        ),
    };
    let mut current = Entity::default();
    let mut ram_columns = Vec::new();
    let title_upper = title.to_ascii_uppercase();
    let firmware_kind = match title_upper.as_str() {
        "CHASSIS" => Some("Power Supply"),
        "(SM)BIOS" => Some("Firmware Component"),
        "CPU" => Some("CPU Socket"),
        "TPM MODULES" => Some("TPM Firmware"),
        _ => None,
    };
    for raw in body.lines() {
        let line = content(raw);
        // These inventories append firmware records independently of the legacy
        // values. A single printable record has fields but no explicit heading.
        if let Some(kind) = firmware_kind {
            let heading = line.starts_with(&format!("{kind} #")) && line.ends_with(" (SMBIOS)");
            let metadata = line.split_once(':').is_some_and(|(label, _)| {
                label.ends_with(" (SMBIOS)")
                    || kind == "TPM Firmware" && label == "TPM Version Cross-check"
            });
            if heading || metadata {
                section.blocked = true;
                if heading || current.family != kind {
                    if !current.rows.is_empty() {
                        section.entities.push(std::mem::take(&mut current));
                    }
                    current.family = kind.into();
                    current.heading = if heading {
                        line.into()
                    } else {
                        format!("{kind} #1 (SMBIOS)")
                    };
                }
                if !heading {
                    add_fields(&mut current, line);
                }
                continue;
            }
            if title_upper == "CPU" && line.starts_with("Name: ") {
                section.blocked = true;
                if !current.rows.is_empty() {
                    section.entities.push(std::mem::take(&mut current));
                }
                current.family = "Processor".into();
                current.heading = format!("CPU {}", section.entities.len() + 1);
            } else if title_upper != "CPU"
                || line.starts_with("CPUID ")
                || current.family != "Processor"
                || line.starts_with("Error ")
            {
                add_fields(&mut section.header, line);
                continue;
            }
        }
        if title_upper == "BATTERY"
            && (line.starts_with("Battery: #") || line.starts_with("SMBIOS Battery: #"))
        {
            if !current.rows.is_empty() {
                section.entities.push(std::mem::take(&mut current));
            }
            current.family = line.split_once(':').map_or("", |(label, _)| label).into();
            current.heading = line.replace(": #", " ");
            continue;
        }
        let separator = line.len() >= 4 && line.bytes().all(|c| c == b'-');
        let disk_root = raw.starts_with("└── PHYSICALDRIVE");
        let gpu = gpu_prefix(line);
        let gpu_root = gpu.is_some_and(|(_, rest)| rest.is_empty());
        if separator || disk_root || gpu_root {
            section.blocked = true;
            if !current.rows.is_empty() {
                section.entities.push(std::mem::take(&mut current));
            }
            if disk_root || gpu_root {
                current.name = line.to_owned();
                current.heading = line.to_owned();
            }
            continue;
        }
        if line.starts_with("DeviceLocator ") && line.contains("SerialNumber") {
            ram_columns = [
                "DeviceLocator",
                "Manufacturer",
                "PartNumber",
                "Capacity",
                "SerialNumber",
            ]
            .into_iter()
            .filter_map(|label| {
                raw.find(label)
                    .map(|start| (label, raw[..start].encode_utf16().count()))
            })
            .collect();
            continue;
        }
        if !ram_columns.is_empty() && !line.is_empty() {
            let units: Vec<_> = raw.encode_utf16().collect();
            let mut entity = Entity::default();
            for (i, (label, start)) in ram_columns.iter().enumerate() {
                let end = ram_columns
                    .get(i + 1)
                    .map_or(units.len(), |(_, n)| *n)
                    .min(units.len());
                let value = String::from_utf16_lossy(&units[(*start).min(end)..end]);
                entity
                    .rows
                    .push(((*label).to_owned(), value.trim().to_owned()));
            }
            section.entities.push(entity);
            continue;
        }
        if line.starts_with("Count: ") {
            add_fields(&mut section.header, line);
            continue;
        }
        // New exports qualify detached NVIDIA details by GPU index. Older exports
        // placed the unqualified board serial after all GPUs, but it belongs to GPU 0.
        if let Some((name, detail)) = gpu.filter(|(_, rest)| !rest.is_empty()) {
            if let Some(entity) = section.entities.iter_mut().find(|e| e.name == name) {
                add_fields(entity, detail);
            } else if current.name == name {
                add_fields(&mut current, detail);
            }
            continue;
        }
        if eq_ignore_case(title, "GPU INFO")
            && [
                "Board Serial Number: ",
                "Serial Number: ",
                "PDI: ",
                "Board Part Number: ",
                "VBIOS Version: ",
            ]
            .iter()
            .any(|label| line.starts_with(label))
        {
            if let Some(first) = section.entities.first_mut() {
                add_fields(first, line);
            } else {
                add_fields(&mut current, line);
            }
            continue;
        }
        if gpu_prefix(&current.name).is_some() && raw.starts_with("└── ") {
            current.rows.push(("Model".into(), line.to_owned()));
        } else if gpu_prefix(&current.name).is_some()
            && !line.contains(": ")
            && line.starts_with("PCI\\")
        {
            current.rows.push(("Instance ID".into(), line.to_owned()));
        } else {
            add_fields(&mut current, line);
        }
    }
    if !current.rows.is_empty() {
        section.entities.push(current);
    }
    for (index, entity) in section.entities.iter_mut().enumerate() {
        if entity.heading.is_empty() && section.blocked {
            let prefix = match title.to_ascii_uppercase().as_str() {
                "DISK DRIVES" => "Disk",
                "GPU INFO" => "GPU",
                "MONITOR INFORMATION" => "Monitor",
                "NETWORK ADAPTERS (NIC'S)" | "BLUETOOTH ADAPTERS" => "Adapter",
                "RAM MODULES" => "Module",
                _ => "Device",
            };
            entity.heading = format!("{prefix} {}", index + 1);
        }
        if entity.name.is_empty() {
            entity.name = entity
                .rows
                .iter()
                .find(|(label, _)| identity_weight(label) == 12)
                .map_or_else(String::new, |(_, value)| value.clone());
        }
        entity.ids = ids
            .iter()
            .filter(|id| entity.rows.iter().any(|(_, value)| value == *id))
            .cloned()
            .collect();
    }
    section
}

fn add_fields(entity: &mut Entity, line: &str) {
    for piece in line.split(" | ") {
        if let Some((label, value)) = piece
            .split_once(':')
            .filter(|(_, value)| value.is_empty() || value.starts_with(' '))
        {
            let label = label.trim();
            let label = LABEL_ALIASES
                .iter()
                .find(|(old, _)| *old == label)
                .map_or(label, |(_, new)| *new);
            if label.chars().any(char::is_alphabetic) {
                entity
                    .rows
                    .push((label.to_owned(), value.trim().to_owned()));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Changed,
    Removed,
    Added,
    Same,
    Moved,
}

impl Kind {
    fn text(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Removed => "removed",
            Self::Added => "added",
            Self::Same => "same",
            Self::Moved => "moved",
        }
    }
}

/// Ready-to-display result; both strings use the spec's wording and the well uses CRLF.
pub struct Comparison {
    pub summary: String,
    pub text: String,
    pub entities: Vec<EntityChange>,
    pub warning: Option<String>,
}

/// One device (or section header), with zero-based positions and independent field kinds.
#[derive(Debug)]
pub struct EntityChange {
    pub section: String,
    pub before_position: Option<usize>,
    pub after_position: Option<usize>,
    pub before_name: Option<String>,
    pub after_name: Option<String>,
    pub kind: Kind,
    pub fields: Vec<FieldChange>,
    pub is_header: bool,
}

#[derive(Debug)]
pub struct FieldChange {
    pub label: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub kind: Kind,
    /// A paired identifier field; generic pairs carry a neutral verdict.
    pub identifier: bool,
}

impl FieldChange {
    /// Placeholders cannot identify a PC, including rows of unmatched devices.
    pub(crate) fn not_unique(&self) -> bool {
        (self.identifier || identifier_label(&self.label))
            && (self.before.is_some() || self.after.is_some())
            && self.before.as_deref().is_none_or(generic_value)
            && self.after.as_deref().is_none_or(generic_value)
    }
}

fn usable(value: &str) -> bool {
    !generic_value(value)
}

fn identifier_label(label: &str) -> bool {
    let label = label.trim().to_ascii_lowercase();
    identity_weight(&label) >= 80
        || matches!(
            label.as_str(),
            "volume-sn"
                | "disk signature"
                | "product id"
                | "identifyingnumber"
                | "thumbprint"
                | "sha256 hash"
                | "windows product key"
                | "instance id"
                | "battery unique id"
        )
        || label.contains("asset tag")
        || label == "sku"
        || label.ends_with(" sku")
        || label == "board part number"
        || label.starts_with("oem string (")
}

fn identity_weight(label: &str) -> u32 {
    let label = label.to_ascii_lowercase();
    let label = label.strip_suffix(" (smbios)").unwrap_or(&label);
    if label.contains("serial")
        || label.starts_with("mac") && !label.starts_with("machine")
        || label.contains(" mac")
        || label == "pdi"
        || label.contains("eui-64")
        || label.contains("wwn")
    {
        100
    } else if label.contains("uuid")
        || label.contains("guid")
        || label.contains("uniqueid")
        || label == "battery unique id"
        || label.contains("asset tag")
        || label.contains("container id")
        || label.contains("storage id")
    {
        80
    } else if label.contains("hardware id") || label == "instance id" {
        40
    } else if label.contains("model")
        || matches!(
            label,
            "name"
                | "device"
                | "adapter"
                | "product"
                | "product name"
                | "device product"
                | "partnumber"
                | "battery name"
                | "component name"
                | "socket part number"
                | "tpm description"
        )
    {
        12
    } else if label.contains("manufacturer") || label == "vendor" || label.ends_with(" vendor") {
        8
    } else {
        0
    }
}

fn score(before: &Entity, after: &Entity) -> u32 {
    if before.family != after.family {
        return 0;
    }
    // Count each evidence category once: duplicate source labels/model aliases must
    // not outweigh a unit serial, nor let a model alone cross the threshold.
    let mut weights = BTreeSet::new();
    for (label, value) in &before.rows {
        if usable(value) && after.rows.iter().any(|(l, v)| l == label && v == value) {
            let weight = identity_weight(label);
            let weight = if weight == 0
                && before.ids.contains(value)
                && after.ids.contains(value)
                && !matches!(
                    label.as_str(),
                    "Device ID" | "DeviceLocator" | "Drive" | "ProcessorId"
                ) {
                30
            } else {
                weight
            };
            weights.insert(weight);
        }
    }
    weights.into_iter().sum()
}

fn model_fields(entity: &Entity) -> Vec<&(String, String)> {
    let mut rows: Vec<_> = entity
        .rows
        .iter()
        .filter(|(label, _)| matches!(identity_weight(label), 8 | 12))
        .collect();
    // A manufacturer alone is not a model. Name/product aliases also cover
    // sections without an explicit Model label (GPU name lines parse as Model).
    if !rows
        .iter()
        .any(|(label, value)| identity_weight(label) == 12 && usable(value))
    {
        rows.clear();
    }
    rows.sort_unstable();
    rows
}

fn match_entities(before: &Section, after: &Section) -> Vec<Option<usize>> {
    let mut matched = vec![None; before.entities.len()];
    if !before.blocked && !after.blocked && matched.len() == 1 && after.entities.len() == 1 {
        matched[0] = Some(0);
        return matched;
    }
    let mut candidates = Vec::new();
    for (i, left) in before.entities.iter().enumerate() {
        for (j, right) in after.entities.iter().enumerate() {
            let score = score(left, right);
            if score >= 20 {
                candidates.push((score, i, j));
            }
        }
    }
    candidates.sort_by_key(|&(score, i, j)| (std::cmp::Reverse(score), i, j));
    let mut used = vec![false; after.entities.len()];
    for &(score, i, j) in &candidates {
        if matched[i].is_some() || used[j] {
            continue;
        }
        // Equal evidence for multiple available devices cannot establish identity.
        // Index order breaks sorting ties only; it must never establish a match.
        if candidates.iter().any(|&(s, a, b)| {
            s == score
                && (a != i || b != j)
                && (a == i || b == j)
                && matched[a].is_none()
                && !used[b]
        }) {
            continue;
        }
        matched[i] = Some(j);
        used[j] = true;
    }
    // Exact content is safe to pair even when all identifiers are generic.
    // Compare multisets, not headings/positions; consume identical twins in order.
    let sorted_rows = |entity: &Entity| {
        let mut rows = entity.rows.clone();
        rows.sort_unstable();
        rows
    };
    let left_rows: Vec<_> = before.entities.iter().map(sorted_rows).collect();
    let right_rows: Vec<_> = after.entities.iter().map(sorted_rows).collect();
    for (i, rows) in left_rows.iter().enumerate() {
        if matched[i].is_none()
            && let Some(j) = right_rows.iter().enumerate().position(|(j, other)| {
                !used[j] && rows == other && before.entities[i].family == after.entities[j].family
            })
        {
            matched[i] = Some(j);
            used[j] = true;
        }
    }
    // A single remaining model can survive a spoof of every unit identifier.
    // Count both sides before assigning: no positional guess within model ties.
    let left_models: Vec<_> = before.entities.iter().map(model_fields).collect();
    let right_models: Vec<_> = after.entities.iter().map(model_fields).collect();
    for (i, model) in left_models.iter().enumerate() {
        if matched[i].is_some()
            || model.is_empty()
            || left_models
                .iter()
                .enumerate()
                .filter(|(a, other)| {
                    matched[*a].is_none()
                        && *other == model
                        && before.entities[i].family == before.entities[*a].family
                })
                .count()
                != 1
        {
            continue;
        }
        let mut remaining = right_models.iter().enumerate().filter(|(j, other)| {
            !used[*j] && *other == model && before.entities[i].family == after.entities[*j].family
        });
        if let Some((j, _)) = remaining.next()
            && remaining.next().is_none()
        {
            matched[i] = Some(j);
            used[j] = true;
        }
    }
    matched
}

fn fields(before: Option<&Entity>, after: Option<&Entity>) -> Vec<FieldChange> {
    let before_ids = before.map_or(&[][..], |e| e.ids.as_slice());
    let after_ids = after.map_or(&[][..], |e| e.ids.as_slice());
    let before = before.map_or(&[][..], |e| e.rows.as_slice());
    let after = after.map_or(&[][..], |e| e.rows.as_slice());
    let mut used = vec![false; after.len()];
    let mut pairs = vec![None; before.len()];
    // Reserve all exact multiset intersections before pairing changed values.
    for (i, row) in before.iter().enumerate() {
        if let Some(j) = after
            .iter()
            .enumerate()
            .position(|(j, other)| !used[j] && row == other)
        {
            pairs[i] = Some(j);
            used[j] = true;
        }
    }
    let mut result = Vec::new();
    for (i, (label, value)) in before.iter().enumerate() {
        let j = pairs[i].or_else(|| {
            after
                .iter()
                .enumerate()
                .position(|(j, (l, _))| !used[j] && l == label)
        });
        let (kind, other) = if let Some(j) = j {
            used[j] = true;
            (
                if value == &after[j].1 {
                    Kind::Same
                } else {
                    Kind::Changed
                },
                Some(after[j].1.clone()),
            )
        } else {
            (Kind::Removed, None)
        };
        result.push(FieldChange {
            identifier: other.as_ref().is_some_and(|other| {
                label != "ProcessorId"
                    && (identifier_label(label)
                        || before_ids.contains(value)
                        || after_ids.contains(other))
            }),
            label: label.clone(),
            before: Some(value.clone()),
            after: other,
            kind,
        });
    }
    for (j, (label, value)) in after.iter().enumerate() {
        if !used[j] {
            result.push(FieldChange {
                label: label.clone(),
                before: None,
                after: Some(value.clone()),
                kind: Kind::Added,
                identifier: false,
            });
        }
    }
    result
}

fn entity_change(
    title: &str,
    before: Option<(usize, &Entity)>,
    after: Option<(usize, &Entity)>,
    is_header: bool,
) -> EntityChange {
    let fields = fields(before.map(|(_, e)| e), after.map(|(_, e)| e));
    let kind = match (before, after) {
        (None, _) => Kind::Added,
        (_, None) => Kind::Removed,
        (Some((i, _)), Some((j, _))) if i != j => Kind::Moved,
        _ if fields.iter().any(|f| f.kind != Kind::Same) => Kind::Changed,
        _ => Kind::Same,
    };
    EntityChange {
        section: title.to_owned(),
        before_position: before.map(|(i, _)| i),
        after_position: after.map(|(i, _)| i),
        before_name: before.map(|(_, e)| e.name.clone()),
        after_name: after.map(|(_, e)| e.name.clone()),
        kind,
        fields,
        is_header,
    }
}

/// Matches device identities, then compares exact labels and value multisets.
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
    let mut entities = Vec::new();
    let warning = if before.masked != after.masked {
        let warning =
            "Cannot compare masked and unmasked values. Export both sides with Mask IDs off."
                .to_owned();
        text.push_str(&warning);
        text.push_str("\r\n");
        return Comparison {
            summary: "Comparison unavailable: mask mismatch".into(),
            text,
            entities,
            warning: Some(warning),
        };
    } else if before.masked {
        Some("Both inputs are masked. Hidden identifiers cannot establish identity or prove equality.".to_owned())
    } else {
        None
    };
    if let Some(warning) = &warning {
        text.push_str(warning);
        text.push_str("\r\n\r\n");
    }
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
        let start = entities.len();
        let header_before = before
            .filter(|s| !s.header.rows.is_empty())
            .map(|s| (0, &s.header));
        let header_after = after
            .filter(|s| !s.header.rows.is_empty())
            .map(|s| (0, &s.header));
        if header_before.is_some() || header_after.is_some() {
            entities.push(entity_change(title, header_before, header_after, true));
        }
        let matched = match (before, after) {
            (Some(left), Some(right)) => match_entities(left, right),
            _ => vec![None; before.map_or(0, |s| s.entities.len())],
        };
        let mut used = BTreeSet::new();
        if let Some(before) = before {
            for (i, entity) in before.entities.iter().enumerate() {
                let other = matched[i].and_then(|j| after.map(|s| (j, &s.entities[j])));
                if let Some((j, _)) = other {
                    used.insert(j);
                }
                entities.push(entity_change(title, Some((i, entity)), other, false));
            }
        }
        if let Some(after) = after {
            for (j, entity) in after.entities.iter().enumerate() {
                if !used.contains(&j) {
                    entities.push(entity_change(title, None, Some((j, entity)), false));
                }
            }
        }
        let groups = &entities[start..];
        let rows: Vec<_> = groups
            .iter()
            .flat_map(|e| &e.fields)
            .map(|f| {
                let value = match (&f.before, &f.after) {
                    (Some(old), Some(new)) if f.kind == Kind::Changed => {
                        format!("{old}  ->  {new}")
                    }
                    (_, Some(value)) | (Some(value), _) => value.clone(),
                    _ => String::new(),
                };
                (f.kind, &f.label, value)
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        text.push_str(title);
        text.push_str("\r\n");
        for entity in groups.iter().filter(|e| e.kind == Kind::Moved) {
            if let (Some(i), Some(j)) = (entity.before_position, entity.after_position) {
                text.push_str(&format!("  moved   Entity {}  ->  {}\r\n", i + 1, j + 1));
            }
        }
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
    let mut summary = format!(
        "Changed {} · Added {} · Removed {} · Same {}",
        counts[0], counts[2], counts[1], counts[3]
    );
    let moved = entities.iter().filter(|e| e.kind == Kind::Moved).count();
    if moved > 0 {
        summary.push_str(&format!(" · Moved {moved}"));
    }
    Comparison {
        summary,
        text,
        entities,
        warning,
    }
}

#[cfg(test)]
mod tests;
