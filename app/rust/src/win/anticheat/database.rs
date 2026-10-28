//! Shared build/runtime parser for the single, versioned detection inventory.

use serde::Deserialize;
use std::collections::HashSet;

pub const SOURCE_PATH: &str = "assets/anticheat-database.json";
pub const EMBEDDED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/anticheat-database.json"
));

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum Gate {
    #[serde(rename = "MUST_GATE")]
    Must,
    #[serde(rename = "SHOULD_GATE")]
    Should,
    #[serde(rename = "DONT_GATE")]
    Dont,
}

#[derive(Debug, Deserialize)]
pub struct Driver {
    pub file: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct AntiCheat {
    pub id: String,
    pub name: String,
    pub gate: Gate,
    pub kernel_mode: bool,
    pub boot_resident: bool,
    pub services: Vec<String>,
    pub drivers: Vec<Driver>,
    pub processes: Vec<String>,
    pub games_sample: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Database {
    pub version: u32,
    pub anticheats: Vec<AntiCheat>,
}

impl Database {
    pub fn parse(json: &str) -> Result<Self, String> {
        // Check the envelope first so a future schema fails with a version error.
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        let version: Version = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if version.version != 1 {
            return Err(format!(
                "unsupported anti-cheat database version {}",
                version.version
            ));
        }
        let database: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        for entry in &database.anticheats {
            if entry.id.trim().is_empty() || entry.name.trim().is_empty() {
                return Err("anti-cheat id and name must be nonempty".into());
            }
            if !ids.insert(&entry.id) {
                return Err(format!("duplicate anti-cheat id {}", entry.id));
            }
            for name in &entry.services {
                let exact = name.strip_suffix('*').unwrap_or(name);
                if !valid_name(exact) || exact.contains('*') {
                    return Err(format!("invalid service matcher for {}", entry.id));
                }
            }
            for name in entry
                .drivers
                .iter()
                .map(|d| &d.file)
                .chain(&entry.processes)
            {
                if !valid_name(name) || name.contains('*') {
                    return Err(format!("invalid basename matcher for {}", entry.id));
                }
            }
        }
        Ok(database)
    }
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.encode_utf16().count() <= 256
        && !name.contains(['\0', '\\', '/', ':'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_database_and_invalid_schema_are_checked() {
        let database = Database::parse(EMBEDDED).unwrap();
        assert!(!database.anticheats.is_empty());
        let original: serde_json::Value = serde_json::from_str(EMBEDDED).unwrap();
        for (field, value, expected) in [
            ("version", serde_json::json!(2), "unsupported"),
            ("version", serde_json::json!("1"), "invalid type"),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            assert!(
                Database::parse(&changed.to_string())
                    .unwrap_err()
                    .contains(expected)
            );
        }
        for gate in ["MUST", "must_gate", "", "DONT_GATE*"] {
            let mut changed = original.clone();
            changed["anticheats"][0]["gate"] = serde_json::json!(gate);
            assert!(
                Database::parse(&changed.to_string())
                    .unwrap_err()
                    .contains("unknown variant")
            );
        }
        let mut duplicate = original.clone();
        duplicate["anticheats"][1]["id"] = duplicate["anticheats"][0]["id"].clone();
        assert!(
            Database::parse(&duplicate.to_string())
                .unwrap_err()
                .contains("duplicate")
        );
        for field in ["id", "name", "gate", "services", "drivers", "processes"] {
            let mut changed = original.clone();
            changed["anticheats"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(Database::parse(&changed.to_string()).is_err());
        }
        for matcher in [
            "*",
            "Easy*Cheat",
            "Easy**",
            "",
            "C:\\game.exe",
            "name\0suffix",
        ] {
            let mut changed = original.clone();
            changed["anticheats"][0]["services"] = serde_json::json!([matcher]);
            assert!(Database::parse(&changed.to_string()).is_err());
        }
        assert!(Database::parse("{").is_err());
    }
}
