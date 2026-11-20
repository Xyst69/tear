//! Convert archive member names into relative Windows paths before planning any writes.
use std::io::{Error, ErrorKind, Result};
use std::path::PathBuf;

/// Converts the archive's separator conventions and preserves ambiguous characters
/// as percent-encoded bytes. Existing percent signs are encoded for reversibility.
pub fn relative_name(name: &str) -> Result<PathBuf> {
    let invalid = || {
        Error::new(
            ErrorKind::InvalidData,
            "expected a relative archive member name",
        )
    };
    if name.contains('\0') || name.starts_with(['/', '\\']) {
        return Err(invalid());
    }
    let mut path = PathBuf::new();
    for component in name.split(['/', '\\']) {
        if component.is_empty() || component == "." {
            continue;
        }
        let bytes = component.as_bytes();
        if component == ".."
            || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        {
            return Err(invalid());
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "AUX" | "PRN" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|n| {
                matches!(
                    n,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        let trimmed = component.trim_end_matches([' ', '.']).len();
        let mut safe = String::new();
        for (index, c) in component.char_indices() {
            if index >= trimmed || (device && index == 0) || c < ' ' || "<>:\"|?*%".contains(c) {
                const HEX: &[u8] = b"0123456789ABCDEF";
                for byte in c.encode_utf8(&mut [0; 4]).bytes() {
                    safe.push('%');
                    safe.push(HEX[(byte >> 4) as usize] as char);
                    safe.push(HEX[(byte & 15) as usize] as char);
                }
            } else {
                safe.push(c);
            }
        }
        path.push(safe);
    }
    if path.as_os_str().is_empty() {
        return Err(invalid());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_relative_names_and_nul() {
        for name in [
            "",
            ".",
            "..",
            "folder/../note",
            "/tmp",
            "\\\\server\\share",
            "D:relative",
            "dir/C:relative",
            "a\0b",
        ] {
            assert_eq!(
                relative_name(name).expect_err(name).kind(),
                ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn escapes_devices_special_characters_and_trailing_spaces() {
        for (name, result) in [
            ("con.txt", "%63on.txt"),
            ("NUL .bin", "%4EUL .bin"),
            ("LPT²", "%4CPT²"),
            ("readme. ", "readme%2E%20"),
            ("%2E", "%252E"),
            ("a*b", "a%2Ab"),
            ("parts/./AUX", "parts/%41UX"),
            ("a\\b", "a/b"),
        ] {
            assert_eq!(relative_name(name).expect(name), PathBuf::from(result));
        }
    }
}
