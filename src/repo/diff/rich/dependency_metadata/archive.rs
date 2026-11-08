//! Read selected text from an archive, never unpack or execute package contents.
use std::collections::BTreeMap;
use std::io::Read;

const MAX_EXPANDED: u64 = 32 << 20;
const MAX_TEXT: usize = 512 << 10;

#[derive(Default)]
pub(super) struct Texts {
    pub files: BTreeMap<String, String>,
    pub skipped: BTreeMap<String, String>,
}
pub(super) fn texts(bytes: &[u8]) -> Result<Texts, String> {
    texts_with(bytes, None)
}
pub(super) fn texts_with(bytes: &[u8], extra: Option<&str>) -> Result<Texts, String> {
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_EXPANDED + 1)
        .read_to_end(&mut tar)
        .map_err(|_| crate::i18n::text("repo-core-2063"))?;
    if tar.len() as u64 > MAX_EXPANDED {
        return Err(crate::i18n::text("repo-core-2064").into());
    }
    let mut result = Texts::default();
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
                .map_err(|_| crate::i18n::text("repo-core-2065").into())
        };
        let expected = u64::from_str_radix(field(148, 156)?, 8)
            .map_err(|_| crate::i18n::text("repo-core-2066"))?;
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
            return Err(crate::i18n::text("repo-core-2067").into());
        }
        let size = usize::from_str_radix(field(124, 136)?, 8)
            .map_err(|_| crate::i18n::text("repo-core-2068"))?;
        let data = at
            .checked_add(512)
            .ok_or(crate::i18n::text("repo-core-2069"))?;
        let end = data
            .checked_add(size)
            .ok_or(crate::i18n::text("repo-core-2069"))?;
        let body = tar
            .get(data..end)
            .ok_or(crate::i18n::text("repo-core-2070"))?;
        let name = long.take().unwrap_or(field(0, 100)?.to_owned());
        match header[156] {
            b'L' if size <= 4096 => {
                long = Some(
                    std::str::from_utf8(body)
                        .map_err(|_| crate::i18n::text("repo-core-2071"))?
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
            b'0' | 0 => {
                if let Some((_, path)) = name.split_once('/') {
                    if path.split('/').any(|p| matches!(p, "." | ".." | "")) || path.contains('\\')
                    {
                        return Err(crate::i18n::text("repo-core-2073").into());
                    }
                    let leaf = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
                    let wanted = Some(path) == extra
                        || matches!(path, "Cargo.toml" | "package.json")
                        || leaf.starts_with("changelog")
                        || leaf.starts_with("changes")
                        || leaf.starts_with("history")
                        || leaf.starts_with("release-notes");
                    if wanted {
                        if result.files.contains_key(path) || result.skipped.contains_key(path) {
                            return Err(crate::i18n::text("repo-core-2079").into());
                        }
                        let manifest = matches!(path, "Cargo.toml" | "package.json");
                        let reason = if size > MAX_TEXT {
                            Some(crate::i18n::message(
                                "repo-core-2080",
                                &[("size", (size).into())],
                            ))
                        } else if !manifest
                            && (retained + size > 2 << 20 || result.files.len() >= 16)
                        {
                            Some(crate::i18n::text("repo-core-2081").into())
                        } else {
                            None
                        };
                        match reason {
                            Some(reason) => {
                                if result.skipped.len() < 64 {
                                    result.skipped.insert(path.to_owned(), reason);
                                }
                            }
                            None => match std::str::from_utf8(body) {
                                Ok(text) => {
                                    retained += size;
                                    result.files.insert(path.to_owned(), text.to_owned());
                                }
                                Err(_) => {
                                    result.skipped.insert(
                                        path.to_owned(),
                                        crate::i18n::text("repo-core-2082").into(),
                                    );
                                }
                            },
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
                    .ok_or(crate::i18n::text("repo-core-2069"))?,
            )
            .ok_or(crate::i18n::text("repo-core-2069"))?;
    }
    Ok(result)
}
