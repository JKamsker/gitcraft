//! Which reparse points are links: only name surrogates (symbolic links, junctions/mount
//! points, WSL links) name another object. Cloud Files placeholders, Dedup and other
//! non-surrogate reparse points are the file itself and keep their policy as usual.
// Elsewhere only its tests use it.
#![cfg_attr(not(windows), allow(dead_code))]

/// `FILE_ATTRIBUTE_REPARSE_POINT`.
pub(crate) const REPARSE_POINT: u32 = 0x0000_0400;
/// `IsReparseTagNameSurrogate`: bit 29 of the tag.
const NAME_SURROGATE: u32 = 0x2000_0000;

/// Whether an object with these attributes and reparse tag stands for another name.
pub(crate) fn is_link(attributes: u32, tag: u32) -> bool {
    attributes & REPARSE_POINT != 0 && tag & NAME_SURROGATE != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_name_surrogate_reparse_points_are_links() {
        for (tag, link) in [
            (0xA000_000C, true),  // IO_REPARSE_TAG_SYMLINK
            (0xA000_0003, true),  // IO_REPARSE_TAG_MOUNT_POINT (junctions)
            (0xA000_001D, true),  // IO_REPARSE_TAG_LX_SYMLINK (WSL)
            (0x2000_1234, true),  // a third-party name surrogate
            (0x9000_001A, false), // IO_REPARSE_TAG_CLOUD (OneDrive placeholder)
            (0x9000_601A, false), // IO_REPARSE_TAG_CLOUD_6
            (0x8000_0013, false), // IO_REPARSE_TAG_DEDUP
            (0x8000_0023, false), // IO_REPARSE_TAG_AF_UNIX (WSL socket)
            (0x8000_001B, false), // IO_REPARSE_TAG_APPEXECLINK
            (0x0000_1234, false), // a third-party non-surrogate
        ] {
            assert_eq!(is_link(REPARSE_POINT | 0x20, tag), link, "{tag:#x}");
        }
        assert!(!is_link(0x20, 0xA000_000C), "no reparse attribute, no link");
    }
}
