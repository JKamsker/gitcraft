//! Resolve raw authors once, after paging. Git loads one effective mailmap before processing
//! stdin, so a checkout/config edit between history pages cannot split one author. Keep Git's
//! parser and precedence: current worktree .mailmap, blob, then configured augmenting file.
use super::{Record, is_hash, process};
use std::{collections::BTreeMap, path::Path};

const MAX_INPUT: usize = 16 * 1024 * 1024;

pub(super) fn resolve(
    repo: &Path,
    records: &mut [Record],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    resolve_inner(repo, records, active).map_err(|e| {
        crate::i18n::message(
            "backend-contributor-mailmap-unavailable-e-860d54",
            &[("detail", (e).to_string().into())],
        )
    })
}

fn resolve_inner(
    repo: &Path,
    records: &mut [Record],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }
    let run = |args: &[&str], input: Option<&[u8]>| process::run(repo, args, input, active);
    let config = |key, path| process::config(repo, key, path, active);
    let file = config("mailmap.file", true)?.unwrap_or_default();
    let configured_blob = config("mailmap.blob", false)?;
    let bare = run(&["rev-parse", "--is-bare-repository"], None)? == b"true\n";
    let root = run(
        &[
            "rev-parse",
            if bare {
                "--absolute-git-dir"
            } else {
                "--show-toplevel"
            },
        ],
        None,
    )?;
    let root = std::str::from_utf8(&root)
        .map_err(|_| crate::i18n::text("backend-mailmap-repository-path-is-not-utf-8-74b4cc"))?
        .trim_end_matches(['\r', '\n']);
    // Pin a configured revision expression to its immutable blob. A bare repository also
    // defaults to HEAD:.mailmap, but an absent default .mailmap is normal, not an error.
    let blob = match configured_blob {
        Some(spec) if !spec.is_empty() => {
            let oid = run(
                &[
                    "--no-lazy-fetch",
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &spec,
                ],
                None,
            )?;
            let oid = std::str::from_utf8(&oid)
                .map_err(|_| crate::i18n::text("backend-invalid-mailmap-blob-id-ef142c"))?
                .trim()
                .to_owned();
            if !is_hash(oid.as_bytes()) {
                return Err(crate::i18n::text("backend-invalid-mailmap-blob-id-ef142c").into());
            }
            Some(oid)
        }
        None if bare => {
            let head = run(&["rev-parse", "--revs-only", "HEAD"], None)?;
            if head.is_empty() {
                None
            } else {
                let head = std::str::from_utf8(&head)
                    .map_err(|_| crate::i18n::text("backend-invalid-head-e1661b"))?
                    .trim();
                if !is_hash(head.as_bytes()) {
                    return Err(crate::i18n::text("backend-invalid-head-e1661b").into());
                }
                let entry = run(
                    &["--no-lazy-fetch", "ls-tree", "-z", head, "--", ".mailmap"],
                    None,
                )?;
                if entry.is_empty() {
                    None
                } else {
                    let metadata = entry
                        .split(|b| *b == b'\t')
                        .next()
                        .ok_or(crate::i18n::text(
                            "backend-invalid-mailmap-tree-entry-f1ba1a",
                        ))?;
                    let oid = metadata
                        .split(|b| *b == b' ')
                        .next_back()
                        .ok_or(crate::i18n::text("backend-invalid-mailmap-blob-id-ef142c"))?;
                    if !is_hash(oid) {
                        return Err(
                            crate::i18n::text("backend-invalid-mailmap-blob-id-ef142c").into()
                        );
                    }
                    Some(
                        std::str::from_utf8(oid)
                            .map_err(|_| {
                                crate::i18n::text("backend-invalid-mailmap-blob-id-ef142c")
                            })?
                            .to_owned(),
                    )
                }
            }
        }
        _ => None,
    };
    if let Some(oid) = &blob {
        // Reading the actual blob catches promised-but-unavailable content. --no-lazy-fetch
        // is essential: resolving an existing tree entry alone does not prove it is local.
        run(&["--no-lazy-fetch", "cat-file", "blob", oid], None)?;
    }
    let fingerprint = || {
        if file.is_empty() {
            Ok(None)
        } else {
            run(
                &["-C", root, "hash-object", "--no-filters", "--", &file],
                None,
            )
            .map(Some)
        }
    };
    let before = fingerprint()?;
    let identities: BTreeMap<_, _> = records
        .iter()
        .map(|r| ((r.name.clone(), r.email.clone()), ()))
        .collect();
    let mut input = Vec::new();
    for (name, email) in identities.keys() {
        if name.contains(['\n', '\r', '\0']) || email.contains(['\n', '\r', '\0']) {
            return Err(crate::i18n::text(
                "backend-an-author-identity-contains-unsupported-line-breaks-68ed6b",
            )
            .into());
        }
        input.extend_from_slice(format!("{name} <{email}>\n").as_bytes());
        if input.len() > MAX_INPUT {
            return Err(crate::i18n::text(
                "backend-author-identities-exceeded-the-16-mib-mapping-limit-bfd026",
            )
            .into());
        }
    }
    let file_option = format!("mailmap.file={file}");
    let blob_option = format!("mailmap.blob={}", blob.as_deref().unwrap_or_default());
    let output = run(
        &[
            "-C",
            root,
            "--no-lazy-fetch",
            "-c",
            &file_option,
            "-c",
            &blob_option,
            "check-mailmap",
            "--stdin",
        ],
        Some(&input),
    )?;
    if fingerprint()? != before {
        return Err(crate::i18n::text(
            "backend-the-configured-mailmap-file-changed-while-resolving-authors-refre-8ccfd4",
        )
        .into());
    }
    let output = std::str::from_utf8(&output)
        .map_err(|_| crate::i18n::text("backend-git-returned-a-non-utf-8-author-mapping-99e699"))?;
    let lines: Vec<_> = output.lines().collect();
    if lines.len() != identities.len() {
        return Err(
            crate::i18n::text("backend-git-returned-an-incomplete-author-mapping-c5c730").into(),
        );
    }
    let mut mapped = BTreeMap::new();
    for ((identity, _), line) in identities.into_iter().zip(lines) {
        let line = line
            .strip_suffix('>')
            .ok_or(crate::i18n::text("backend-invalid-mapped-identity-2642a6"))?;
        let (name, email) = line
            .rsplit_once('<')
            .ok_or(crate::i18n::text("backend-invalid-mapped-identity-2642a6"))?;
        mapped.insert(identity, (name.trim_end().to_owned(), email.to_owned()));
    }
    apply_mapping(records, &mapped, active, 64 * 1024 * 1024)
}

