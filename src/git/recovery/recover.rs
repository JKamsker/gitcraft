//! Immutable previews and deliberately separate, create-only recovery operations.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub identity: Identity,
    pub blob: String,
    pub source: Option<Provenance>,
    pub bytes: Arc<Vec<u8>>,
    pub lines: Arc<Vec<String>>,
    pub display_limited: bool,
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
        return Err(crate::i18n::text("backend-invalid-recovery-blob-id-f3ac36").into());
    }
    let mut checked = batch::check(reader, &[oid.to_owned()])?;
    let info = checked
        .pop()
        .and_then(|item| item.info)
        .ok_or(crate::i18n::text(
            "backend-the-saved-blob-is-no-longer-available-locally-952872",
        ))?;
    if info.0 != "blob" || info.1 > BLOB_LIMIT {
        return Err(crate::i18n::text(
            "backend-the-recovery-source-is-not-a-blob-within-2-mib-466f2f",
        )
        .into());
    }
    let mut objects = batch::read(reader, &[(oid.to_owned(), info.1)], "blob")?;
    let bytes = objects
        .pop()
        .ok_or(crate::i18n::text("backend-missing-recovery-blob-306f48"))?
        .1;
    let actual = reader.run(&["hash-object", "--no-filters", "--stdin"], Some(&bytes))?;
    if actual.strip_suffix(b"\n") != Some(oid.as_bytes()) {
        return Err(crate::i18n::text(
            "backend-recovery-blob-failed-its-object-identity-check-62b1b3",
        )
        .into());
    }
    Ok(bytes)
}
fn source(reader: &Reader<'_>, source: &Provenance, blob: &str) -> Result<(), String> {
    if !valid_oid(&source.commit) || !valid_oid(&source.tree) || !valid_oid(blob) {
        return Err(crate::i18n::text("backend-invalid-recovery-provenance-ef93c3").into());
    }
    let mut report = Report::default();
    let commit = read_commits(
        reader,
        &BTreeSet::from([source.commit.clone()]),
        &mut report,
    )?
    .remove(&source.commit)
    .ok_or(crate::i18n::text(
        "backend-the-source-commit-is-no-longer-available-locally-62eec5",
    ))?;
    let tree = match source.origin {
        Origin::DroppedStash | Origin::LiveStash => {
            let parents = read_commits(
                reader,
                &commit.parents.iter().skip(1).cloned().collect(),
                &mut report,
            )?;
            if !stash_shape(&commit, &parents) {
                return Err(crate::i18n::text(
                    "backend-the-source-is-not-a-validated-stash-object-e14336",
                )
                .into());
            }
            match source.layer {
                "working" => commit.tree.clone(),
                "staged" => parents
                    .get(&commit.parents[1])
                    .ok_or(crate::i18n::text("backend-missing-stash-index-6b6dcf"))?
                    .tree
                    .clone(),
                "untracked" if commit.parents.len() == 3 => parents
                    .get(&commit.parents[2])
                    .ok_or(crate::i18n::text(
                        "backend-missing-stash-untracked-tree-d98f5f",
                    ))?
                    .tree
                    .clone(),
                _ => {
                    return Err(
                        crate::i18n::text("backend-invalid-stash-content-layer-053a53").into(),
                    );
                }
            }
        }
        _ if source.layer == "working" => commit.tree,
        _ => return Err(crate::i18n::text("backend-invalid-commit-content-layer-b60655").into()),
    };
    if tree != source.tree {
        return Err(crate::i18n::text(
            "backend-recovery-provenance-tree-does-not-match-its-pinned-commit-338aee",
        )
        .into());
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
        return Err(crate::i18n::text(
            "backend-recovery-path-does-not-match-its-pinned-blob-6d828d",
        )
        .into());
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
    (|| {
        let reader = Reader::new(repo, active);
        if reader.identity() != Ok(results.identity.clone()) {
            return Err(crate::i18n::text(
                "backend-repository-changed-since-search-search-again-444078",
            )
            .into());
        }
        if !results
            .report
            .hits
            .iter()
            .any(|saved| saved.blob == hit.blob && saved.size == hit.size)
        {
            return Err(crate::i18n::text(
                "backend-this-content-is-not-a-hit-in-the-current-search-0a6956",
            )
            .into());
        }
        let selected = source_index
            .map(|index| {
                hit.provenance
                    .get(index)
                    .cloned()
                    .ok_or(crate::i18n::text("backend-invalid-source-selection-1a308a"))
            })
            .transpose()?;
        if let Some(selected) = &selected {
            source(&reader, selected, &hit.blob)?;
        }
        let bytes = blob(&reader, &hit.blob)?;
        if bytes.len() != hit.size {
            return Err(
                crate::i18n::text("backend-content-size-changed-since-search-c298d8").into(),
            );
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| crate::i18n::text("backend-binary-content-cannot-be-previewed-b457a3"))?;
        if text.contains('\0') || !text.contains(&results.query) {
            return Err(crate::i18n::text(
                "backend-the-saved-content-no-longer-matches-the-literal-search-97b031",
            )
            .into());
        }
        let mut display_limited = false;
        let lines: Vec<String> = text
            .lines()
            .take(2000)
            .map(|line| {
                if line.chars().count() > 400 {
                    display_limited = true;
                }
                line.chars().take(400).collect()
            })
            .collect();
        display_limited |= text.lines().take(2001).count() > 2000;
        // Keep full immutable raw bytes for recovery; UI renders only bounded excerpts.
        Ok(Preview {
            identity: results.identity.clone(),
            blob: hit.blob.clone(),
            source: selected,
            lines: Arc::new(lines),
            display_limited,
            bytes: Arc::new(bytes),
        })
    })()
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
        return Err(
            crate::i18n::text("backend-cannot-inspect-recovery-destination-ref-6f9454").into(),
        );
    }
    let oid = String::from_utf8(out.stdout)
        .map_err(|_| crate::i18n::text("backend-invalid-destination-object-id-89254e"))?
        .trim()
        .to_owned();
    if !valid_oid(&oid) {
        return Err(crate::i18n::text("backend-invalid-destination-object-id-89254e").into());
    }
    Ok(Some(oid))
}
fn file_policy(reader: &Reader<'_>, identity: &Identity, path: &str) -> Result<(), String> {
    super::file::valid_path(path)?;
    let root = identity.root.as_deref().ok_or(crate::i18n::text(
        "backend-file-recovery-needs-a-working-repository-4a4bd6",
    ))?;
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
        return Err(crate::i18n::text(
            "backend-choose-a-new-untracked-filename-tracked-paths-are-preserved-even-b8d282",
        )
        .into());
    }
    // --error-unmatch deliberately diagnoses an untracked name. Validate that the
    // independent ignore query succeeds; unreadable ignore rules never authorize a write.
    if tracked.code != 1 {
        return Err(
            crate::i18n::text("backend-cannot-inspect-the-destination-index-entry-e78a01").into(),
        );
    }
    let input = format!("{path}\0");
    let ignored = reader.output(
        &["-C", root, "check-ignore", "--no-index", "-z", "--stdin"],
        Some(input.as_bytes()),
    )?;
    if !ignored.stderr.is_empty() || !matches!(ignored.code, 0 | 1) {
        return Err(
            crate::i18n::text("backend-cannot-verify-destination-ignore-rules-d69be0").into(),
        );
    }
    if ignored.code == 0 || !ignored.stdout.is_empty() {
        return Err(
            crate::i18n::text("backend-choose-a-nonignored-destination-filename-c865ef").into(),
        );
    }
    Ok(())
}
fn stash_destination(reader: &Reader<'_>, source: &Provenance) -> Result<Option<String>, String> {
    if source.origin != Origin::DroppedStash {
        return Err(crate::i18n::text(
            "backend-only-a-validated-dropped-stash-can-return-to-the-stash-list-180c69",
        )
        .into());
    }
    let symbolic = reader.output(&["symbolic-ref", "--quiet", "refs/stash"], None)?;
    if symbolic.code != 1 || !symbolic.stderr.is_empty() {
        return Err(crate::i18n::text(
            "backend-recovery-refuses-a-symbolic-or-unreadable-stash-destination-185170",
        )
        .into());
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
        let text = std::str::from_utf8(&log)
            .map_err(|_| crate::i18n::text("backend-invalid-stash-list-e3f1a5"))?;
        if text.lines().count() > 500 {
            return Err(crate::i18n::text(
                "backend-the-stash-list-is-too-large-to-verify-safely-b2bb0a",
            )
            .into());
        }
        if text.lines().any(|oid| oid == source.commit) {
            return Err(crate::i18n::text(
                "backend-this-stash-is-already-in-the-stash-list-e8ba9f",
            )
            .into());
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
                return Err(
                    crate::i18n::text("backend-choose-a-plain-new-branch-name-c2e736").into(),
                );
            }
            let source = preview.source.as_ref().ok_or(crate::i18n::text(
                "backend-a-dangling-blob-has-no-commit-to-branch-from-8a8dd1",
            ))?;
            if reader.run(&["check-ref-format", "--branch", name], None)?
                != format!("{name}\n").as_bytes()
            {
                return Err(crate::i18n::text(
                    "backend-branch-shorthand-is-not-a-new-destination-name-9c8b51",
                )
                .into());
            }
            let reference = format!("refs/heads/{name}");
            let symbolic = reader.output(&["symbolic-ref", "--quiet", &reference], None)?;
            if symbolic.code != 1 || !symbolic.stderr.is_empty() {
                return Err(
                    crate::i18n::text("backend-the-branch-destination-is-symbolic-or-unreadable-choose-a-new-nam-0dff22").into(),
                );
            }
            if optional_ref(&reader, &reference)?.is_some() {
                return Err(crate::i18n::text(
                    "backend-that-branch-already-exists-choose-a-new-name-992323",
                )
                .into());
            }
            if !valid_oid(&source.commit) {
                return Err(crate::i18n::text("backend-invalid-source-commit-587a58").into());
            }
        }
        Target::File(path) => {
            file_policy(&reader, &preview.identity, path)?;
            super::file::available(&reader, &preview.identity, path)?;
        }
        Target::Stash => {
            expected_stash = stash_destination(
                &reader,
                preview.source.as_ref().ok_or(crate::i18n::text(
                    "backend-a-dangling-blob-is-not-a-stash-31781e",
                ))?,
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
        return Err(crate::i18n::text(
            "backend-the-selected-repository-changed-reopen-the-recovery-preview-daf464",
        )
        .into());
    }
    if let Some(selected) = &preview.source {
        source(reader, selected, &preview.blob)?;
    }
    if blob(reader, &preview.blob)?.as_slice() != preview.bytes.as_slice() {
        return Err(crate::i18n::text(
            "backend-recovery-source-bytes-changed-or-were-pruned-6b4858",
        )
        .into());
    }
    Ok(())
}
pub(crate) fn perform(
    repo: &Path,
    prepared: &Prepared,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    (|| {
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
                    .ok_or(crate::i18n::text("backend-missing-source-commit-d182e6"))?;
                // Revalidate destination and identity, then native create-only ref update.
                prepare(repo, &prepared.preview, prepared.target.clone(), active)?;
                let command = format!("create refs/heads/{name} {}\n", source.commit);
                reader.run(
                    &["update-ref", "--no-deref", "--stdin"],
                    Some(command.as_bytes()),
                )?;
                Ok(crate::i18n::message(
                    "backend-created-name-head-and-working-files-were-preserved-6e7643",
                    &[("name", (name).to_string().into())],
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
                Ok(crate::i18n::message(
                    "backend-recovered-path-as-a-new-file-staged-contents-were-preserved-719350",
                    &[("path", (path).to_string().into())],
                ))
            }
            Target::Stash => {
                let selected = prepared
                    .preview
                    .source
                    .as_ref()
                    .ok_or(crate::i18n::text("backend-missing-stash-source-8adf5d"))?;
                let current = stash_destination(&reader, selected)?;
                if current != prepared.expected_stash {
                    return Err(crate::i18n::text(
                        "backend-the-stash-list-moved-since-preview-prepare-recovery-again-2a3cd2",
                    )
                    .into());
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
                Ok(crate::i18n::text("backend-returned-the-dropped-stash-to-the-stash-list-it-was-not-applied-1220c3").into())
            }
        }
    })()
}
