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

fn supported_version(text: &[u8]) -> bool {
    let Some(version) = std::str::from_utf8(text)
        .ok()
        .and_then(|text| text.strip_prefix("git version "))
        .and_then(|text| text.split_whitespace().next())
    else {
        return false;
    };
    let mut parts = version.split('.');
    let parsed = (|| {
        Some((
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
        ))
    })();
    parsed.is_some_and(|version| version >= (2, 50, 0))
}

fn path_line(bytes: Vec<u8>) -> Result<String, String> {
    let text = String::from_utf8(bytes).map_err(|_| "Safe repair requires UTF-8 metadata paths")?;
    let text = text.strip_suffix('\n').unwrap_or(&text);
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err("Safe repair requires metadata paths without control characters".into());
    }
    Ok(text.to_owned())
}

fn read(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git::forecast_bytes(repo, args, &[], &[0], &|| true)
}

fn success(output: std::process::Output) -> Result<Vec<u8>, String> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("The guarded Git repair command was refused; tool diagnostics were withheld.".into())
    }
}

fn local(
    repo: &Path,
    args: &[&str],
    input: &[u8],
    env: &[(&'static str, String)],
) -> Result<Vec<u8>, String> {
    success(git::local_repair_input(repo, args, input, env)?)
}

fn owned(
    context_repo: &Path,
    directory: &Path,
    args: &[&str],
    input: &[u8],
    env: &[(&'static str, String)],
) -> Result<Vec<u8>, String> {
    // Keep the originating repository's Practice-first/profile context even while Git
    // works in an application-owned scratch directory. The second -C is absolute.
    let directory = remote::git_path(directory);
    let words: Vec<_> = ["-C", directory.as_str()]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    local(context_repo, &words, input, env)
}

fn transaction(old: &str, new: &str) -> String {
    // Updating symbolic HEAD lets Git acquire HEAD and its referent together. An
    // explicit symref-verify plus referent update collides with Git's implicit HEAD
    // reflog update. The private hook separately requires the exact intended branch
    // in native stdin and symbolic HEAD before/after the original prepared hook.
    format!("start\nupdate HEAD {new} {old}\nprepare\ncommit\n")
}

fn snapshot(source: &str, destination: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut input =
        std::fs::File::open(source).map_err(|_| "Could not snapshot the original index")?;
    let mut bytes = Vec::new();
    (&mut input)
        .take(MAX_INDEX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the original index")?;
    if bytes.len() as u64 > MAX_INDEX {
        return Err("Index recovery snapshot exceeds 64 MiB".into());
    }
    std::fs::write(destination, bytes)
        .map_err(|_| "Could not save the owned index recovery snapshot".into())
}

/// The resolver observes only the immutable owned main-index snapshot. If Git cannot
/// load that snapshot's dependency in this repository context, refuse before dispatch.
fn snapshot_pair(
    source_index: &str,
    directory: &Path,
    resolve: impl FnOnce(&Path) -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    let saved_index = directory.join("before.index");
    snapshot(source_index, &saved_index)?;
    let dependency = resolve(&saved_index)?;
    if let Some(path) = &dependency {
        if !Path::new(path).is_absolute() {
            return Err("Saved index dependency was not an absolute path; repair refused".into());
        }
        snapshot(path, &directory.join("before.shared-index"))?;
    }
    Ok(dependency)
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
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
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
        hook.prove(repo, &ref_format, &object_format)?;
        // Fresh private indexes have no old working-file stat cache. Comparing through
        // them after the original hook re-reads working content without changing the
        // actual index. Each check uses a separate never-refreshed index snapshot.
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

    fn prove(&mut self, repo: &Path, refs: &str, objects: &str) -> Result<(), String> {
        let probe = self.directory.join("capability.git");
        let format = format!("--object-format={objects}");
        let refs = format!("--ref-format={refs}");
        // Every subprocess uses the existing owned bounded runner. If completion is
        // uncertain even for a scratch request, retain its directory rather than racing it.
        self.keep = true;
        owned(
            repo,
            &self.directory,
            &[
                "-c",
                "core.hooksPath=",
                "init",
                "--bare",
                "--template=",
                &format,
                &refs,
                &probe.to_string_lossy(),
            ],
            b"",
            &[],
        )?;
        let blob = self.directory.join("empty-tree");
        std::fs::write(&blob, b"").map_err(|_| "Could not write capability input")?;
        let tree = path_line(owned(
            repo,
            &probe,
            &[
                "-c",
                "core.hooksPath=",
                "hash-object",
                "-w",
                "--no-filters",
                "-t",
                "tree",
                "--",
                &blob.to_string_lossy(),
            ],
            b"",
            &[],
        )?)?;
        let metadata = vec![
            ("GIT_AUTHOR_NAME", "Gitcraft capability".into()),
            ("GIT_AUTHOR_EMAIL", "probe@example.invalid".into()),
            ("GIT_COMMITTER_NAME", "Gitcraft capability".into()),
            ("GIT_COMMITTER_EMAIL", "probe@example.invalid".into()),
        ];
        let commit = |message: &str| -> Result<String, String> {
            path_line(owned(
                repo,
                &probe,
                &[
                    "-c",
                    "core.hooksPath=",
                    "commit-tree",
                    "--no-gpg-sign",
                    &tree,
                    "-m",
                    message,
                ],
                b"",
                &metadata,
            )?)
        };
        let old = commit("Before capability proof")?;
        let new = commit("After capability proof")?;
        let branch = "refs/heads/gitcraft-capability";
        owned(
            repo,
            &probe,
            &["-c", "core.hooksPath=", "symbolic-ref", "HEAD", branch],
            b"",
            &[],
        )?;
        owned(
            repo,
            &probe,
            &["-c", "core.hooksPath=", "update-ref", branch, &old],
            b"",
            &[],
        )?;
        std::fs::write(self.directory.join("mode"), b"probe")
            .map_err(|_| "Could not arm capability proof")?;
        let hooks = format!("core.hooksPath={}", remote::git_path(&self.directory));
        let command_probe = self.directory.join("gitcraft-capability");
        std::fs::write(&command_probe, "#!/bin/sh\ndirectory=$(dirname \"$0\")\ncat >\"$directory/hook-command.stdin\" || exit 1\nprintf 'probe\\n' >\"$directory/hook-command.ok\"\n")
            .map_err(|_| "Could not write owned hook-command proof")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&command_probe, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not protect owned capability hook")?;
        }
        let hook_input = self.directory.join("hook-input");
        std::fs::write(&hook_input, b"owned capability input\n")
            .map_err(|_| "Could not prepare hook stdin proof")?;
        let input_arg = format!("--to-stdin={}", hook_input.to_string_lossy());
        owned(
            repo,
            &probe,
            &[
                "-c",
                &hooks,
                "hook",
                "run",
                "--ignore-missing",
                &input_arg,
                "gitcraft-capability",
            ],
            b"",
            &[],
        )?;
        if !ack(&self.directory, "hook-command", b"probe\n")
            || std::fs::read(self.directory.join("hook-command.stdin"))
                .ok()
                .as_deref()
                != Some(&b"owned capability input\n"[..])
        {
            return Err("Git did not prove original-hook command/stdin support; no user update was launched".into());
        }
        let result = owned(
            repo,
            &probe,
            &["-c", &hooks, "update-ref", "--stdin"],
            transaction(old.trim(), new.trim()).as_bytes(),
            &[],
        );
        if result.is_err()
            || !ack(&self.directory, "probe-prepared", b"probe\n")
            || !ack(&self.directory, "probe-committed", b"probe\n")
        {
            return Err("This Git/backend did not prove private hook selection and an atomic current-HEAD/branch update. No user reference update was launched.".into());
        }
        let expected_update = format!("{} {} {branch}\n", old.trim(), new.trim());
        let recorded = std::fs::read(self.directory.join("probe-updates-prepared"))
            .map_err(|_| "Missing native transaction input proof")?;
        if recorded != expected_update.as_bytes()
            || path_line(owned(repo, &probe, &["symbolic-ref", "HEAD"], b"", &[])?)? != branch
            || path_line(owned(
                repo,
                &probe,
                &["rev-parse", "--verify", "HEAD"],
                b"",
                &[],
            )?)? != new.trim()
        {
            return Err("The native HEAD/branch transaction result was not proven; no user update was launched".into());
        }
        self.capability_acknowledged = true;
        self.keep = false;
        Ok(())
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

struct HookScript<'a> {
    original: &'a str,
    branch: &'a str,
    old: &'a str,
    new: &'a str,
    index: &'a str,
    shared: Option<&'a str>,
    affected: &'a [String],
    proof: &'a str,
    needs_checkout: bool,
}

fn script(args: HookScript<'_>) -> String {
    let HookScript {
        original,
        branch,
        old,
        new,
        index,
        shared,
        affected,
        proof,
        needs_checkout,
    } = args;
    let original_dir = original
        .rsplit_once('/')
        .map_or(original, |(directory, _)| directory);
    let paths = affected
        .iter()
        .map(|path| remote::sh_quote(path))
        .collect::<Vec<_>>()
        .join(" ");
    let shared_check = shared
        .map(|path| {
            format!(
                "cmp -s -- {} \"$directory/before.shared-index\" || exit 1\n",
                remote::sh_quote(path)
            )
        })
        .unwrap_or_default();
    let payload = format!(
        r#"case "$1" in prepared|committed|aborted) ;; *) exit 1 ;; esac
directory=$(dirname "$0")
command -v cmp >/dev/null && command -v cp >/dev/null || exit 1
if [ "$(cat "$directory/mode")" = probe ]; then
    cat >"$directory/probe-updates-$1" || exit 1
    printf 'probe\n' >"$directory/probe-$1.ok" || exit 1
    exit 0
fi
updates="$directory/updates-$1"
cat >"$updates" || exit 1
if [ "$1" = prepared ]; then
    matched=0
    while read -r before after reference extra; do
        [ -z "$extra" ] || exit 1
        [ "$reference" = {branch} ] || exit 1
        [ "$before" = {old} ] && [ "$after" = {new} ] || exit 1
        matched=$((matched+1))
    done <"$updates"
    [ "$matched" = 1 ] || exit 1
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@" || exit $?
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    cmp -s -- {index} "$directory/before.index" || exit 1
    {shared_check}
    GIT_INDEX_FILE="$directory/check-before.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff-files --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    if [ {needs_checkout} = true ]; then
        git --literal-pathspecs -c core.fsmonitor=false status --porcelain=v1 -z --untracked-files=all --ignored=matching -- {paths} >"$directory/affected-status" || exit 1
        [ ! -s "$directory/affected-status" ] || exit 1
    fi
    printf '%s' {proof} >"$directory/prepared.ok" || exit 1
    if [ {needs_checkout} = true ]; then
        printf 'attempted\n' >"$directory/checkout-attempted" || exit 1
        git -c core.fsmonitor=false read-tree -u -m {old} {new} || exit 1
    fi
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    git -c core.fsmonitor=false diff-index --cached --quiet --no-ext-diff --no-textconv {new} -- || exit 1
    GIT_INDEX_FILE="$directory/check-after.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff-files --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    printf '%s' {proof} >"$directory/checkout.ok" || exit 1
elif [ "$1" = committed ]; then
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@" || exit $?
    [ "$(git symbolic-ref -q HEAD)" = {branch} ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {new} ] || exit 1
    git -c core.fsmonitor=false diff-index --cached --quiet --no-ext-diff --no-textconv {new} -- || exit 1
    GIT_INDEX_FILE="$directory/check-committed.index" git -c core.fsmonitor=false -c core.trustctime=true -c core.ignoreStat=false diff-files --quiet --no-ext-diff --no-textconv --ignore-submodules=none -- || exit 1
    printf '%s' {proof} >"$directory/committed.ok" || exit 1
else
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@"
fi
"#,
        branch = remote::sh_quote(branch),
        old = remote::sh_quote(old),
        new = remote::sh_quote(new),
        index = remote::sh_quote(index),
        proof = remote::sh_quote(proof)
    );
    // The outer command's sole -c selects this wrapper. Remove that inherited parameter
    // before relaying the original hook; preserve the actual profile's remaining config.
    format!(
        "#!/bin/sh\numask 077\nunset GIT_CONFIG_PARAMETERS\nexec env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0={} sh -c {} \"$0\" \"$@\"\n",
        remote::sh_quote(original_dir),
        remote::sh_quote(&payload)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_split_index_dependency_is_resolved_from_the_saved_index() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gitcraft-index-pair-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let result = (|| {
            let old_shared = root.join("old.shared");
            let new_shared = root.join("new.shared");
            let live_index = root.join("index");
            let owned = root.join("evidence");
            std::fs::create_dir(&owned).map_err(|error| error.to_string())?;
            std::fs::write(&old_shared, b"old dependency").map_err(|error| error.to_string())?;
            std::fs::write(&new_shared, b"new dependency").map_err(|error| error.to_string())?;
            // A previous live lookup saw old.shared; an ordinary writer rotated before
            // our main snapshot was captured. Only the saved main index is authoritative.
            let previously_observed = old_shared;
            std::fs::write(&live_index, b"main referring to new.shared")
                .map_err(|error| error.to_string())?;
            let selected = snapshot_pair(
                live_index.to_str().ok_or("Fixture path is not UTF-8")?,
                &owned,
                |saved| {
                    assert_eq!(saved, owned.join("before.index"));
                    assert_eq!(
                        std::fs::read(saved).map_err(|error| error.to_string())?,
                        b"main referring to new.shared"
                    );
                    Ok(Some(new_shared.to_string_lossy().into_owned()))
                },
            )?;
            assert_eq!(selected.as_deref(), new_shared.to_str());
            assert_ne!(selected.as_deref(), previously_observed.to_str());
            assert_eq!(
                std::fs::read(owned.join("before.shared-index"))
                    .map_err(|error| error.to_string())?,
                b"new dependency"
            );
            let refused = root.join("refused");
            std::fs::create_dir(&refused).map_err(|error| error.to_string())?;
            let unresolved = snapshot_pair(
                live_index.to_str().ok_or("Fixture path is not UTF-8")?,
                &refused,
                |_| Err("The dependency named by the saved main index cannot be proven".into()),
            );
            assert!(unresolved.is_err());
            assert!(!refused.join("before.shared-index").exists());
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result
    }

    #[test]
    fn synthetic_capability_versions_refuse_before_user_dispatch() {
        for text in [
            "git version 2.28.0",
            "git version 2.40.1",
            "git version 2.49.9",
            "unknown",
            "git version 2.50.GIT",
        ] {
            let mut dispatched = false;
            if supported_version(text.as_bytes()) {
                dispatched = true;
            }
            assert!(!dispatched);
        }
        assert!(supported_version(b"git version 2.50.0.windows.1\n"));
        assert!(supported_version(b"git version 3.0.0\n"));
        assert_eq!(
            transaction("old", "new"),
            "start\nupdate HEAD new old\nprepare\ncommit\n"
        );
        assert!(!transaction("old", "new").contains("symref-verify"));
        assert!(!transaction("old", "new").contains("HEAD.lock"));
    }

    #[test]
    fn missing_backend_acknowledgement_cannot_qualify_user_dispatch() -> Result<(), String> {
        let directory =
            std::env::temp_dir().join(format!("gitcraft-ack-recording-{}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let mut dispatched = false;
        if ack(&directory, "probe-prepared", b"probe\n")
            && ack(&directory, "probe-committed", b"probe\n")
        {
            dispatched = true;
        }
        assert!(!dispatched); // a backend that ignored the hook cannot pass
        std::fs::write(directory.join("probe-prepared.ok"), b"probe\n")
            .map_err(|e| e.to_string())?;
        assert!(!ack(&directory, "probe-committed", b"probe\n"));
        let _ = std::fs::remove_dir_all(directory);
        Ok(())
    }

    #[test]
    fn synthetic_uncertain_lifecycle_always_retains_owned_evidence() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        for state in [
            "queued",
            "timeout-before-prepared",
            "timeout-before-checkout",
            "timeout-after-checkout",
        ] {
            let directory = std::env::temp_dir().join(format!(
                "gitcraft-owned-evidence-{}-{stamp}-{state}",
                std::process::id()
            ));
            std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
            if state == "timeout-after-checkout" {
                std::fs::write(directory.join("checkout-attempted"), b"owned")
                    .map_err(|e| e.to_string())?;
            }
            let hook = Hook {
                directory: directory.clone(),
                keep: true,
                old: "1".repeat(40),
                new: "2".repeat(40),
                proof: "owned".into(),
                capability_acknowledged: false,
            };
            drop(hook);
            assert!(
                directory.exists(),
                "An uncertain {state} must never trigger cleanup"
            );
            std::fs::remove_dir_all(directory).map_err(|e| e.to_string())?;
        }
        // Actual production constructors refuse a remote location before even probing Git.
        for state in [
            "queued",
            "before-prepared",
            "marker-observed",
            "after-checkout",
        ] {
            let refused = Hook::new(
                Path::new("ssh://fixture/home/repository"),
                "refs/heads/main",
                "old",
                "new",
            )
            .err();
            assert!(
                refused.is_some_and(|error| error.contains("no remote repair was dispatched")),
                "{state}"
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn recording_hook_changes_are_refused_before_checkout_and_keep_external_bytes()
    -> Result<(), String> {
        recording_scenarios(
            &[
                "unsupported-hook-command",
                "veto",
                "head-other",
                "detached",
                "index-changed",
                "work-changed",
                "unchanged",
            ],
            false,
        )
    }

    #[cfg(unix)]
    #[test]
    fn recording_pre_hook_identity_guards_prevent_relay_and_checkout() -> Result<(), String> {
        recording_scenarios(
            &[
                "wrong-target",
                "wrong-old",
                "wrong-new",
                "duplicate-input",
                "extra-reference",
                "extra-field",
                "empty-input",
                "pre-head-other",
                "pre-head-detached",
                "pre-head-oid-changed",
            ],
            true,
        )
    }

    #[cfg(unix)]
    fn recording_scenarios(scenarios: &[&str], guards: bool) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        use std::time::Duration;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gitcraft-hook-recording-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let result = (|| {
            let bin = root.join("bin");
            std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
            let fake = bin.join("git");
            std::fs::write(&fake, r#"#!/bin/sh
while :; do case "$1" in -c) shift 2 ;; --literal-pathspecs) shift ;; *) break ;; esac; done
printf '%s\n' "$*" >>"$REC_DIRECTORY/commands"
case "$1" in
hook)
    input=
    for arg in "$@"; do case "$arg" in --to-stdin=*) input=${arg#--to-stdin=} ;; esac; phase=$arg; done
    cat "$input" >"$REC_DIRECTORY/original-input-$phase"
    [ "$REC_SCENARIO" != unsupported-hook-command ] || exit 92
    [ "$REC_SCENARIO" != veto ] || exit 1
    if [ "$phase" = prepared ]; then
        case "$REC_SCENARIO" in
        head-other) printf '%s\n' refs/heads/other >"$REC_DIRECTORY/head" ;;
        detached) : >"$REC_DIRECTORY/head" ;;
        index-changed) printf 'external-index' >"$REC_DIRECTORY/index" ;;
        work-changed) printf 'external-work' >"$REC_DIRECTORY/work" ;;
        esac
    fi
    ;;
symbolic-ref) [ -s "$REC_DIRECTORY/head" ] || exit 1; cat "$REC_DIRECTORY/head" ;;
rev-parse) cat "$REC_DIRECTORY/oid" ;;
diff-files) [ "$REC_SCENARIO" != work-changed ] ;;
diff-index|status) exit 0 ;;
read-tree) printf 'checkout\n' >>"$REC_DIRECTORY/checkout-record" ;;
*) exit 91 ;;
esac
"#).map_err(|e|e.to_string())?;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let old = "1".repeat(40);
            let new = "2".repeat(40);
            let proof = format!("refs/heads/main\n{old}\n{new}\n");
            let correct = format!("{old} {new} refs/heads/main\n");
            for &scenario in scenarios {
                let updates = match scenario {
                    "wrong-target" => format!("{old} {new} refs/heads/other\n"),
                    "wrong-old" => format!("{} {new} refs/heads/main\n", "3".repeat(40)),
                    "wrong-new" => format!("{old} {} refs/heads/main\n", "3".repeat(40)),
                    "duplicate-input" => format!("{correct}{correct}"),
                    "extra-reference" => format!("{correct}{old} {new} refs/heads/other\n"),
                    "extra-field" => format!("{old} {new} refs/heads/main unexpected\n"),
                    "empty-input" => String::new(),
                    _ => correct.clone(),
                };
                let directory = root.join(scenario);
                std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
                for (name, value) in [
                    ("mode", "repair"),
                    ("index", "original-index"),
                    ("before.index", "original-index"),
                    ("work", "original-work"),
                    ("head", "refs/heads/main\n"),
                    ("oid", old.as_str()),
                ] {
                    std::fs::write(directory.join(name), value).map_err(|e| e.to_string())?;
                }
                match scenario {
                    "pre-head-other" => {
                        std::fs::write(directory.join("head"), b"refs/heads/other\n")
                    }
                    "pre-head-detached" => std::fs::write(directory.join("head"), b""),
                    "pre-head-oid-changed" => std::fs::write(directory.join("oid"), "3".repeat(40)),
                    _ => Ok(()),
                }
                .map_err(|e| e.to_string())?;
                let head_before =
                    std::fs::read(directory.join("head")).map_err(|e| e.to_string())?;
                let oid_before = std::fs::read(directory.join("oid")).map_err(|e| e.to_string())?;
                let wrapper = directory.join("reference-transaction");
                std::fs::write(
                    &wrapper,
                    script(HookScript {
                        original: "/owned/original/reference-transaction",
                        branch: "refs/heads/main",
                        old: &old,
                        new: &new,
                        index: &directory.join("index").to_string_lossy(),
                        shared: None,
                        affected: &["file.txt".into()],
                        proof: &proof,
                        needs_checkout: true,
                    }),
                )
                .map_err(|e| e.to_string())?;
                let run = |phase: &str| -> Result<std::process::Output, String> {
                    let mut command = Command::new("sh");
                    command
                        .env_clear()
                        .arg(&wrapper)
                        .arg(phase)
                        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                        .env("REC_SCENARIO", scenario)
                        .env("REC_DIRECTORY", &directory);
                    crate::bounded_process::run_input(
                        command,
                        Some(updates.as_bytes().to_vec()),
                        &|| true,
                        Duration::from_secs(5),
                        1024 * 1024,
                        1024 * 1024,
                    )
                };
                let output = run("prepared")?;
                assert_eq!(
                    output.status.success(),
                    scenario == "unchanged",
                    "{scenario}"
                );
                assert_eq!(
                    directory.join("checkout-record").exists(),
                    scenario == "unchanged",
                    "{scenario}"
                );
                assert_eq!(
                    std::fs::read_to_string(directory.join("index")).map_err(|e| e.to_string())?,
                    if scenario == "index-changed" {
                        "external-index"
                    } else {
                        "original-index"
                    }
                );
                assert_eq!(
                    std::fs::read_to_string(directory.join("work")).map_err(|e| e.to_string())?,
                    if scenario == "work-changed" {
                        "external-work"
                    } else {
                        "original-work"
                    }
                );
                if guards {
                    assert!(
                        !directory.join("original-input-prepared").exists(),
                        "{scenario}: original relay"
                    );
                    assert!(
                        !directory.join("prepared.ok").exists(),
                        "{scenario}: prepared ACK"
                    );
                    assert!(
                        !directory.join("checkout-attempted").exists(),
                        "{scenario}: checkout"
                    );
                    assert_eq!(
                        std::fs::read(directory.join("head")).map_err(|e| e.to_string())?,
                        head_before
                    );
                    assert_eq!(
                        std::fs::read(directory.join("oid")).map_err(|e| e.to_string())?,
                        oid_before
                    );
                    let commands =
                        std::fs::read_to_string(directory.join("commands")).unwrap_or_default();
                    assert!(!commands.lines().any(|line| line.starts_with("hook ") || line.starts_with("read-tree ")), "{scenario}: {commands}");
                    println!(
                        "guard {scenario}: refused; original_prepared=0 checkout=0; external HEAD/index/work preserved"
                    );
                } else {
                    assert_eq!(
                        std::fs::read_to_string(directory.join("original-input-prepared"))
                            .map_err(|e| e.to_string())?,
                        updates
                    );
                }
                if scenario == "unchanged" {
                    assert!(
                        ack(&directory, "prepared", proof.as_bytes())
                            && ack(&directory, "checkout", proof.as_bytes())
                    );
                    std::fs::write(directory.join("oid"), &new).map_err(|e| e.to_string())?;
                    assert!(run("committed")?.status.success());
                    assert!(ack(&directory, "committed", proof.as_bytes()));
                } else {
                    assert!(!ack(&directory, "checkout", proof.as_bytes()));
                }
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result
    }
}

#[cfg(all(test, unix))]
mod owned_fixtures;
