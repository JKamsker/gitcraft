//! Shared directories, borrowed stores and verified, donor-preserving independence.
#[cfg(unix)]
#[path = "held_unix.rs"]
mod held;
#[cfg(windows)]
#[path = "held_windows.rs"]
mod held;
#[cfg(test)]
mod tests;
pub(super) mod topology;
use super::{
    guard::{Runner, Snapshot, text},
    object, read_with,
};
use crate::remote;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub(crate) struct Sharing {
    pub git: String,
    pub common: String,
    pub objects: PathBuf,
    pub topology: topology::Topology,
    pub blocked: Option<String>,
    pub plan: Option<Plan>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    repo: PathBuf,
    format: String,
    topology: topology::Topology,
    snapshot: Snapshot,
    index: String,
    reflogs: String,
    roots: String,
    grafts: Option<Vec<u8>>,
    ownership: Arc<held::Ownership>,
}
fn line(repo: &Path, args: &[&str]) -> Result<String, String> {
    Ok(text(repo, args)?.trim_end().into())
}
fn roots(repo: &Path) -> Result<String, String> {
    // Git 2.45+ is required here, including enumeration of arbitrary root refs.
    let out = line(
        repo,
        &[
            "--no-lazy-fetch",
            "--no-replace-objects",
            "for-each-ref",
            "--include-root-refs",
            "--format=%(refname)%00%(objectname)",
        ],
    )?;
    let mut ids = BTreeSet::new();
    for row in out.lines() {
        let (name, oid) = row
            .split_once('\0')
            .ok_or(crate::i18n::text("backend-invalid-root-ref-record-97e0e8"))?;
        if let Some(original) = name
            .strip_prefix("refs/replace/")
            .filter(|s| object::valid_oid(s))
        {
            ids.insert(original.to_owned());
        }
        if !object::valid_oid(oid) {
            return Err(crate::i18n::text("backend-unexpected-root-object-id-af416a").into());
        }
        ids.insert(oid.to_owned());
    }
    // FETCH_HEAD and MERGE_HEAD can contain several objects and aren't ordinary refs.
    for name in [
        "HEAD",
        "ORIG_HEAD",
        "FETCH_HEAD",
        "MERGE_HEAD",
        "AUTO_MERGE",
        "CHERRY_PICK_HEAD",
        "REBASE_HEAD",
        "REVERT_HEAD",
        "BISECT_HEAD",
    ] {
        let p = line(
            repo,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )?;
        if let Some(bytes) = object::files::read(&remote::resolve(repo, &p), 1 << 20, &|| true)? {
            for row in bytes.split(|b| *b == b'\n') {
                if let Some(oid) = row
                    .split(|b| b.is_ascii_whitespace())
                    .next()
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .filter(|s| object::valid_oid(s))
                {
                    ids.insert(oid.to_owned());
                }
            }
        }
    }
    if let Some(bytes) = grafts(repo)? {
        for row in bytes
            .split(|b| *b == b'\n')
            .filter(|r| !r.is_empty() && !r.starts_with(b"#"))
        {
            for oid in row
                .split(|b| b.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
            {
                let oid = std::str::from_utf8(oid).map_err(|_| {
                    crate::i18n::text("backend-legacy-graft-metadata-is-not-utf-8-e457c9")
                })?;
                if !object::valid_oid(oid) {
                    return Err(
                        crate::i18n::text("backend-invalid-legacy-graft-metadata-repair-it-before-independence-740c36").into(),
                    );
                }
                ids.insert(oid.to_owned());
            }
        }
    }
    Ok(ids.into_iter().collect::<Vec<_>>().join("\n") + "\n")
}
fn grafts(repo: &Path) -> Result<Option<Vec<u8>>, String> {
    let path = line(
        repo,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/grafts",
        ],
    )?;
    object::files::read(&remote::resolve(repo, &path), 1 << 20, &|| true)
}
pub(crate) fn inspect(repo: &Path) -> Result<Sharing, String> {
    let git = line(repo, &["rev-parse", "--absolute-git-dir"])?;
    let common = line(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let objects = remote::resolve(
        repo,
        &line(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "objects",
            ],
        )?,
    );
    let topology = topology::read(repo, &objects, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })?;
    let prepare = (|| {
        if remote::is_remote(repo) {
            return Err(crate::i18n::text(
                "backend-for-independence-open-this-repository-locally-on-its-machine-this-3bb2c6",
            )
            .to_owned());
        }
        if git != common
            || topology
                .worktrees
                .lines()
                .filter(|s| s.starts_with("worktree "))
                .count()
                != 1
        {
            return Err(crate::i18n::text(
                "backend-this-repository-shares-a-git-directory-with-linked-worktrees-remo-cd2516",
            )
            .into());
        }
        if topology.alternates.as_ref().is_none_or(|s| s.is_empty()) {
            return Err(crate::i18n::text(
                "backend-this-clone-has-no-alternate-object-links-to-remove-bd3b73",
            )
            .into());
        }
        if topology.promisor {
            return Err(crate::i18n::text(
                "backend-partial-clones-may-have-uncopied-promised-objects-materialize-the-58e5ce",
            )
            .into());
        }
        if !topology.warnings.is_empty()
            || topology
                .stores
                .iter()
                .any(|s| !s.available || s.error.is_some())
        {
            return Err(crate::i18n::text(
                "backend-repair-the-missing-cyclic-http-or-unreadable-donors-before-prepar-2a920e",
            )
            .into());
        }
        let repo = std::fs::canonicalize(repo).map_err(|e| e.to_string())?;
        let git = std::fs::canonicalize(&git).map_err(|e| e.to_string())?;
        let format = line(&repo, &["rev-parse", "--show-object-format"])?;
        if !matches!(format.as_str(), "sha1" | "sha256") {
            return Err(crate::i18n::text("backend-unsupported-object-format-f8189c").into());
        }
        let ownership = Arc::new(held::Ownership::capture(
            &repo,
            &git,
            &git.join("objects"),
            topology.alternates.as_deref().unwrap_or_default(),
        )?);
        let roots = roots(&repo).map_err(|e| {
            crate::i18n::message(
                "backend-independence-requires-git-2-45-with-root-ref-enumeration-and-no-l-536432",
                &[("detail", (e).to_string().into())],
            )
        })?;
        Ok(Plan {
            snapshot: Snapshot::capture(&repo)?,
            index: line(&repo, &["ls-files", "--stage", "-z"])?,
            reflogs: line(
                &repo,
                &["reflog", "show", "--all", "--format=%gD%x00%H%x00%gs"],
            )?,
            format,
            topology: topology.clone(),
            grafts: grafts(&repo)?,
            repo,
            roots,
            ownership,
        })
    })();
    let (plan, blocked) = match prepare {
        Ok(p) => (Some(p), None),
        Err(e) => (None, Some(e)),
    };
    Ok(Sharing {
        git,
        common,
        objects,
        topology,
        blocked,
        plan,
    })
}
impl Plan {
    fn check(&self) -> Result<(), String> {
        self.ownership.current(true)?;
        self.check_state()?;
        let objects = self.repo.join(".git/objects");
        if self.topology != topology::read(&self.repo, &objects, &|| true)? {
            return Err(crate::i18n::text(
                "backend-object-borrowing-topology-changed-prepare-again-68de87",
            )
            .into());
        }
        Ok(())
    }
    fn check_state(&self) -> Result<(), String> {
        self.snapshot.check(&self.repo)?;
        if self.grafts != grafts(&self.repo)?
            || self.roots != roots(&self.repo)?
            || self.index != line(&self.repo, &["ls-files", "--stage", "-z"])?
            || self.reflogs
                != line(
                    &self.repo,
                    &["reflog", "show", "--all", "--format=%gD%x00%H%x00%gs"],
                )?
        {
            return Err(crate::i18n::text(
                "backend-roots-index-or-reflogs-changed-prepare-independence-again-288e0b",
            )
            .into());
        }
        Ok(())
    }
}
fn binary(
    runner: &Runner<'_>,
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
) -> Result<Vec<u8>, String> {
    if runner.control.is_cancelled() {
        return Err(crate::git::CANCELLED.into());
    }
    let args: Vec<_> = [
        "--no-lazy-fetch",
        "--no-replace-objects",
        "--no-optional-locks",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "-c",
        "pack.threads=2",
    ]
    .into_iter()
    .chain(args.iter().copied())
    .collect();
    // Disable legacy grafts as well: a self-contained pack must retain original ancestry.
    read_with(
        repo,
        &args,
        input,
        cap,
        &[(
            "GIT_GRAFT_FILE",
            if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
        )],
        Some(runner.control.clone()),
    )
}
pub(crate) fn independent(runner: &Runner<'_>, plan: &Plan) -> Result<String, String> {
    let _lock = plan.ownership.lock()?;
    plan.check()?;
    if std::fs::canonicalize(runner.repo).map_err(|e| e.to_string())? != plan.repo {
        return Err(crate::i18n::text("backend-repository-changed-cc1fe0").into());
    }
    let census = binary(
        runner,
        &plan.repo,
        &[
            "rev-list",
            "--objects",
            "--all",
            "--reflog",
            "--indexed-objects",
            "--missing=print",
            "--no-object-names",
            "--stdin",
        ],
        plan.roots.as_bytes().to_vec(),
        32 << 20,
    )?;
    let required: BTreeSet<_> = String::from_utf8(census)
        .map_err(|_| crate::i18n::text("backend-invalid-object-census-4acde6"))?
        .lines()
        .map(str::to_owned)
        .collect();
    if required.iter().any(|s| !object::valid_oid(s)) {
        return Err(crate::i18n::text(
            "backend-some-required-objects-are-missing-the-alternate-was-retained-373c8a",
        )
        .into());
    }
    // A complete, non-thin pack includes reflog/index/private/root refs and original history.
    let pack = binary(
        runner,
        &plan.repo,
        &[
            "pack-objects",
            "--stdout",
            "--revs",
            "--all",
            "--reflog",
            "--indexed-objects",
            "--no-reuse-delta",
            "--no-reuse-object",
        ],
        plan.roots.as_bytes().to_vec(),
        128 << 20,
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-copy-a-complete-pack-128-mib-staging-limit-borrowing-is-dec4ed",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let scratch = tempfile::Builder::new()
        .prefix("gitcraft-independent-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    runner.in_repo(
        scratch.path(),
        &[
            "init",
            "--bare",
            "--template=",
            &format!("--object-format={}", plan.format),
            ".",
        ],
    )?;
    let hash = String::from_utf8(binary(
        runner,
        scratch.path(),
        &["index-pack", "--stdin", "--strict"],
        pack.clone(),
        65536,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-index-pack-result-ba9ee9"))?;
    let hash = hash
        .split_whitespace()
        .last()
        .filter(|h| object::valid_oid(h))
        .ok_or(crate::i18n::text("backend-invalid-pack-hash-86f617"))?;
    let index_path = scratch.path().join(format!("objects/pack/pack-{hash}.idx"));
    let printed = index_path.to_str().ok_or(crate::i18n::text(
        "backend-pack-staging-path-is-not-utf-8-f2a6fb",
    ))?;
    let rows = object::pack_rows(
        &String::from_utf8(binary(
            runner,
            scratch.path(),
            &["verify-pack", "-v", "--", printed],
            Vec::new(),
            32 << 20,
        )?)
        .map_err(|_| crate::i18n::text("backend-invalid-pack-verification-3f4baa"))?,
    )?;
    if required.iter().any(|oid| !rows.contains_key(oid)) {
        return Err(crate::i18n::text(
            "backend-independent-pack-is-missing-a-required-object-borrowing-is-retain-ddbdea",
        )
        .into());
    }
    let index = object::files::read(&index_path, 32 << 20, &|| true)?
        .ok_or(crate::i18n::text("backend-missing-staged-index-0a7d28"))?;
    plan.check()?;
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let receipt = crate::i18n::message(
        "backend-verified-pack-git-objects-pack-pack-hash-pack-and-idx-may-remain-aa97d9",
        &[("hash", (hash).to_string().into())],
    );
    plan.ownership
        .install(hash, &pack, &index, &nonce)
        .map_err(|e| format!("{e}. {receipt}"))?;
    plan.check().map_err(|e| format!("{e}. {receipt}"))?;
    if runner.control.is_cancelled() {
        return Err(format!("{}. {receipt}", crate::git::CANCELLED));
    }
    let backup = format!("alternates.gitcraft-backup-{nonce}");
    plan.ownership.detach(&backup)?;
    // Once detached, always finish proof or restore, even when cancellation is requested.
    let proof = (|| {
        plan.ownership.current(false)?;
        plan.ownership.verify_backup(&backup)?;
        let input = required.iter().cloned().collect::<Vec<_>>().join("\n") + "\n";
        let out = read_with(
            &plan.repo,
            &[
                "--no-lazy-fetch",
                "--no-replace-objects",
                "cat-file",
                "--batch-check=%(objectname) %(objecttype)",
            ],
            input.into_bytes(),
            32 << 20,
            &[],
            None,
        )?;
        let lines = String::from_utf8(out)
            .map_err(|_| crate::i18n::text("backend-invalid-independent-object-proof-1dbe30"))?;
        if lines.lines().count() != required.len() || lines.lines().any(|s| s.ends_with(" missing"))
        {
            return Err(
                crate::i18n::text("backend-independent-object-proof-failed-0a36c1").to_owned(),
            );
        }
        plan.check_state()?;
        plan.ownership.current(false)?;
        plan.ownership.verify_backup(&backup)?;
        Ok(())
    })();
    if let Err(e) = proof {
        let restored = plan.ownership.restore(&backup);
        return Err(crate::i18n::message(
            "backend-e-alternate-recovery-restored-retained-git-objects-info-backup-1386b4",
            &[
                ("detail", (e).to_string().into()),
                ("restored", format!("{:?}", restored).into()),
                ("backup", (backup).to_string().into()),
            ],
        ));
    }
    Ok(crate::i18n::message(
        "backend-copied-and-independently-verified-required-objects-alternate-link-6fc4fd",
        &[
            ("count", (required.len()).into()),
            ("backup", (backup).to_string().into()),
        ],
    ))
}
