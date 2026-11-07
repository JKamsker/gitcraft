//! Read selected text from an archive, never unpack or execute package contents.
use std::collections::BTreeMap;
use std::io::Read;

const MAX_EXPANDED: u64 = 32 << 20;
const MAX_TEXT: usize = 512 << 10;

pub(super) fn texts(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_EXPANDED + 1)
        .read_to_end(&mut tar)
        .map_err(|_| "Invalid package archive")?;
    if tar.len() as u64 > MAX_EXPANDED {
        return Err("Expanded package exceeds 32 MiB".into());
    }
    let mut files = BTreeMap::new();
    let mut at = 0usize;
    let mut long = None;
    let mut retained = 0usize;
    while let Some(header) = tar.get(at..at.saturating_add(512)) {
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let field = |a, b| -> Result<&str, String> {
            std::str::from_utf8(&header[a..b])
                .map(|s| s.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace()))
                .map_err(|_| "Invalid archive header".into())
        };
        let expected =
            u64::from_str_radix(field(148, 156)?, 8).map_err(|_| "Invalid archive checksum")?;
        let checksum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u64::from(*b)
                }
            })
            .sum();
        if expected != checksum {
            return Err("Package archive checksum mismatch".into());
        }
        let size =
            usize::from_str_radix(field(124, 136)?, 8).map_err(|_| "Invalid archive size")?;
        let data = at.checked_add(512).ok_or("Archive overflow")?;
        let end = data.checked_add(size).ok_or("Archive overflow")?;
        let body = tar.get(data..end).ok_or("Truncated package archive")?;
        let name = long.take().unwrap_or(field(0, 100)?.to_owned());
        match header[156] {
            b'L' if size <= 4096 => {
                long = Some(
                    std::str::from_utf8(body)
                        .map_err(|_| "Invalid long path")?
                        .trim_end_matches('\0')
                        .to_owned(),
                )
            }
            b'x' if size <= 8192 => {
                long = std::str::from_utf8(body).ok().and_then(|s| {
                    s.lines()
                        .find_map(|l| l.split_once(" path=").map(|(_, v)| v.to_owned()))
                });
            }
            b'0' | 0 if size <= MAX_TEXT => {
                if let Some((_, path)) = name.split_once('/') {
                    if path.split('/').any(|p| matches!(p, "." | ".." | "")) || path.contains('\\')
                    {
                        return Err("Unsafe package member path".into());
                    }
                    let leaf = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
                    let wanted = matches!(path, "Cargo.toml" | "package.json")
                        || leaf.starts_with("changelog")
                        || leaf.starts_with("changes")
                        || leaf.starts_with("history")
                        || leaf.starts_with("release-notes");
                    if wanted {
                        retained += size;
                        if retained > 2 << 20 || files.len() >= 16 {
                            return Err("Too much package metadata".into());
                        }
                        let text = std::str::from_utf8(body)
                            .map_err(|_| "Package metadata is not UTF-8")?;
                        if files.insert(path.to_owned(), text.to_owned()).is_some() {
                            return Err("Duplicate package metadata member".into());
                        }
                    }
                }
            }
            _ => {}
        }
        at = data
            .checked_add(
                size.div_ceil(512)
                    .checked_mul(512)
                    .ok_or("Archive overflow")?,
            )
            .ok_or("Archive overflow")?;
    }
    Ok(files)
}
