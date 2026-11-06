//! The files in a package that is published already: a `.crate` or an npm tarball, both a gzip'd
//! tar.

use std::io::Read;

/// How big a download is let be.
const MAX: u64 = 200 * 1024 * 1024;

/// The paths of the files in the gzip'd tar `bytes`, without their first folder (`app-1.2.3/`,
/// `package/`).
pub(crate) fn list(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX)
        .read_to_end(&mut tar)
        .map_err(|e| format!("not a gzip'd archive: {e}"))?;
    let mut out = Vec::new();
    let mut at = 0;
    // A long name (GNU's `L` entry, or a pax `path`) for the entry that follows.
    let mut long: Option<String> = None;
    while at + 512 <= tar.len() {
        let header = &tar[at..at + 512];
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let field = |from: usize, len: usize| {
            let raw = &header[from..from + len];
            let end = raw.iter().position(|b| *b == 0).unwrap_or(len);
            String::from_utf8_lossy(&raw[..end]).into_owned()
        };
        let size = u64::from_str_radix(field(124, 12).trim(), 8)
            .map_err(|_| "a damaged archive".to_owned())? as usize;
        let kind = header[156];
        let data = at + 512;
        let next = data + size.div_ceil(512) * 512;
        if next > tar.len() + 512 {
            return Err("a damaged archive".to_owned());
        }
        let body = &tar[data..(data + size).min(tar.len())];
        match kind {
            b'L' => {
                long = Some(
                    String::from_utf8_lossy(body)
                        .trim_end_matches('\0')
                        .to_owned(),
                );
            }
            b'x' => {
                long = pax_path(body).or(long);
            }
            b'0' | 0 | b'7' => {
                let name = long.take().unwrap_or_else(|| {
                    let prefix = field(345, 155);
                    let name = field(0, 100);
                    if prefix.is_empty() {
                        name
                    } else {
                        format!("{prefix}/{name}")
                    }
                });
                if let Some((_, rest)) = name.split_once('/')
                    && !rest.is_empty()
                {
                    out.push(rest.to_owned());
                }
            }
            _ => {
                long = None;
            }
        }
        at = next;
    }
    out.sort();
    Ok(out)
}

/// The `path` of a pax header's records (`<len> path=<value>\n`).
fn pax_path(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    text.lines().find_map(|record| {
        let (_, kv) = record.split_once(' ')?;
        kv.strip_prefix("path=").map(str::to_owned)
    })
}

/// Builds a gzip'd tar of `files` under `root/`: what the tests read.
#[cfg(test)]
pub(crate) fn make(root: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut tar = Vec::new();
    for (name, data) in files {
        let path = format!("{root}/{name}");
        let mut header = [0u8; 512];
        if path.len() > 100 {
            let mut long = [0u8; 512];
            long[..13].copy_from_slice(b"././@LongLink");
            long[124..135].copy_from_slice(format!("{:011o}", path.len() + 1).as_bytes());
            long[156] = b'L';
            checksum(&mut long);
            tar.extend_from_slice(&long);
            let mut body = path.as_bytes().to_vec();
            body.push(0);
            body.resize(body.len().div_ceil(512) * 512, 0);
            tar.extend_from_slice(&body);
            header[..100].copy_from_slice(&path.as_bytes()[..100]);
        } else {
            header[..path.len()].copy_from_slice(path.as_bytes());
        }
        header[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        header[156] = b'0';
        checksum(&mut header);
        tar.extend_from_slice(&header);
        let mut body = data.to_vec();
        body.resize(body.len().div_ceil(512) * 512, 0);
        tar.extend_from_slice(&body);
    }
    tar.extend_from_slice(&[0u8; 1024]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let _ = gz.write_all(&tar);
    gz.finish().unwrap_or_default()
}

#[cfg(test)]
fn checksum(header: &mut [u8; 512]) {
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_of_a_package_are_listed_without_its_folder() {
        let long = format!("src/{}/deep ü.rs", "x".repeat(120));
        let bytes = make(
            "app-1.2.3",
            &[
                ("Cargo.toml", b"[package]"),
                ("src/main.rs", b"fn main() {}"),
                (&long, b""),
            ],
        );
        let files = list(&bytes).unwrap_or_default();
        assert_eq!(
            files,
            ["Cargo.toml".to_owned(), "src/main.rs".to_owned(), long]
        );
        assert!(list(b"not gzip").is_err());
        assert!(list(&make("package", &[])).is_ok_and(|f| f.is_empty()));
    }
}
