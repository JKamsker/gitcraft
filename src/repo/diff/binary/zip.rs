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
/// The most bytes inflated from one archive, all members together.
pub const ARCHIVE_BUDGET: usize = 48 * 1024 * 1024;
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

/// An archive's central directory as far as it could be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub members: Vec<Member>,
    /// Whether every entry it says it has was read (`false`: it is damaged past some, or has
    /// more than gitcraft lists).
    pub complete: bool,
    /// Names given to more than one member: the first of each is the one read and compared.
    pub duplicates: Vec<String>,
}

/// The members of the archive, from its central directory; `None` when it has none that can
/// be read.
pub fn members(bytes: &[u8]) -> Option<Vec<Member>> {
    let d = directory(bytes)?;
    (!d.members.is_empty() || d.complete).then_some(d.members)
}

/// Where the end-of-central-directory record is: in the last 64 KB + 22 bytes, the last whose
/// comment ends the file (a comment may hold the signature too); else the last pointing at a
/// central directory (some files have bytes after the archive); else the last.
fn find_eocd(bytes: &[u8]) -> Option<usize> {
    let from = bytes.len().saturating_sub(65_557);
    let candidates: Vec<usize> = (from..bytes.len().saturating_sub(21))
        .rev()
        .filter(|&i| bytes[i..].starts_with(b"PK\x05\x06"))
        .collect();
    let ends_file = |&&i: &&usize| {
        u16_at(bytes, i + 20).is_some_and(|c| i + 22 + usize::from(c) == bytes.len())
    };
    let points_at_directory = |&&i: &&usize| {
        let count = u16_at(bytes, i + 10).unwrap_or(0);
        u32_at(bytes, i + 16).is_some_and(|at| {
            count == 0
                || bytes
                    .get(at as usize..)
                    .is_some_and(|b| b.starts_with(b"PK\x01\x02"))
        })
    };
    candidates
        .iter()
        .find(ends_file)
        .or_else(|| candidates.iter().find(points_at_directory))
        .or(candidates.first())
        .copied()
}

/// The archive's central directory; `None` when it has no end record.
pub fn directory(bytes: &[u8]) -> Option<Directory> {
    let eocd = find_eocd(bytes)?;
    let count = usize::from(u16_at(bytes, eocd + 10)?);
    let mut at = u32_at(bytes, eocd + 16)? as usize;
    let mut out = Vec::new();
    let mut offsets = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    let mut complete = count <= MAX_MEMBERS;
    for _ in 0..count.min(MAX_MEMBERS) {
        // What was read so far stays listed when the rest is broken.
        let Some(member) = entry(bytes, at) else {
            complete = false;
            break;
        };
        let (m, next) = member;
        // Two entries for one member: a trick to inflate the same data again and again.
        if offsets.insert(m.offset) {
            if !names.insert(m.name.clone()) && !duplicates.contains(&m.name) {
                duplicates.push(m.name.clone());
            }
            out.push(m);
        }
        at = next;
    }
    Some(Directory {
        members: out,
        complete,
        duplicates,
    })
}

/// The central directory entry at `at`, and where the next one starts.
fn entry(bytes: &[u8], at: usize) -> Option<(Member, usize)> {
    if !bytes.get(at..)?.starts_with(b"PK\x01\x02") {
        return None;
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
    let member = Member {
        name: String::from_utf8_lossy(name).into_owned(),
        size,
        compressed,
        crc,
        method,
        offset,
    };
    Some((member, at + 46 + name_len + extra_len + comment_len))
}

/// Inflates members of one archive within [`ARCHIVE_BUDGET`] all together.
pub struct Reader<'a> {
    bytes: &'a [u8],
    left: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            left: ARCHIVE_BUDGET,
        }
    }

    /// The contents of `member`, while the budget lasts.
    pub fn read(&mut self, member: &Member) -> Option<Vec<u8>> {
        if self.left == 0 {
            return None;
        }
        let data = read_limited(self.bytes, member, self.left.min(MAX_INFLATED))?;
        self.left = self.left.saturating_sub(data.len().max(1));
        Some(data)
    }

    /// The contents of the (first) member named `name`, if the archive has it.
    pub fn read_named(&mut self, members: &[Member], name: &str) -> Option<Vec<u8>> {
        let m = members.iter().find(|m| m.name == name)?;
        self.read(m)
    }
}

/// The contents of `member` (stored or deflated), when at most [`MAX_INFLATED`] bytes.
#[cfg(test)]
pub fn read(bytes: &[u8], member: &Member) -> Option<Vec<u8>> {
    read_limited(bytes, member, MAX_INFLATED)
}

