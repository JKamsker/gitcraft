//! Immutable previews and deliberately separate, create-only recovery operations.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub identity: Identity,
    pub blob: String,
    pub source: Option<Provenance>,
    pub bytes: Arc<Vec<u8>>,
    pub text: Arc<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Branch(String),
    File(String),
    Stash,
}
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    pub preview: Preview,
    pub target: Target,
    expected_stash: Option<String>,
}
fn blob(reader: &Reader<'_>, oid: &str) -> Result<Vec<u8>, String> {
    if !valid_oid(oid) {
        return Err("Invalid recovery blob ID".into());
    }
    let mut checked = batch::check(reader, &[oid.to_owned()])?;
    let info = checked
        .pop()
        .and_then(|item| item.info)
        .ok_or("The saved blob is no longer available locally")?;
    if info.0 != "blob" || info.1 > BLOB_LIMIT {
        return Err("The recovery source is not a blob within 2 MiB".into());
    }
    let mut objects = batch::read(reader, &[(oid.to_owned(), info.1)], "blob")?;
    let bytes = objects.pop().ok_or("Missing recovery blob")?.1;
    let actual = reader.run(&["hash-object", "--no-filters", "--stdin"], Some(&bytes))?;
    if actual.strip_suffix(b"\n") != Some(oid.as_bytes()) {
        return Err("Recovery blob failed its object identity check".into());
    }
    Ok(bytes)
}
fn source(reader: &Reader<'_>, source: &Provenance, blob: &str) -> Result<(), String> {
    if !valid_oid(&source.commit) || !valid_oid(&source.tree) || !valid_oid(blob) {
        return Err("Invalid recovery provenance".into());
    }
    let mut report = Report::default();
    let commit = read_commits(
        reader,
        &BTreeSet::from([source.commit.clone()]),
        &mut report,
    )?
    .remove(&source.commit)
    .ok_or("The source commit is no longer available locally")?;
    let tree = match source.origin {
        Origin::DroppedStash | Origin::LiveStash => {
            let parents = read_commits(
                reader,
                &commit.parents.iter().skip(1).cloned().collect(),
                &mut report,
            )?;
            if !stash_shape(&commit, &parents) {
                return Err("The source is not a validated stash object".into());
            }
            match source.layer {
                "working" => commit.tree.clone(),
                "staged" => parents
                    .get(&commit.parents[1])
                    .ok_or("Missing stash index")?
                    .tree
                    .clone(),
                "untracked" if commit.parents.len() == 3 => parents
                    .get(&commit.parents[2])
                    .ok_or("Missing stash untracked tree")?
                    .tree
                    .clone(),
                _ => return Err("Invalid stash content layer".into()),
            }
        }
        _ if source.layer == "working" => commit.tree,
        _ => return Err("Invalid commit content layer".into()),
    };
    if tree != source.tree {
        return Err("Recovery provenance tree does not match its pinned commit".into());
    }
    let record = reader.run(
        &[
            "--literal-pathspecs",
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            &tree,
            "--",
            &source.path,
        ],
        None,
    )?;
    let expected = format!("{} blob {blob}\t{}\0", source.mode, source.path);
    if record != expected.as_bytes() {
        return Err("Recovery path does not match its pinned blob".into());
    }
    Ok(())
}
pub(crate) fn inspect(
    repo: &Path,
    results: &Results,
    hit: &Hit,
    source_index: Option<usize>,
    active: &dyn Fn() -> bool,
) -> Result<Preview, String> {
    let context = crate::connections::current();
    context.run(|| {
        let reader = Reader::new(repo, active);
        if reader.identity() != Ok(results.identity.clone()) {
            return Err("Repository changed since search; search again".into());
        }
        if !results
            .report
            .hits
            .iter()
            .any(|saved| saved.blob == hit.blob && saved.size == hit.size)
        {
            return Err("This content is not a hit in the current search".into());
        }
        let selected = source_index
            .map(|index| {
                hit.provenance
                    .get(index)
                    .cloned()
                    .ok_or("Invalid source selection")
            })
            .transpose()?;
        if let Some(selected) = &selected {
            source(&reader, selected, &hit.blob)?;
        }
        let bytes = blob(&reader, &hit.blob)?;
        if bytes.len() != hit.size {
            return Err("Content size changed since search".into());
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| "Binary content cannot be previewed")?;
        if text.contains('\0') || !text.contains(&results.query) {
            return Err("The saved content no longer matches the literal search".into());
        }
        // Keep full immutable raw bytes for recovery; UI renders only bounded excerpts.
        Ok(Preview {
            identity: results.identity.clone(),
            blob: hit.blob.clone(),
            source: selected,
            text: Arc::new(text.to_owned()),
            bytes: Arc::new(bytes),
        })
    })
}
fn optional_ref(reader: &Reader<'_>, reference: &str) -> Result<Option<String>, String> {
    let out = reader.output(
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            reference,
        ],
        None,
    )?;
    if out.code == 1 && out.stderr.is_empty() {
        return Ok(None);
    }
    if out.code != 0 || !out.stderr.is_empty() {
        return Err("Cannot inspect recovery destination ref".into());
    }
    let oid = String::from_utf8(out.stdout)
        .map_err(|_| "Invalid destination object ID")?
        .trim()
        .to_owned();
    if !valid_oid(&oid) {
        return Err("Invalid destination object ID".into());
    }
    Ok(Some(oid))
}
fn file_policy(reader: &Reader<'_>, identity: &Identity, path: &str) -> Result<(), String> {
    super::file::valid_path(path)?;
    let root = identity
        .root
        .as_deref()
        .ok_or("File recovery needs a working repository")?;
    let tracked = reader.output(
        &[
            "-C",
            root,
            "--literal-pathspecs",
            "ls-files",
            "--error-unmatch",
            "--",
            path,
        ],
        None,
    )?;
    if tracked.code == 0 {
        return Err(
            "Choose a new, untracked filename; tracked paths are preserved even when missing"
                .into(),
        );
    }
    // --error-unmatch deliberately diagnoses an untracked name. Validate that the
    // independent ignore query succeeds; unreadable ignore rules never authorize a write.
    if tracked.code != 1 {
        return Err("Cannot inspect the destination index entry".into());
    }
    let input = format!("{path}\0");
    let ignored = reader.output(
        &["-C", root, "check-ignore", "--no-index", "-z", "--stdin"],
        Some(input.as_bytes()),
    )?;
    if !ignored.stderr.is_empty() || !matches!(ignored.code, 0 | 1) {
        return Err("Cannot verify destination ignore rules".into());
    }
    if ignored.code == 0 || !ignored.stdout.is_empty() {
        return Err("Choose a nonignored destination filename".into());
    }
    Ok(())
}
fn stash_destination(reader: &Reader<'_>, source: &Provenance) -> Result<Option<String>, String> {
    if source.origin != Origin::DroppedStash {
        return Err("Only a validated dropped stash can return to the stash list".into());
    }
    let symbolic = reader.output(&["symbolic-ref", "--quiet", "refs/stash"], None)?;
    if symbolic.code != 1 || !symbolic.stderr.is_empty() {
        return Err("Recovery refuses a symbolic or unreadable stash destination".into());
    }
    let expected = optional_ref(reader, "refs/stash")?;
    if expected.is_some() {
        let log = reader.run(
            &[
                "log",
                "-g",
                "--format=%H",
                "--no-show-signature",
                "--max-count=501",
                "refs/stash",
                "--",
            ],
            None,
        )?;
        let text = std::str::from_utf8(&log).map_err(|_| "Invalid stash list")?;
        if text.lines().count() > 500 {
            return Err("The stash list is too large to verify safely".into());
        }
        if text.lines().any(|oid| oid == source.commit) {
            return Err("This stash is already in the stash list".into());
        }
    }
    Ok(expected)
}
pub(crate) fn prepare(
    repo: &Path,
    preview: &Preview,
    target: Target,
    active: &dyn Fn() -> bool,
) -> Result<Prepared, String> {
    let reader = Reader::new(repo, active);
    verify(&reader, preview)?;
    let mut expected_stash = None;
    match &target {
        Target::Branch(name) => {
            if name.is_empty()
                || name.starts_with('-')
                || name.len() > 240
                || name.chars().any(char::is_control)
            {
                return Err("Choose a plain new branch name".into());
            }
            let source = preview
                .source
                .as_ref()
                .ok_or("A dangling blob has no commit to branch from")?;
            if reader.run(&["check-ref-format", "--branch", name], None)?
                != format!("{name}\n").as_bytes()
            {
                return Err("Branch shorthand is not a new destination name".into());
            }
            if optional_ref(&reader, &format!("refs/heads/{name}"))?.is_some() {
                return Err("That branch already exists; choose a new name".into());
            }
            if !valid_oid(&source.commit) {
                return Err("Invalid source commit".into());
            }
        }
        Target::File(path) => {
            file_policy(&reader, &preview.identity, path)?;
            super::file::available(&reader, &preview.identity, path)?;
        }
        Target::Stash => {
            expected_stash = stash_destination(
                &reader,
                preview
                    .source
                    .as_ref()
                    .ok_or("A dangling blob is not a stash")?,
            )?;
        }
    }
    Ok(Prepared {
        preview: preview.clone(),
        target,
        expected_stash,
    })
}
fn verify(reader: &Reader<'_>, preview: &Preview) -> Result<(), String> {
    if reader.identity()? != preview.identity {
        return Err("The selected repository changed; reopen the recovery preview".into());
    }
    if let Some(selected) = &preview.source {
        source(reader, selected, &preview.blob)?;
    }
    if blob(reader, &preview.blob)?.as_slice() != preview.bytes.as_slice() {
        return Err("Recovery source bytes changed or were pruned".into());
    }
    Ok(())
}
pub(crate) fn perform(
    repo: &Path,
    prepared: &Prepared,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let context = crate::connections::current();
    context.run(|| {
        let reader = Reader::new(repo, active);
        verify(&reader, &prepared.preview)?;
        if !reader.active() {
            return Err("Recovery cancelled before mutation".into());
        }
        match &prepared.target {
            Target::Branch(name) => {
                let source = prepared
                    .preview
                    .source
                    .as_ref()
                    .ok_or("Missing source commit")?;
                // Revalidate destination and identity, then native create-only ref update.
                prepare(repo, &prepared.preview, prepared.target.clone(), active)?;
                let command = format!("create refs/heads/{name} {}\n", source.commit);
                reader.run(
                    &["update-ref", "--no-deref", "--stdin"],
                    Some(command.as_bytes()),
                )?;
                Ok(format!(
                    "Created {name}; HEAD and working files were preserved"
                ))
            }
            Target::File(path) => {
                file_policy(&reader, &prepared.preview.identity, path)?;
                super::file::create(
                    &reader,
                    &prepared.preview.identity,
                    path,
                    &prepared.preview.bytes,
                )?;
                Ok(format!(
                    "Recovered {path} as a new file; staged contents were preserved"
                ))
            }
            Target::Stash => {
                let selected = prepared
                    .preview
                    .source
                    .as_ref()
                    .ok_or("Missing stash source")?;
                let current = stash_destination(&reader, selected)?;
                if current != prepared.expected_stash {
                    return Err("The stash list moved since preview; prepare recovery again".into());
                }
                let zero = "0".repeat(if prepared.preview.identity.format == "sha256" {
                    64
                } else {
                    40
                });
                reader.run(
                    &[
                        "update-ref",
                        "--no-deref",
                        "--create-reflog",
                        "-m",
                        "gitcraft: recover dropped stash by content",
                        "refs/stash",
                        &selected.commit,
                        prepared.expected_stash.as_deref().unwrap_or(&zero),
                    ],
                    None,
                )?;
                Ok("Returned the dropped stash to the stash list; it was not applied".into())
            }
        }
    })
}
