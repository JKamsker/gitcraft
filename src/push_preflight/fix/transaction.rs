//! Local-only guarded repair. Git owns HEAD and branch locks in one native transaction.
//! Capability proof occurs in an owned scratch repository before any user-ref mutation.
use std::path::{Path, PathBuf};

use crate::{git, remote};

const MAX_INDEX: u64 = 64 * 1024 * 1024;
const NO_REMOTE: &str = "Automatic remote repair requires completion-aware ownership; no remote repair was dispatched. Preview and reviewed push remain available.";

pub(super) struct Hook {
    directory: PathBuf,
    keep: bool,
    old: String,
    new: String,
    proof: String,
    capability_acknowledged: bool,
}

impl Hook {
    pub(super) fn new(repo: &Path, branch: &str, old: &str, new: &str) -> Result<Self, String> {
        if remote::is_remote(repo) {
            return Err(NO_REMOTE.into());
        }
        if !supported_version(&read(repo, &["--version"])?) {
            return Err("Guarded repair requires Git 2.50 or later plus a successful native HEAD/ref capability proof. No repair reference update was launched.".into());
        }
        let ref_format = path_line(read(repo, &["rev-parse", "--show-ref-format"])?)?;
        let object_format = path_line(read(repo, &["rev-parse", "--show-object-format"])?)?;
        if !matches!(ref_format.as_str(), "files" | "reftable")
            || !matches!(object_format.as_str(), "sha1" | "sha256")
        {
            return Err(
                "This Git reference/object backend cannot be proven safe for automatic repair"
                    .into(),
            );
        }
        // Refuse a symbolic branch rather than replacing its indirection with a direct ref.
        if !git::forecast_run(repo, &["symbolic-ref", "-q", branch], &[], &[0, 1], &|| {
            true
        })?
        .is_empty()
        {
            return Err("Automatic repair requires a direct local branch reference".into());
        }
        let original = path_line(read(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "hooks/reference-transaction",
            ],
        )?)?;
        let index = path_line(read(
            repo,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )?)?;
        if !Path::new(&original).is_absolute() || !Path::new(&index).is_absolute() {
            return Err(
                "Git did not provide absolute metadata paths; safe repair was refused".into(),
            );
        }
        let flags = read(repo, &["ls-files", "-v", "-z"])?;
        if flags
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .any(|entry| !entry.starts_with(b"H "))
        {
            return Err("Clear sparse/skip-worktree/assume-unchanged index flags before automatic tree repair".into());
        }
        let before_tree = path_line(read(
            repo,
            &["rev-parse", "--verify", &format!("{old}^{{tree}}")],
        )?)?;
        let after_tree = path_line(read(
            repo,
            &["rev-parse", "--verify", &format!("{new}^{{tree}}")],
        )?)?;
        let needs_checkout = before_tree != after_tree;
        let affected = read(
            repo,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                "-z",
                "--no-renames",
                old,
                new,
            ],
        )?;
        let affected: Vec<String> = affected
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                String::from_utf8(path.to_vec())
                    .map_err(|_| "Safe repair requires UTF-8 changed paths".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "No clock")?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "gitcraft-push-ref-transaction-{}-{stamp}",
            std::process::id()
        ));
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder
            .create(&directory)
            .map_err(|_| "Could not create owned repair evidence")?;
        let mut hook = Self {
            directory,
            keep: false,
            old: old.into(),
            new: new.into(),
            proof: format!("{branch}\n{old}\n{new}\n"),
            capability_acknowledged: false,
        };
        let mut ownership_uncertain = false;
        let pair = snapshot_pair(&index, &hook.directory, |saved_index| {
            let saved_index = saved_index
                .to_str()
                .ok_or("Owned index path is not UTF-8")?;
            ownership_uncertain = true;
            // Resolve the dependency encoded by the immutable saved main index,
            // never a separately observed live index. Retain the originating repository
            // context for Git's shared-index lookup and Practice/profile isolation.
            let shared = local(
                repo,
                &["rev-parse", "--path-format=absolute", "--shared-index-path"],
                b"",
                &[("GIT_INDEX_FILE", saved_index.to_owned())],
            )?;
            ownership_uncertain = false;
            if shared == b"\n" || shared.is_empty() {
                Ok(None)
            } else {
                path_line(shared).map(Some)
            }
        });
        hook.keep = ownership_uncertain;
        let shared = pair?;
        let recovery = format!(
            "Guarded repair recovery\nBranch: {branch}\nOriginal: {old}\nRewritten: {new}\nIndex path: {index}\nShared-index dependency: {}\n\nInspect HEAD, git status, git diff and git diff --cached before recovery.\nSaved before.index and before.shared-index (when present) are owned evidence, not permission to overwrite concurrent edits.\nSave concurrent work before deliberately restoring the original tree/ref.\nAfter saving work, git reset --merge {old} may help; do not remove locks owned by another operation.\n",
            shared.as_deref().unwrap_or("none")
        );
        std::fs::write(hook.directory.join("recovery.txt"), recovery)
            .map_err(|_| "Could not write recovery instructions")?;
        let wrapper = script(HookScript {
            original: &original,
            branch,
            old,
            new,
            index: &index,
            shared: shared.as_deref(),
            affected: &affected,
            proof: &hook.proof,
            needs_checkout,
        });
        let wrapper_path = hook.directory.join("reference-transaction");
        std::fs::write(&wrapper_path, wrapper)
            .map_err(|_| "Could not write the private repair hook")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(wrapper_path, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not protect repair hook")?;
        }
        // Uncertain proof completion retains the owned wrapper and evidence.
        hook.keep = true;
        capability::prove(&hook.directory, repo, &ref_format, &object_format)?;
        hook.capability_acknowledged = true;
        hook.keep = false;
        // Fresh private indexes have no old working-file stat cache. Comparing through
        // them after the original hook re-reads working content without changing the
        // actual index. Each check uses a separate never-refreshed index snapshot.
        // Porcelain diff checks content when these entries have no cached stat data;
        // plumbing diff-files --quiet reports such clean entries as modified. Optional
        // locks stay disabled for these private-index comparisons.
        hook.keep = true;
        for (name, tree) in [
            ("check-before.index", old),
            ("check-after.index", new),
            ("check-committed.index", new),
        ] {
            local(
                repo,
                &[
                    "-c",
                    "core.hooksPath=",
                    "-c",
                    "core.sparseCheckout=false",
                    "read-tree",
                    tree,
                ],
                b"",
                &[(
                    "GIT_INDEX_FILE",
                    hook.directory.join(name).to_string_lossy().into_owned(),
                )],
            )?;
        }
        std::fs::write(hook.directory.join("mode"), b"repair")
            .map_err(|_| "Could not arm local repair")?;
        hook.keep = false;
        Ok(hook)
    }

    pub(super) fn execute(mut self, repo: &Path) -> Result<(), String> {
        if remote::is_remote(repo) {
            return Err(NO_REMOTE.into());
        }
        if !self.capability_acknowledged {
            return Err(
                "No native checkout/ref capability acknowledgement; repair refused before dispatch"
                    .into(),
            );
        }
        // From dispatch onward, uncertainty ALWAYS retains the wrapper/evidence. Marker
        // absence is never completion proof; no remote request is ever dispatched here.
        self.keep = true;
        let hooks = format!("core.hooksPath={}", remote::git_path(&self.directory));
        let result = local(
            repo,
            &[
                "-c",
                &hooks,
                "update-ref",
                "-m",
                "gitcraft push preflight fix",
                "--stdin",
            ],
            transaction(&self.old, &self.new).as_bytes(),
            &[],
        );
        let completed = result.is_ok()
            && ["prepared", "checkout", "committed"]
                .into_iter()
                .all(|phase| ack(&self.directory, phase, self.proof.as_bytes()));
        if !completed {
            return Err(format!(
                "The local repair did not establish complete owned acknowledgement. Preserve concurrent edits; inspect recovery.txt and saved index/dependency in {}. No automatic rollback overwrote later work.",
                self.directory.display()
            ));
        }
        self.keep = false;
        Ok(())
    }
}

fn ack(directory: &Path, phase: &str, expected: &[u8]) -> bool {
    std::fs::read(directory.join(format!("{phase}.ok"))).is_ok_and(|bytes| bytes == expected)
}

impl Drop for Hook {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

mod support;
use support::{local, owned, read, snapshot_pair, transaction};
pub(super) use support::{path_line, supported_version};
mod capability;
mod wrapper;
pub(super) use wrapper::{HookScript, script};
#[cfg(all(test, unix))]
mod owned_fixtures;
#[cfg(test)]
mod tests;