/// The contents of `member`, when at most `limit` bytes; `None` when larger or unreadable.
fn read_limited(bytes: &[u8], member: &Member, limit: usize) -> Option<Vec<u8>> {
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
        // Past the limit it isn't read at all: a cut member would pass for the whole.
        0 => (data.len() <= limit).then(|| data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit).ok(),
        _ => None,
    }
}

/// The contents of the member named `name`, if the archive has it.
#[cfg(test)]
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
        // Many entries for one member are listed once.
        let one = make_zip(&[("a", b"x", false)]);
        let at = one.windows(4).position(|w| w == b"PK\x01\x02").unwrap_or(0);
        let entry = one[at..one.len() - 22].to_vec();
        let mut many = one[..at].to_vec();
        for _ in 0..3 {
            many.extend(&entry);
        }
        many.extend(b"PK\x05\x06");
        many.extend([0u8; 4]);
        many.extend(3u16.to_le_bytes());
        many.extend(3u16.to_le_bytes());
        many.extend(((entry.len() * 3) as u32).to_le_bytes());
        many.extend((at as u32).to_le_bytes());
        many.extend(0u16.to_le_bytes());
        assert_eq!(members(&many).map(|m| m.len()), Some(1));
        // The budget runs out across members.
        let mut r = Reader {
            bytes: &one,
            left: 1,
        };
        let list = members(&one).unwrap_or_default();
        assert!(r.read(&list[0]).is_some());
        assert!(r.read(&list[0]).is_none());
        // A directory pointing past the end lists nothing rather than failing.
        let mut zip = make_zip(&[("a", b"x", false)]);
        let n = zip.len();
        zip[n - 6..n - 2].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(members(&zip), None);
    }

    /// `zip` with the central directory entry of `name` given compression `method`.
    pub(crate) fn with_method(mut zip: Vec<u8>, name: &str, method: u16) -> Vec<u8> {
        let mut at = 0;
        while let Some(i) = zip[at..].windows(4).position(|w| w == b"PK\x01\x02") {
            let e = at + i;
            let len = usize::from(u16_at(&zip, e + 28).unwrap_or(0));
            if zip.get(e + 46..e + 46 + len) == Some(name.as_bytes()) {
                zip[e + 10..e + 12].copy_from_slice(&method.to_le_bytes());
            }
            at = e + 4;
        }
        zip
    }

    /// A signature in the archive's comment isn't taken for its end record.
    #[test]
    fn end_record_in_the_comment() -> Result<(), String> {
        let mut zip = make_zip(&[("a.txt", b"hello", false)]);
        let mut fake = b"PK\x05\x06".to_vec();
        fake.extend([0u8; 16]);
        fake.extend(99u16.to_le_bytes());
        let n = zip.len();
        zip[n - 2..].copy_from_slice(&(fake.len() as u16).to_le_bytes());
        zip.extend(&fake);
        let d = directory(&zip).ok_or("no directory")?;
        if d.members.len() != 1 || !d.complete {
            return Err(format!("{d:?}"));
        }
        Ok(())
    }

    /// A directory cut short, more entries said than there are, is incomplete; names given
    /// twice are told; a stored member past the limit isn't read cut.
    #[test]
    fn damaged_directories_and_cut_members() -> Result<(), String> {
        let mut zip = make_zip(&[("a", b"x", false), ("b", b"y", false)]);
        let n = zip.len();
        zip[n - 12..n - 10].copy_from_slice(&3u16.to_le_bytes());
        let d = directory(&zip).ok_or("no directory")?;
        if d.complete || d.members.len() != 2 {
            return Err(format!("not incomplete: {d:?}"));
        }
        let twice = make_zip(&[
            ("a", b"one", false),
            ("a", b"two!", false),
            ("b", b"", false),
        ]);
        let d = directory(&twice).ok_or("no directory")?;
        if d.duplicates != ["a"] || !d.complete {
            return Err(format!("duplicates: {d:?}"));
        }
        let mut r = Reader::new(&twice);
        if r.read_named(&d.members, "a").as_deref() != Some(&b"one"[..]) {
            return Err("not the first".into());
        }
        let stored = make_zip(&[("big", &[b'x'; 100], false)]);
        let list = members(&stored).unwrap_or_default();
        let m = list.first().ok_or("no member")?;
        if read_limited(&stored, m, 99).is_some() || read_limited(&stored, m, 100).is_none() {
            return Err("a stored member past the limit was read".into());
        }
        Ok(())
    }
}
