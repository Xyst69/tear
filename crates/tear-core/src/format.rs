//! Header recognition for the outer container. Compressed tar has a second header
//! inside its compression stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    SevenZip,
    Rar4,
    Rar5,
    Tar,
    Gzip,
    Xz,
    Zstd,
    Bzip2,
}

impl Format {
    /// Recognizes a leading header. An empty tar requires its two zero end blocks.
    pub fn from_prefix(prefix: &[u8]) -> Option<Self> {
        for (magic, kind) in [
            (&b"PK\x03\x04"[..], Self::Zip),
            (b"PK\x05\x06", Self::Zip),
            (b"PK\x07\x08", Self::Zip),
            (b"7z\xbc\xaf\x27\x1c", Self::SevenZip),
            (b"Rar!\x1a\x07\x01\x00", Self::Rar5),
            (b"Rar!\x1a\x07\x00", Self::Rar4),
            (b"\x1f\x8b\x08", Self::Gzip),
            (b"\xfd7zXZ\x00", Self::Xz),
            (b"\x28\xb5\x2f\xfd", Self::Zstd),
        ] {
            if prefix.starts_with(magic) {
                return Some(kind);
            }
        }
        if prefix.starts_with(b"BZh") && matches!(prefix.get(3), Some(b'1'..=b'9')) {
            return Some(Self::Bzip2);
        }
        if let Some(header) = prefix.get(..512) {
            if header.iter().all(|&b| b == 0) {
                return prefix
                    .get(512..1024)
                    .filter(|block| block.iter().all(|&b| b == 0))
                    .map(|_| Self::Tar);
            }
            let sum: u32 = header[..148]
                .iter()
                .chain(&header[156..])
                .map(|&b| u32::from(b))
                .sum::<u32>()
                + 8 * u32::from(b' ');
            let stored = std::str::from_utf8(&header[148..156])
                .ok()?
                .trim_matches([' ', '\0']);
            if u32::from_str_radix(stored, 8).ok() == Some(sum) {
                return Some(Self::Tar);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_inputs_and_empty_tar() {
        for n in 0..1024 {
            assert_eq!(Format::from_prefix(&vec![0; n]), None);
        }
        assert_eq!(Format::from_prefix(&[0; 1024]), Some(Format::Tar));
        assert_eq!(Format::from_prefix(b"BZh9"), Some(Format::Bzip2));
        assert_eq!(Format::from_prefix(b"BZh0"), None);
        assert_eq!(Format::from_prefix(b"PK\x05\x06"), Some(Format::Zip));
    }
}