fn apply_mapping(
    records: &mut [Record],
    mapped: &BTreeMap<(String, String), (String, String)>,
    active: &dyn Fn() -> bool,
    max_bytes: usize,
) -> Result<(), String> {
    let mut remaining = max_bytes;
    for record in records {
        if !active() {
            return Err("Analysis cancelled".into());
        }
        let (name, email) = mapped
            .get(&(record.name.clone(), record.email.clone()))
            .ok_or(crate::i18n::text("backend-missing-mapped-author-1b52c7"))?;
        // A tiny raw identity may map to a long canonical name. Bound the
        // cumulative expansion before cloning it into every commit record.
        let cost = name
            .len()
            .checked_add(email.len())
            .ok_or(crate::i18n::text(
                "backend-mapped-author-size-overflow-2a1087",
            ))?;
        remaining = remaining.checked_sub(cost).ok_or_else(|| {
            crate::i18n::message(
                "backend-expanded-mapped-author-metadata-exceeds-the-max-bytes-byte-alloca-5875de",
                &[("max_bytes", (max_bytes).to_string().into())],
            )
        })?;
        record.name = name.clone();
        record.email = email.clone();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_identity_expansion_refuses_before_cloning_every_record() -> Result<(), String> {
        let day = super::super::Day::parse("2026-11-08").ok_or("date")?;
        let mut records: Vec<_> = (0..512)
            .map(|_| Record {
                hash: String::new(),
                name: "Raw".into(),
                email: "old@test".into(),
                author: day,
                committer: day,
                subject: String::new(),
                paths: Default::default(),
            })
            .collect();
        let mapped = BTreeMap::from([(
            ("Raw".into(), "old@test".into()),
            ("N".repeat(16_384), "canonical@test".into()),
        )]);
        assert!(
            apply_mapping(&mut records, &mapped, &|| true, 32_768)
                .is_err_and(|e| e.contains("allocation budget"))
        );
        assert_eq!(records.iter().filter(|r| r.name.len() > 100).count(), 1);
        assert!(
            apply_mapping(&mut records[1..], &mapped, &|| false, 32_768)
                .is_err_and(|e| e.contains("cancelled"))
        );
        Ok(())
    }
}
