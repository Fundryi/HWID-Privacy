//! C2b presentation of the identity engine's matches; matching stays in report::compare.

use crate::report::compare::{Comparison, EntityChange, Export, FieldChange, Kind};
use crate::report::pad_right_utf16;
use std::path::Path;

const FIELD_ORDER: [Kind; 4] = [Kind::Changed, Kind::Removed, Kind::Added, Kind::Same];

pub(crate) fn kind_text(kind: Kind) -> &'static str {
    match kind {
        Kind::Changed => "changed",
        Kind::Moved => "moved",
        Kind::Removed => "removed",
        Kind::Added => "added",
        Kind::Same => "same",
    }
}

pub(super) fn position(before: &str, after: &str, kind: Kind) -> String {
    if kind != Kind::Moved {
        return if before.is_empty() { after } else { before }.to_owned();
    }
    let prefix = before.trim_end_matches(|c: char| c.is_ascii_digit());
    let after_prefix = after.trim_end_matches(|c: char| c.is_ascii_digit());
    let destination = if prefix == after_prefix && prefix.len() < before.len() {
        &after[prefix.len()..]
    } else {
        after
    };
    format!("{before} -> {destination}")
}

fn fields(text: &mut String, rows: &[&FieldChange], indent: &str, width: usize) {
    for kind in FIELD_ORDER {
        for field in rows.iter().filter(|f| f.kind == kind) {
            let value = match (&field.before, &field.after) {
                (Some(before), Some(after)) if kind == Kind::Changed => {
                    format!("{before}  ->  {after}")
                }
                (_, Some(value)) | (Some(value), _) => value.clone(),
                _ => String::new(),
            };
            text.push_str(&format!(
                "{indent}{:8}{}{value}\r\n",
                kind_text(kind),
                pad_right_utf16(&field.label, width)
            ));
        }
    }
}

pub(super) fn format(
    result: &mut Comparison,
    before: &Export,
    after: &Export,
    before_path: &Path,
    after_path: &Path,
) {
    let mut text = format!(
        "Before: {}\r\nAfter:  {}\r\n\r\n",
        before_path.display(),
        after_path.display()
    );
    if let Some(warning) = &result.warning {
        text.push_str(warning);
        text.push_str("\r\n\r\n");
    }
    let mut counts = [0usize; 4];
    for field in result.entities.iter().flat_map(|e| &e.fields) {
        if let Some(index) = FIELD_ORDER.iter().position(|kind| *kind == field.kind) {
            counts[index] += 1;
        }
    }
    let moved = result
        .entities
        .iter()
        .filter(|e| e.kind == Kind::Moved)
        .count();
    result.summary = format!(
        "Changed {} · Moved {moved} · Added {} · Removed {} · Same {}",
        counts[0], counts[2], counts[1], counts[3]
    );

    let mut remaining = result.entities.as_slice();
    while let Some(first) = remaining.first() {
        let end = remaining
            .iter()
            .position(|e| e.section != first.section)
            .unwrap_or(remaining.len());
        let (section, rest) = remaining.split_at(end);
        remaining = rest;
        let rows: Vec<_> = section.iter().flat_map(|e| &e.fields).collect();
        if rows.is_empty() {
            continue;
        }
        text.push_str(&first.section);
        text.push_str("\r\n");
        if section.iter().all(|e| e.kind == Kind::Same) {
            text.push_str(&format!(
                "  same     {} values, none changed\r\n\r\n",
                rows.len()
            ));
            continue;
        }
        let label_width = rows
            .iter()
            .map(|f| f.label.encode_utf16().count())
            .max()
            .unwrap_or(0)
            + 2;
        let mut flat = Vec::new();
        let mut devices: Vec<(&EntityChange, String, &str)> = Vec::new();
        for entity in section {
            let left = before.device(&entity.section, entity.before_position);
            let right = after.device(&entity.section, entity.after_position);
            if entity.is_header || (left.is_none() && right.is_none()) {
                flat.extend(entity.fields.iter());
            } else {
                let (old, old_name) = left.unwrap_or_default();
                let (new, new_name) = right.unwrap_or_default();
                devices.push((
                    entity,
                    position(old, new, entity.kind),
                    if right.is_some() { new_name } else { old_name },
                ));
            }
        }
        fields(&mut text, &flat, "  ", label_width);
        let position_width = devices
            .iter()
            .map(|(_, p, _)| p.encode_utf16().count())
            .max()
            .unwrap_or(0)
            + 2;
        for (entity, heading, name) in devices {
            let heading = pad_right_utf16(&heading, position_width);
            if entity.kind == Kind::Same {
                text.push_str(&format!(
                    "  same    {heading}{} values, none changed\r\n",
                    entity.fields.len()
                ));
                continue;
            }
            text.push_str(&format!(
                "  {:8}{heading}{name}\r\n",
                kind_text(entity.kind)
            ));
            if entity.kind != Kind::Moved || entity.fields.iter().any(|f| f.kind != Kind::Same) {
                fields(
                    &mut text,
                    &entity.fields.iter().collect::<Vec<_>>(),
                    "    ",
                    label_width,
                );
            }
        }
        text.push_str("\r\n");
    }
    result.text = text;
}
