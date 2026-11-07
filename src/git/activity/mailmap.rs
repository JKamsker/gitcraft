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
    resolve_inner(repo, records, active)
        .map_err(|e| format!("Contributor mailmap unavailable: {e}"))
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
    let config =
        |key, path| super::super::command::activity_mailmap_config(repo, key, path, active);
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
        .map_err(|_| "Mailmap repository path is not UTF-8")?
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
                .map_err(|_| "Invalid mailmap blob ID")?
                .trim()
                .to_owned();
            if !is_hash(oid.as_bytes()) {
                return Err("Invalid mailmap blob ID".into());
            }
            Some(oid)
        }
        None if bare => {
            let head = run(&["rev-parse", "--revs-only", "HEAD"], None)?;
            if head.is_empty() {
                None
            } else {
                let head = std::str::from_utf8(&head)
                    .map_err(|_| "Invalid HEAD")?
                    .trim();
                if !is_hash(head.as_bytes()) {
                    return Err("Invalid HEAD".into());
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
                        .ok_or("Invalid mailmap tree entry")?;
                    let oid = metadata
                        .split(|b| *b == b' ')
                        .next_back()
                        .ok_or("Invalid mailmap blob ID")?;
                    if !is_hash(oid) {
                        return Err("Invalid mailmap blob ID".into());
                    }
                    Some(
                        std::str::from_utf8(oid)
                            .map_err(|_| "Invalid mailmap blob ID")?
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
            return Err("An author identity contains unsupported line breaks".into());
        }
        input.extend_from_slice(format!("{name} <{email}>\n").as_bytes());
        if input.len() > MAX_INPUT {
            return Err("Author identities exceeded the 16 MiB mapping limit".into());
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
        return Err(
            "The configured mailmap file changed while resolving authors; refresh activity".into(),
        );
    }
    let output =
        std::str::from_utf8(&output).map_err(|_| "Git returned a non-UTF-8 author mapping")?;
    let lines: Vec<_> = output.lines().collect();
    if lines.len() != identities.len() {
        return Err("Git returned an incomplete author mapping".into());
    }
    let mut mapped = BTreeMap::new();
    for ((identity, _), line) in identities.into_iter().zip(lines) {
        let line = line.strip_suffix('>').ok_or("Invalid mapped identity")?;
        let (name, email) = line.rsplit_once('<').ok_or("Invalid mapped identity")?;
        mapped.insert(identity, (name.trim_end().to_owned(), email.to_owned()));
    }
    for record in records {
        if !active() {
            return Err("Analysis cancelled".into());
        }
        let (name, email) = mapped
            .get(&(record.name.clone(), record.email.clone()))
            .ok_or("Missing mapped author")?;
        record.name = name.clone();
        record.email = email.clone();
    }
    Ok(())
}
