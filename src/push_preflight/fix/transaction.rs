//! Checkout during Git's prepared reference transaction, while Git owns the target lock.
//! No PID discovery, signals, or independent process controller is introduced here.
use std::path::{Path, PathBuf};

use crate::{git, remote};

pub(super) struct Hook {
    directory: PathBuf,
    keep: bool,
}

impl Hook {
    pub(super) fn new(repo: &Path, branch: &str, old: &str, new: &str) -> Result<Self, String> {
        let original = git::forecast_run(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "hooks/reference-transaction",
            ],
            &[],
            &[0],
            &|| true,
        )?;
        let wrapper = script(original.trim_end(), branch, old, new);
        if let Some(target) = remote::parse(repo) {
            let output = git::forecast_remote(&target.host, remote::Request {
                script: "umask 077; d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-push-ref-transaction.XXXXXXXX\") || exit 1; cat >\"$d/reference-transaction\" && chmod 700 \"$d/reference-transaction\" || exit 1; printf '%s' \"$d\"".into(),
                stdin: Some(wrapper.into_bytes()),
                ..Default::default()
            }, &|| true)?;
            let path = String::from_utf8(output.stdout).map_err(|_| "Invalid repair hook path")?;
            if output.code != 0
                || !path.starts_with('/')
                || path.contains(['\n', '\r', '\0'])
                || !path
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("gitcraft-push-ref-transaction."))
            {
                return Err("Could not prepare the reference transaction hook".into());
            }
            return Ok(Self {
                directory: remote::encode(&target.host, &path),
                keep: false,
            });
        }
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
            .map_err(|_| "Could not prepare the reference transaction hook")?;
        let hook = Self {
            directory,
            keep: false,
        };
        let path = hook.directory.join("reference-transaction");
        std::fs::write(&path, wrapper)
            .map_err(|_| "Could not write the reference transaction hook")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not protect the reference transaction hook")?;
        }
        Ok(hook)
    }

    pub(super) fn environment(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_CONFIG_COUNT", "1".into()),
            ("GIT_CONFIG_KEY_0", "core.hooksPath".into()),
            ("GIT_CONFIG_VALUE_0", remote::git_path(&self.directory)),
        ]
    }

    /// Preserve owned evidence after any checkout attempt. Never restore over edits made
    /// concurrently; an interrupted transaction needs an explicit recovery decision.
    pub(super) fn recovery(&mut self) -> Option<String> {
        let attempted = if let Some(target) = remote::parse(&self.directory) {
            git::forecast_remote(
                &target.host,
                remote::Request {
                    script: format!(
                        "test -f {}/checkout-attempted",
                        remote::sh_path(&target.path)
                    ),
                    ..Default::default()
                },
                &|| true,
            )
            .map(|output| output.code != 1)
            .unwrap_or(true)
        } else {
            self.directory
                .join("checkout-attempted")
                .try_exists()
                .unwrap_or(true)
        };
        self.keep = attempted;
        attempted.then(|| remote::git_path(&self.directory))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        if let Some(target) = remote::parse(&self.directory) {
            crate::connections::spawn(move || {
                let _ = git::forecast_remote(
                    &target.host,
                    remote::Request {
                        script: format!("rm -rf -- {}", remote::sh_path(&target.path)),
                        ..Default::default()
                    },
                    &|| true,
                );
            });
        } else {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

fn script(original: &str, branch: &str, old: &str, new: &str) -> String {
    let original_dir = original
        .rsplit_once('/')
        .map_or(original, |(directory, _)| directory);
    let recovery = format!(
        "Push repair recovery evidence\nReference: {branch}\nOriginal object: {old}\nRewritten object: {new}\n\nA checkout was attempted while Git held the reference transaction lock.\nInspect HEAD, git status, git diff and git diff --cached before recovery.\nThe original index is saved as before.index; do not copy it over newer work.\nSave any concurrent edits before deliberately restoring the original object.\nAfter preserving new work, git reset --merge {old} is a possible recovery.\nGit may also have left a lock after interruption; do not remove another process's lock.\n"
    );
    let payload = format!(
        r#"case "$1" in prepared|committed|aborted) ;; *) exit 1 ;; esac
directory=$(dirname "$0")
updates="$directory/updates-$1"
cat >"$updates" || exit 1
if [ "$1" = prepared ]; then
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@" || exit $?
    matched=0
    while read -r before after reference; do
        if [ "$reference" = {branch} ]; then
            [ "$before" = {old} ] && [ "$after" = {new} ] || exit 1
            matched=1
        fi
    done <"$updates"
    [ "$matched" = 1 ] || exit 1
    index=$(git rev-parse --path-format=absolute --git-path index) || exit 1
    cp -- "$index" "$directory/before.index" || exit 1
    printf '%s' {recovery} >"$directory/recovery.txt" || exit 1
    printf '%s\n' attempted >"$directory/checkout-attempted" || exit 1
    git read-tree -u -m {old} {new} || {{ printf '%s\n' checkout-failed >"$directory/phase"; exit 1; }}
    printf '%s\n' prepared >"$directory/phase"
else
    printf '%s\n' "$1" >"$directory/phase"
    git hook run --ignore-missing --to-stdin="$updates" reference-transaction -- "$@"
fi
"#,
        branch = remote::sh_quote(branch),
        old = remote::sh_quote(old),
        new = remote::sh_quote(new),
        recovery = remote::sh_quote(&recovery)
    );
    // As with the pre-push relay, nested Git calls and the original hook retain the
    // original hooksPath. The outer update-ref alone selects this private wrapper.
    format!(
        "#!/bin/sh\numask 077\nn=${{GIT_CONFIG_COUNT:-0}}\nexec env \"GIT_CONFIG_KEY_${{n}}=core.hooksPath\" \"GIT_CONFIG_VALUE_${{n}}=\"{} \"GIT_CONFIG_COUNT=$((n+1))\" sh -c {} \"$0\" \"$@\"\n",
        remote::sh_quote(original_dir),
        remote::sh_quote(&payload)
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    /// Recording-only protocol fixture: Git is a supplied recorder, no real repository,
    /// ref transaction, process snapshot search, signal or network operation is used.
    #[test]
    fn prepared_hook_recordings_gate_checkout_and_preserve_recovery_evidence() -> Result<(), String>
    {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gitcraft-ref-recording-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let result = (|| {
            let bin = root.join("bin");
            std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
            let index = root.join("recorded-index");
            std::fs::write(&index, "original-index").map_err(|e| e.to_string())?;
            let recorder = bin.join("git");
            std::fs::write(&recorder, r#"#!/bin/sh
printf '%s\n' "$*" >>"$RECORDING_LOG"
case "$1" in
    hook)
        input=
        for arg in "$@"; do case "$arg" in --to-stdin=*) input=${arg#--to-stdin=} ;; esac; phase=$arg; done
        "$RECORDING_ORIGINAL" "$phase" <"$input"
        ;;
    rev-parse) printf '%s\n' "$RECORDING_INDEX" ;;
    read-tree) [ "$RECORDING_FAIL_CHECKOUT" != yes ] ;;
    *) exit 91 ;;
esac
"#).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&recorder, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let original = root.join("original-hook");
            std::fs::write(&original, "#!/bin/sh\nprintf '%s\\n' \"original:$1\" >>\"$RECORDING_LOG\"\ncat >\"$RECORDING_ORIGINAL_INPUT\"\n[ \"$RECORDING_VETO\" != yes ]\n").map_err(|e|e.to_string())?;
            std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let old = "1".repeat(40);
            let new = "2".repeat(40);
            let updates = format!("{old} {new} refs/heads/main\n");
            for (mode, veto, fail, should_attempt) in [
                ("veto", "yes", "no", false),
                ("checkout-error", "no", "yes", true),
                ("prepared", "no", "no", true),
                ("commit-failed", "no", "no", true),
            ] {
                let directory = root.join(mode);
                std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
                let wrapper = directory.join("reference-transaction");
                std::fs::write(
                    &wrapper,
                    script(&original.to_string_lossy(), "refs/heads/main", &old, &new),
                )
                .map_err(|e| e.to_string())?;
                let log = directory.join("commands");
                let input = directory.join("original-input");
                let run_phase = |phase: &str| -> Result<std::process::Output, String> {
                    let mut command = Command::new("sh");
                    command
                        .arg(&wrapper)
                        .arg(phase)
                        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                        .env("GIT_CONFIG_COUNT", "0")
                        .env("RECORDING_ORIGINAL", &original)
                        .env("RECORDING_LOG", &log)
                        .env("RECORDING_INDEX", &index)
                        .env("RECORDING_ORIGINAL_INPUT", &input)
                        .env("RECORDING_VETO", veto)
                        .env("RECORDING_FAIL_CHECKOUT", fail)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    let mut child = command.spawn().map_err(|e| e.to_string())?;
                    child
                        .stdin
                        .take()
                        .ok_or("No recording input")?
                        .write_all(updates.as_bytes())
                        .map_err(|e| e.to_string())?;
                    child.wait_with_output().map_err(|e| e.to_string())
                };
                let output = run_phase("prepared")?;
                assert_eq!(
                    output.status.success(),
                    matches!(mode, "prepared" | "commit-failed")
                );
                let final_phase = if mode == "prepared" {
                    "committed"
                } else {
                    "aborted"
                };
                let _ = run_phase(final_phase)?;
                assert_eq!(
                    std::fs::read_to_string(directory.join("phase"))
                        .map_err(|e| e.to_string())?
                        .trim(),
                    final_phase
                );
                let commands = std::fs::read_to_string(&log).map_err(|e| e.to_string())?;
                assert!(commands.lines().any(|line| line == "original:prepared"));
                assert!(
                    commands.find("original:prepared").unwrap_or(usize::MAX)
                        < commands.find("rev-parse").unwrap_or(usize::MAX)
                        || !should_attempt
                );
                assert_eq!(
                    commands
                        .lines()
                        .filter(|line| line.starts_with("read-tree "))
                        .count(),
                    usize::from(should_attempt)
                );
                assert!(commands.contains(&format!("original:{final_phase}")));
                assert_eq!(
                    directory.join("checkout-attempted").exists(),
                    should_attempt
                );
                assert_eq!(
                    std::fs::read_to_string(&input).map_err(|e| e.to_string())?,
                    updates
                );
                if should_attempt {
                    assert_eq!(
                        std::fs::read_to_string(directory.join("before.index"))
                            .map_err(|e| e.to_string())?,
                        "original-index"
                    );
                    assert!(
                        std::fs::read_to_string(directory.join("recovery.txt"))
                            .map_err(|e| e.to_string())?
                            .contains(&old)
                    );
                }
                assert_eq!(
                    std::fs::read_to_string(&index).map_err(|e| e.to_string())?,
                    "original-index"
                );
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result
    }
}
