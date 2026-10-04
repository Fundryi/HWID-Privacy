//! UTF-16 conversion shared by Windows helpers.

/// Encodes UTF-16 and appends a terminating NUL.
pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Decodes through the first NUL, replacing malformed UTF-16 with U+FFFD.
pub fn from_wide(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_empty_and_surrogate_pairs() {
        assert_eq!(to_wide(""), [0]);
        assert_eq!(to_wide("A🦀"), [65, 0xD83E, 0xDD80, 0]);
    }

    #[test]
    fn stops_at_nul_and_accepts_unterminated_input() {
        assert_eq!(from_wide(&[65, 0, 66]), "A");
        assert_eq!(from_wide(&[65, 66]), "AB");
        assert_eq!(from_wide(&[]), "");
    }

    #[test]
    fn malformed_utf16_is_replaced() {
        assert_eq!(from_wide(&[0xD800, 65, 0xDC00]), "�A�");
    }
}
