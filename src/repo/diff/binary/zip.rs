//! Reading ZIP archives (and what is built on them: .docx, .xlsx, .pptx, .odt, .jar…) far
//! enough to list their members and inflate a few: the central directory at the end of the
//! file, then the members asked for.

/// A member of an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub size: u64,
    pub compressed: u64,
    /// CRC-32 of its contents: two members with the same are the same.
    pub crc: u32,
    method: u16,
    offset: u64,
}

/// The largest member inflated, so a hostile archive can't fill the memory.
const MAX_INFLATED: usize = 16 * 1024 * 1024;
/// The most members listed.
const MAX_MEMBERS: usize = 100_000;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Whether `bytes` start like a ZIP archive.
pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// The members of the archive, from its central directory; `None` when it has none that can
/// be read.
pub fn members(bytes: &[u8]) -> Option<Vec<Member>> {
    // The end-of-central-directory record: in the last 64 KB + 22 bytes.
    let from = bytes.len().saturating_sub(65_557);
    let eocd = (from..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes[i..].starts_with(b"PK\x05\x06"))?;
    let count = usize::from(u16_at(bytes, eocd + 10)?);
    let mut at = u32_at(bytes, eocd + 16)? as usize;
    let mut out = Vec::new();
    for _ in 0..count.min(MAX_MEMBERS) {
        if !bytes.get(at..)?.starts_with(b"PK\x01\x02") {
            break;
        }
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let compressed = u64::from(u32_at(bytes, at + 20)?);
        let size = u64::from(u32_at(bytes, at + 24)?);
        let name_len = usize::from(u16_at(bytes, at + 28)?);
        let extra_len = usize::from(u16_at(bytes, at + 30)?);
        let comment_len = usize::from(u16_at(bytes, at + 32)?);
        let offset = u64::from(u32_at(bytes, at + 42)?);
        let name = bytes.get(at + 46..at + 46 + name_len)?;
        out.push(Member {
            name: String::from_utf8_lossy(name).into_owned(),
            size,
            compressed,
            crc,
            method,
            offset,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Some(out)
}

/// The contents of `member` (stored or deflated), at most [`MAX_INFLATED`] bytes.
pub fn read(bytes: &[u8], member: &Member) -> Option<Vec<u8>> {
    let at = usize::try_from(member.offset).ok()?;
    if !bytes.get(at..)?.starts_with(b"PK\x03\x04") {
        return None;
    }
    let name_len = usize::from(u16_at(bytes, at + 26)?);
    let extra_len = usize::from(u16_at(bytes, at + 28)?);
    let start = at + 30 + name_len + extra_len;
    let len = usize::try_from(member.compressed).ok()?;
    let data = bytes.get(start..start.checked_add(len)?)?;
    match member.method {
        0 => Some(data.get(..data.len().min(MAX_INFLATED))?.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_INFLATED).ok(),
        _ => None,
    }
}

/// The contents of the member named `name`, if the archive has it.
pub fn read_named(bytes: &[u8], members: &[Member], name: &str) -> Option<Vec<u8>> {
    members
        .iter()
        .find(|m| m.name == name)
        .and_then(|m| read(bytes, m))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A ZIP archive of `files` (name, contents, deflated or stored), as `zip` writes them.
    pub(crate) fn make_zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflate) in files {
            let packed = if *deflate {
                miniz_oxide::deflate::compress_to_vec(data, 6)
            } else {
                data.to_vec()
            };
            let crc = crc32(data);
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend(b"PK\x03\x04");
            out.extend(20u16.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(method.to_le_bytes());
            out.extend([0u8; 4]);
            out.extend(crc.to_le_bytes());
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend(&packed);
            central.extend(b"PK\x01\x02");
            central.extend(20u16.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(0u16.to_le_bytes());
            central.extend(method.to_le_bytes());
            central.extend([0u8; 4]);
            central.extend(crc.to_le_bytes());
            central.extend((packed.len() as u32).to_le_bytes());
            central.extend((data.len() as u32).to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let at = out.len() as u32;
        out.extend(&central);
        out.extend(b"PK\x05\x06");
        out.extend([0u8; 4]);
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn members_listed_and_read() {
        let zip = make_zip(&[
            ("a.txt", b"hello", false),
            ("dir/ü.xml", "<x>wörld</x>".as_bytes(), true),
        ]);
        assert!(is_zip(&zip));
        let list = members(&zip).unwrap_or_default();
        let names: Vec<&str> = list.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "dir/ü.xml"]);
        assert_eq!(list[1].size, "<x>wörld</x>".len() as u64);
        assert_eq!(
            read_named(&zip, &list, "dir/ü.xml").as_deref(),
            Some("<x>wörld</x>".as_bytes())
        );
        assert_eq!(
            read_named(&zip, &list, "a.txt").as_deref(),
            Some(&b"hello"[..])
        );
        assert_eq!(read_named(&zip, &list, "missing"), None);
        assert_ne!(list[0].crc, list[1].crc);
    }

    #[test]
    fn not_archives() {
        assert!(!is_zip(b"%PDF-1.4"));
        assert_eq!(members(b""), None);
        assert_eq!(members(b"PK\x03\x04 truncated"), None);
        // A directory pointing past the end lists nothing rather than failing.
        let mut zip = make_zip(&[("a", b"x", false)]);
        let n = zip.len();
        zip[n - 6..n - 2].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(members(&zip), None);
    }
}
