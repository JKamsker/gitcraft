//! Raw tree/blob export. Only the container extracts this archive; the host never checks
//! out source, applies attributes/filters, runs an archiver, or executes repository code.
use crate::{git, remote, tasks};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};
const MAX_FILES: usize = 20_000;
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_BLOB: usize = 16 * 1024 * 1024;
const GENERATED: &str = "tests/gitcraft_reproducer.rs";

pub(super) fn text(repo: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(git::run_inert_objects(
        repo,
        args,
        None,
        4 * 1024 * 1024,
        &[0],
    )?)
    .map_err(|e| e.to_string())
}
pub(super) fn resolve(repo: &Path, rev: &str) -> Result<String, String> {
    text(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", rev.trim()),
        ],
    )
    .map(|s| s.trim().to_owned())
}
pub(super) fn load<T: serde::de::DeserializeOwned>(
    repo: &Path,
    namespace: &str,
    key: &str,
) -> Result<Option<T>, String> {
    let reference = super::super::store::reference(namespace, key);
    let oid = git::run_inert_objects(
        repo,
        &["rev-parse", "--verify", "--quiet", &reference],
        None,
        256,
        &[0, 1],
    )?;
    if oid.is_empty() {
        return Ok(None);
    }
    let oid = String::from_utf8(oid).map_err(|e| e.to_string())?;
    let bytes = git::run_inert_objects(
        repo,
        &["cat-file", "blob", oid.trim()],
        None,
        4 * 1024 * 1024,
        &[0],
    )?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("Invalid preserved reproducer evidence: {e}"))
}
pub(super) fn save<T: Serialize>(
    repo: &Path,
    namespace: &str,
    key: &str,
    value: &T,
) -> Result<(), String> {
    let reference = super::super::store::reference(namespace, key);
    let old = git::run_inert_objects(
        repo,
        &["rev-parse", "--verify", "--quiet", &reference],
        None,
        256,
        &[0, 1],
    )?;
    let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let oid = git::run_inert_objects(
        repo,
        &["hash-object", "-w", "--stdin"],
        Some(&json),
        256,
        &[0],
    )?;
    let (oid, old) = (
        String::from_utf8(oid).map_err(|e| e.to_string())?,
        String::from_utf8(old).map_err(|e| e.to_string())?,
    );
    text(repo, &["update-ref", &reference, oid.trim(), old.trim()]).map(|_| ())
}
trait Objects {
    fn read(&mut self, repo: &Path, args: &[&str], limit: usize) -> Result<Vec<u8>, String>;
}
struct GitObjects;
impl Objects for GitObjects {
    fn read(&mut self, repo: &Path, args: &[&str], limit: usize) -> Result<Vec<u8>, String> {
        git::run_inert_objects(repo, args, None, limit, &[0])
    }
}
struct Entry {
    mode: u32,
    data: Vec<u8>,
}
struct Export {
    entries: BTreeMap<String, Entry>,
    bytes: usize,
    started: Instant,
}
impl Export {
    fn check(&self) -> Result<(), String> {
        if tasks::cancellation().is_cancelled() {
            return Err("Source export cancelled".into());
        }
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err("Raw source export exceeded its 60s preparation budget".into());
        }
        if self.entries.len() >= MAX_FILES || self.bytes > MAX_BYTES {
            return Err("Sandbox source exceeds 20,000 files or 128MiB".into());
        }
        Ok(())
    }
    fn walk(
        &mut self,
        objects: &mut impl Objects,
        repo: &Path,
        rev: &str,
        prefix: &str,
        depth: usize,
    ) -> Result<(), String> {
        self.check()?;
        if depth > 8 {
            return Err("Submodule nesting exceeds the eight-level source-export limit".into());
        }
        let listing = objects.read(
            repo,
            &["ls-tree", "--full-tree", "-r", "-z", "-l", rev],
            4 * 1024 * 1024,
        )?;
        for record in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            self.check()?;
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or("Malformed raw tree record")?;
            let header =
                std::str::from_utf8(&record[..tab]).map_err(|_| "Invalid raw tree metadata")?;
            let fields: Vec<_> = header.split_whitespace().collect();
            let [mode, kind, oid, size] = fields.as_slice() else {
                return Err("Malformed raw tree metadata".into());
            };
            if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Raw tree contains an invalid object ID".into());
            }
            let relative = std::str::from_utf8(&record[tab + 1..])
                .map_err(|_| "Sandbox source paths must be UTF-8")?;
            safe_path(relative)?;
            let path = format!("{prefix}{relative}");
            safe_path(&path)?;
            if *mode == "160000" && *kind == "commit" {
                let child = remote::join(repo, relative);
                self.walk(objects, &child, oid, &format!("{path}/"), depth+1).map_err(|e| format!("Submodule {path} at {oid}: {e}. Its pinned objects must already be present locally; gitcraft never clones or checks it out."))?;
                continue;
            }
            let mode = match (*mode, *kind) {
                ("100644", "blob") => 0o644,
                ("100755", "blob") => 0o755,
                ("120000", "blob") => 0o120000,
                _ => return Err("Unsupported raw tree object mode".into()),
            };
            let size = size.parse::<usize>().map_err(|_| "Invalid raw blob size")?;
            if size > MAX_BLOB || self.bytes.saturating_add(size) > MAX_BYTES {
                return Err("Sandbox source exceeds 16MiB per blob or 128MiB total".into());
            }
            let data = objects.read(repo, &["cat-file", "blob", oid], size.saturating_add(1))?;
            if data.len() != size {
                return Err("Raw blob size changed or output was truncated".into());
            }
            self.bytes += data.len();
            if self.entries.insert(path, Entry { mode, data }).is_some() {
                return Err("Duplicate raw source path".into());
            }
        }
        Ok(())
    }
}
fn safe_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path.contains(['\0', '\\', ':'])
        || path
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | "..") || p.eq_ignore_ascii_case(".git"))
    {
        Err(format!(
            "Unsafe or unsupported sandbox source path: {path:?}"
        ))
    } else {
        Ok(())
    }
}
fn safe_link(path: &str, data: &[u8]) -> Result<String, String> {
    let target = std::str::from_utf8(data).map_err(|_| "Sandbox symlink targets must be UTF-8")?;
    if target.is_empty() || target.starts_with('/') || target.contains(['\0', '\\', ':']) {
        return Err(format!("Unsafe symlink {path}"));
    }
    let mut depth = path.split('/').count().saturating_sub(1);
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("Symlink {path} escapes the source root"))?;
            }
            p if p.eq_ignore_ascii_case(".git") => {
                return Err("Symlink points into Git metadata".into());
            }
            _ => depth += 1,
        }
    }
    Ok(target.into())
}
fn octal(field: &mut [u8], value: usize) -> Result<(), String> {
    let text = format!("{:0width$o}\0", value, width = field.len() - 1);
    if text.len() != field.len() {
        return Err("Archive field overflow".into());
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}
fn header(
    out: &mut Vec<u8>,
    path: &str,
    mode: u32,
    kind: u8,
    link: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if path.len() > 100 || link.len() > 100 {
        return Err("Archive header requires a PAX path record".into());
    }
    let mut block = [0u8; 512];
    block[..path.len()].copy_from_slice(path.as_bytes());
    octal(&mut block[100..108], mode as usize)?;
    octal(&mut block[108..116], 0)?;
    octal(&mut block[116..124], 0)?;
    octal(&mut block[124..136], bytes.len())?;
    octal(&mut block[136..148], 0)?;
    block[148..156].fill(b' ');
    block[156] = kind;
    block[157..157 + link.len()].copy_from_slice(link.as_bytes());
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    let checksum = block.iter().map(|b| *b as usize).sum::<usize>();
    let sum = format!("{checksum:06o}\0 ");
    block[148..156].copy_from_slice(sum.as_bytes());
    out.extend_from_slice(&block);
    out.extend_from_slice(bytes);
    out.resize(out.len().next_multiple_of(512), 0);
    Ok(())
}
fn pax_record(key: &str, value: &str) -> String {
    let body = format!(" {key}={value}\n");
    let mut length = body.len() + 1;
    loop {
        let next = body.len() + length.to_string().len();
        if next == length {
            break;
        }
        length = next;
    }
    format!("{length}{body}")
}
fn pack(mut entries: BTreeMap<String, Entry>, source: &str) -> Result<Vec<u8>, String> {
    if source.is_empty() || source.len() > 64 * 1024 {
        return Err("The generated integration test must be 1–65536 bytes".into());
    }
    entries.insert(
        GENERATED.into(),
        Entry {
            mode: 0o644,
            data: source.as_bytes().to_vec(),
        },
    );
    let links: Vec<_> = entries
        .iter()
        .filter(|(_, e)| e.mode == 0o120000)
        .map(|(p, _)| p.as_str())
        .collect();
    for path in entries.keys() {
        safe_path(path)?;
        if links.iter().any(|link| {
            path.strip_prefix(*link)
                .is_some_and(|tail| tail.starts_with('/'))
        }) {
            return Err(format!("Archive member {path} traverses a symlink"));
        }
    }
    let mut out = Vec::new();
    for (index, (path, entry)) in entries.into_iter().enumerate() {
        let link = if entry.mode == 0o120000 {
            safe_link(&path, &entry.data)?
        } else {
            String::new()
        };
        let mut pax = String::new();
        if path.len() > 100 {
            pax.push_str(&pax_record("path", &path));
        }
        if link.len() > 100 {
            pax.push_str(&pax_record("linkpath", &link));
        }
        if !pax.is_empty() {
            header(
                &mut out,
                &format!("PaxHeaders/{index}"),
                0o644,
                b'x',
                "",
                pax.as_bytes(),
            )?;
        }
        let name = if path.len() > 100 { "PaxFile" } else { &path };
        let link_name = if link.len() > 100 { "PaxLink" } else { &link };
        header(
            &mut out,
            name,
            if link.is_empty() { entry.mode } else { 0o777 },
            if link.is_empty() { b'0' } else { b'2' },
            link_name,
            if link.is_empty() { &entry.data } else { &[] },
        )?;
    }
    out.extend_from_slice(&[0; 1024]);
    Ok(out)
}
pub(super) fn archive(repo: &Path, rev: &str, source: &str) -> Result<Vec<u8>, String> {
    let mut export = Export {
        entries: BTreeMap::new(),
        bytes: 0,
        started: Instant::now(),
    };
    export.walk(&mut GitObjects, repo, rev, "", 0)?;
    pack(export.entries, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r2_pure_raw_export_uses_only_blob_tree_reads_and_does_not_apply_attributes()
    -> Result<(), String> {
        struct Recorded(Vec<String>);
        impl Objects for Recorded {
            fn read(&mut self, _: &Path, args: &[&str], _: usize) -> Result<Vec<u8>, String> {
                self.0.push(args.join(" "));
                let oid = "a".repeat(40);
                match args[0] {
                    "ls-tree" => Ok(format!("100644 blob {oid} 3\t.gitattributes\0").into_bytes()),
                    "cat-file" => Ok(b"raw".to_vec()),
                    _ => Err("host helper requested".into()),
                }
            }
        }
        let mut recorded = Recorded(Vec::new());
        let mut export = Export {
            entries: BTreeMap::new(),
            bytes: 0,
            started: Instant::now(),
        };
        export.walk(
            &mut recorded,
            Path::new("recording-only"),
            &"b".repeat(40),
            "",
            0,
        )?;
        assert_eq!(export.entries[".gitattributes"].data, b"raw");
        let tar = pack(export.entries, "#[test] fn same() {}")?;
        assert!(tar.windows(3).any(|w| w == b"raw"));
        assert!(
            tar.windows(GENERATED.len())
                .any(|w| w == GENERATED.as_bytes())
        );
        assert_eq!(recorded.0.len(), 2);
        assert!(recorded.0[0].starts_with("ls-tree --full-tree -r -z -l"));
        assert!(recorded.0[1].starts_with("cat-file blob"));
        Ok(())
    }
    #[test]
    fn r2_pure_archive_rejects_traversal_links_and_preserves_long_paths_modes_and_identical_test()
    -> Result<(), String> {
        for path in [
            "../outside",
            "/etc/x",
            ".git/hooks/post-checkout",
            "src/../../x",
            "C:/x",
        ] {
            assert!(safe_path(path).is_err());
        }
        assert!(safe_link("src/link", b"../../outside").is_err());
        assert_eq!(safe_link("src/link", b"../safe")?, "../safe");
        let mut entries = BTreeMap::new();
        entries.insert(
            "tests".into(),
            Entry {
                mode: 0o120000,
                data: b"src".to_vec(),
            },
        );
        assert!(pack(entries, "test").is_err());
        let mut entries = BTreeMap::new();
        let long = format!("src/{}/file.rs", "a".repeat(200));
        entries.insert(
            long.clone(),
            Entry {
                mode: 0o755,
                data: b"raw executable".to_vec(),
            },
        );
        let archive = pack(entries, "identical test")?;
        assert!(archive.windows(long.len()).any(|w| w == long.as_bytes()));
        assert!(archive.windows(7).any(|w| w == b"0000755"));
        assert!(archive.windows(14).any(|w| w == b"identical test"));
        assert!(archive.ends_with(&[0; 1024]));
        Ok(())
    }
    #[test]
    #[ignore = "R2 runtime hold: hook/filter sentinel fixture authored, not executed"]
    fn r2_runtime_raw_archive_does_not_execute_checkout_or_filter_sentinels() -> Result<(), String>
    {
        let r = git::stack::test_repo::TestRepo::new("reproducer-inert-export")?;
        r.commit("source.txt", "raw source\n", "source")?;
        r.commit(
            ".gitattributes",
            "source.txt filter=sentinel\n",
            "attributes",
        )?;
        let marker = r.path().join("HOST_SENTINEL");
        let script = format!(
            "printf ran >> {}; cat",
            remote::sh_quote(marker.to_str().ok_or("path")?)
        );
        r.git(&["config", "filter.sentinel.smudge", &script])?;
        r.git(&["config", "filter.sentinel.process", &script])?;
        let hooks = r.path().join("hooks");
        std::fs::create_dir_all(&hooks).map_err(|e| e.to_string())?;
        std::fs::write(
            hooks.join("post-checkout"),
            format!("#!/bin/sh\n{script}\n"),
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks.join("post-checkout"),
                std::fs::Permissions::from_mode(0o755),
            )
            .map_err(|e| e.to_string())?;
        }
        r.git(&["config", "core.hooksPath", hooks.to_str().ok_or("path")?])?;
        std::fs::write(
            hooks.join("reference-transaction"),
            format!("#!/bin/sh\n{script}\n"),
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks.join("reference-transaction"),
                std::fs::Permissions::from_mode(0o755),
            )
            .map_err(|e| e.to_string())?;
        }
        let bytes = archive(r.path(), &r.tip("HEAD")?, "#[test] fn repro() {}")?;
        assert!(bytes.windows(10).any(|w| w == b"raw source"));
        save(r.path(), "recording-sentinel", "case", &"inert metadata")?;
        assert!(!marker.exists());
        Ok(())
    }
}
