//! Release signatures (ed25519). Shared by the browser (verifies updates) and the
//! `viima_sign` tool (signs releases in CI), so both build the exact same message.

/// What a release signature covers: the version (so an old, vulnerable build can't be
/// replayed as a "newer" one) and the SHA-256 of Viima.exe.
pub fn signed_message(version: &str, sha256_hex: &str) -> Vec<u8> {
    format!(
        "viima-update-v1\n{}\n{}",
        version.trim().trim_start_matches(['v', 'V']),
        sha256_hex.trim().to_ascii_lowercase()
    )
    .into_bytes()
}

pub fn from_hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    let s = s.trim();
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[allow(dead_code)] // used by viima_sign and the tests, not by the browser itself
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_and_message() {
        assert_eq!(from_hex::<2>("0aFf"), Some([0x0a, 0xff]));
        assert_eq!(from_hex::<2>("0aF"), None);
        assert_eq!(from_hex::<1>("zz"), None);
        assert_eq!(to_hex(&[0x0a, 0xff]), "0aff");
        assert_eq!(signed_message("v1.2.3", "AB"), signed_message("1.2.3 ", "ab"));
    }
}
