//! C2c presentation state. Native controls own painting and input; matching stays in report.

use super::text;
use crate::report::compare::{Comparison, Export, Kind};
use crate::report::{self, Section};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowId {
    Notice,
    Section(usize),
    Device(usize),
    Field(usize, usize),
    More(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub id: RowId,
    pub line: String,
    pub label: String,
    pub position: String,
    pub before: String,
    pub after: String,
    pub kind: Kind,
    pub identifier: bool,
    pub not_unique: bool,
    pub generic: [bool; 2],
    pub flat: bool,
    pub expanded: bool,
    pub counts: [usize; 2],
    pub extras: [usize; 4],
    pub unchanged: Option<usize>,
}

impl Row {
    fn new(id: RowId, label: String) -> Self {
        Self {
            id,
            line: label.clone(),
            label,
            position: String::new(),
            before: String::new(),
            after: String::new(),
            kind: Kind::Same,
            identifier: false,
            not_unique: false,
            generic: [false; 2],
            flat: false,
            expanded: false,
            counts: [0; 2],
            extras: [0; 4],
            unchanged: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Group {
    row: Row,
    // None follows the current filter's defaults; explicit choices survive switches.
    open: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
struct Device {
    group: Group,
    section: usize,
    fields: Vec<Row>,
    show_all: bool,
}

/// Shared form/list state for the compare table, including stable row identities.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub(crate) rows: Vec<Row>,
    sections: Vec<Group>,
    devices: Vec<Device>,
    notice: Option<String>,
    pub(crate) all: bool,
    pub(crate) counts: [usize; 2],
    pub(crate) extras: [usize; 4],
    pub(crate) filters: [bool; 4],
}

impl Table {
    /// Every value pair, including collapsed rows and fields hidden by the current filter.
    pub(crate) fn values(&self) -> impl Iterator<Item = [&str; 2]> {
        self.devices
            .iter()
            .flat_map(|device| &device.fields)
            .map(|row| [row.before.as_str(), row.after.as_str()])
    }

    pub(crate) fn verdicts(&self) -> bool {
        self.notice.is_none()
    }

    pub(super) fn new(result: &Comparison, before: &Export, after: &Export) -> Self {
        let mut table = Self {
            rows: Vec::new(),
            sections: Vec::new(),
            devices: Vec::new(),
            notice: result.warning.clone(),
            all: false,
            counts: [0; 2],
            extras: [0; 4],
            filters: [true, true, false, false],
        };
        for entity in &result.entities {
            if table
                .sections
                .last()
                .is_none_or(|s| s.row.label != entity.section)
            {
                let id = table.sections.len();
                table.sections.push(Group {
                    row: Row::new(RowId::Section(id), entity.section.clone()),
                    open: None,
                });
            }
            let section = table.sections.len() - 1;
            let index = table.devices.len();
            let left = before.device(&entity.section, entity.before_position);
            let right = after.device(&entity.section, entity.after_position);
            let (old, old_name) = left.unwrap_or_default();
            let (new, new_name) = right.unwrap_or_default();
            let mut row = Row::new(
                RowId::Device(index),
                if right.is_some() { new_name } else { old_name }.to_owned(),
            );
            row.position = text::position(old, new, entity.kind);
            if entity.kind == Kind::Moved {
                row.position = format!(
                    "moved {} → {}",
                    trailing_position(old),
                    trailing_position(new)
                );
            }
            row.kind = entity.kind;
            row.flat = entity.is_header || (left.is_none() && right.is_none());
            row.line = format!(
                "{}  {}  {}",
                text::kind_text(row.kind),
                text::position(old, new, entity.kind),
                row.label
            );
            let fields: Vec<_> = entity
                .fields
                .iter()
                .enumerate()
                .map(|(field_index, f)| {
                    let mut field = Row::new(RowId::Field(index, field_index), f.label.clone());
                    field.before = f.before.clone().unwrap_or_default();
                    field.after = f.after.clone().unwrap_or_default();
                    field.kind = f.kind;
                    field.identifier = f.identifier;
                    field.not_unique = f.not_unique();
                    field.generic = [
                        f.before
                            .as_deref()
                            .is_some_and(crate::report::compare::generic_value),
                        f.after
                            .as_deref()
                            .is_some_and(crate::report::compare::generic_value),
                    ];
                    field.flat = row.flat;
                    field.line = format!(
                        "{}  {}  {}",
                        if field.not_unique {
                            "not unique"
                        } else {
                            text::kind_text(f.kind)
                        },
                        f.label,
                        if f.kind == Kind::Changed {
                            format!("{}  ->  {}", field.before, field.after)
                        } else if f.after.is_some() {
                            field.after.clone()
                        } else {
                            field.before.clone()
                        }
                    );
                    field
                })
                .collect();
            for field in &fields {
                if table.notice.is_none() && field.not_unique {
                    row.extras[0] += 1;
                }
                if table.notice.is_none() && field.identifier && !field.not_unique {
                    let count = match field.kind {
                        Kind::Same => Some(0),
                        Kind::Changed => Some(1),
                        _ => None,
                    };
                    if let Some(count) = count {
                        row.counts[count] += 1;
                        table.sections[section].row.counts[count] += 1;
                        table.counts[count] += 1;
                    }
                }
            }
            match row.kind {
                Kind::Added => row.extras[1] = 1,
                Kind::Removed => row.extras[2] = 1,
                Kind::Moved => row.extras[3] = 1,
                _ => {}
            }
            for (i, count) in row.extras.iter().enumerate() {
                table.extras[i] += count;
                table.sections[section].row.extras[i] += count;
            }
            table.devices.push(Device {
                group: Group { row, open: None },
                section,
                fields,
                show_all: false,
            });
        }
        for (i, section) in table.sections.iter_mut().enumerate() {
            let devices: Vec<_> = table.devices.iter().filter(|d| d.section == i).collect();
            if devices.iter().all(|d| d.group.row.kind == Kind::Same) {
                section.row.unchanged = Some(devices.iter().map(|d| d.fields.len()).sum());
            }
        }
        table.rebuild();
        table
    }

    pub(super) fn mask(&mut self, ids: &[String]) {
        let mask = |value: &str| {
            report::masked(&Section {
                body: value.to_owned(),
                ids: ids.to_vec(),
                ..Section::default()
            })
            .body
        };
        for device in &mut self.devices {
            device.group.row.label = mask(&device.group.row.label);
            device.group.row.line = mask(&device.group.row.line);
            device.group.row.position = mask(&device.group.row.position);
            for field in &mut device.fields {
                field.before = mask(&field.before);
                field.after = mask(&field.after);
                field.label = mask(&field.label);
                field.line = mask(&field.line);
            }
        }
        self.rebuild();
    }

    pub(crate) fn rebuild(&mut self) {
        self.rows.clear();
        if let Some(notice) = &self.notice {
            self.rows.push(Row::new(RowId::Notice, notice.clone()));
        }
        for (section_index, section) in self.sections.iter().enumerate() {
            let mut row = section.row.clone();
            let visible = self
                .devices
                .iter()
                .filter(|d| d.section == section_index)
                .any(|d| d.fields.iter().any(|f| self.includes(d, f)));
            if !visible {
                continue;
            }
            row.expanded = section.open.unwrap_or(section.row.unchanged.is_none());
            self.rows.push(row.clone());
            if !row.expanded {
                continue;
            }
            // Flat/header fields precede devices, preserving engine order within each group.
            for flat in [true, false] {
                for (device_index, device) in self
                    .devices
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.section == section_index && d.group.row.flat == flat)
                {
                    if !device.fields.iter().any(|f| self.includes(device, f)) {
                        continue;
                    }
                    let mut row = device.group.row.clone();
                    row.expanded = device.group.open.unwrap_or(true);
                    if !flat {
                        self.rows.push(row.clone());
                        if !row.expanded {
                            continue;
                        }
                    }
                    let mut fields: Vec<_> = device.fields.iter().collect();
                    fields.sort_by_key(|f| {
                        if f.identifier {
                            0
                        } else {
                            match f.kind {
                                Kind::Changed => 1,
                                Kind::Removed => 2,
                                Kind::Added => 3,
                                _ => 4,
                            }
                        }
                    });
                    let mut hidden = 0;
                    for field in fields {
                        if !self.includes(device, field) {
                            continue;
                        }
                        if !self.all
                            && !device.show_all
                            && !(self.filters[3]
                                && matches!(
                                    device.group.row.kind,
                                    Kind::Added | Kind::Removed | Kind::Moved
                                ))
                            && !field.not_unique
                            && !field.identifier
                            && field.kind == Kind::Same
                        {
                            hidden += 1;
                        } else {
                            self.rows.push(field.clone());
                        }
                    }
                    if hidden > 0 {
                        let mut more = Row::new(
                            RowId::More(device_index),
                            format!("{hidden} more values, unchanged"),
                        );
                        more.flat = flat;
                        self.rows.push(more);
                    }
                }
            }
        }
    }

    fn includes(&self, device: &Device, field: &Row) -> bool {
        self.all
            || (self.filters[3] && matches!(device.group.row.kind, Kind::Added | Kind::Removed | Kind::Moved))
            || (self.filters[2] && field.not_unique)
            || (!field.not_unique && field.identifier && match field.kind {
                Kind::Changed => self.filters[0],
                Kind::Same => self.filters[1],
                _ => false,
            })
            // Neutral facts stay behind the explicit More affordance for a visible group.
            || (field.kind == Kind::Same && !field.identifier && device.fields.iter().any(|f|
                !f.not_unique && f.identifier && ((f.kind == Kind::Changed && self.filters[0])
                    || (f.kind == Kind::Same && self.filters[1]))))
    }

    pub(crate) fn labels(&self) -> impl Iterator<Item = &str> {
        self.devices
            .iter()
            .flat_map(|d| &d.fields)
            .map(|r| r.label.as_str())
    }

    /// Updates collapse state and returns the logical row that should remain selected.
    pub(crate) fn activate(&mut self, id: RowId, expand: Option<bool>) -> RowId {
        match id {
            RowId::Section(i) => {
                let group = &mut self.sections[i];
                group.open =
                    Some(expand.unwrap_or(!group.open.unwrap_or(group.row.unchanged.is_none())));
            }
            RowId::Device(i) => {
                let group = &mut self.devices[i].group;
                group.open = Some(expand.unwrap_or(!group.open.unwrap_or(true)));
            }
            RowId::More(i) if expand != Some(false) => self.devices[i].show_all = true,
            RowId::Field(i, _) | RowId::More(i) if expand == Some(false) => {
                return if self.devices[i].group.row.flat {
                    RowId::Section(self.devices[i].section)
                } else {
                    RowId::Device(i)
                };
            }
            _ => {}
        }
        self.rebuild();
        if let RowId::More(i) = id {
            self.rows
                .iter()
                .find(|r| {
                    matches!(r.id, RowId::Field(d, _) if d == i)
                        && !r.identifier
                        && r.kind == Kind::Same
                })
                .map_or(RowId::Device(i), |r| r.id)
        } else {
            id
        }
    }

    pub(crate) fn parent(&self, id: RowId) -> Option<RowId> {
        match id {
            RowId::Field(i, _) | RowId::More(i) if !self.devices[i].group.row.flat => {
                Some(RowId::Device(i))
            }
            RowId::Field(i, _) | RowId::More(i) | RowId::Device(i) => {
                Some(RowId::Section(self.devices[i].section))
            }
            _ => None,
        }
    }
}

pub(crate) fn legend(counts: [usize; 2]) -> [String; 3] {
    if counts == [0; 2] {
        return [
            "No identifiers found".to_owned(),
            String::new(),
            String::new(),
        ];
    }
    [
        format!("{} unchanged", counts[0]),
        " · ".to_owned(),
        format!("{} changed", counts[1]),
    ]
}

fn trailing_position(heading: &str) -> &str {
    let heading = heading.trim_end();
    let start = heading
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, _)| i);
    start.map_or(heading, |i| &heading[i..])
}
