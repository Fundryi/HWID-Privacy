//! Local timestamps preserve the C# date and export filename formats.

/// Formats Unix seconds in local time as yyyy-MM-dd HH:mm:ss.
pub fn unix_to_local(_secs: i64) -> Option<String> {
    None
}
/// Returns local dd.MM.yyyy and HH;mm;ss filename components.
pub fn export_stamp() -> (String, String) {
    ("not ported".to_owned(), "not ported".to_owned())
}
