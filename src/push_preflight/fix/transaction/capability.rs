//! Native backend capability proof in an owned scratch repository.
use super::*;

pub(super) fn prove(
    directory: &Path,
    repo: &Path,
    refs: &str,
    objects: &str,
) -> Result<(), String> {
    let probe = directory.join("capability.git");
    let format = format!("--object-format={objects}");
    let refs = format!("--ref-format={refs}");
    // Every subprocess uses the existing owned bounded runner. If completion is
    // uncertain even for a scratch request, retain its directory rather than racing it.
    owned(
        repo,
        directory,
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
    let blob = directory.join("empty-tree");
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
    std::fs::write(directory.join("mode"), b"probe")
        .map_err(|_| "Could not arm capability proof")?;
    let hooks = format!("core.hooksPath={}", remote::git_path(directory));
    let command_probe = directory.join("pre-push");
    std::fs::write(&command_probe, "#!/bin/sh\ndirectory=$(dirname \"$0\")\ncat >\"$directory/hook-command.stdin\" || exit 1\nprintf 'probe\\n' >\"$directory/hook-command.ok\"\n")
            .map_err(|_| "Could not write owned hook-command proof")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&command_probe, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect owned capability hook")?;
    }
    let hook_input = directory.join("hook-input");
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
            "pre-push",
        ],
        b"",
        &[],
    )?;
    if !ack(directory, "hook-command", b"probe\n")
        || std::fs::read(directory.join("hook-command.stdin"))
            .ok()
            .as_deref()
            != Some(&b"owned capability input\n"[..])
    {
        return Err(
            "Git did not prove original-hook command/stdin support; no user update was launched"
                .into(),
        );
    }
    let result = owned(
        repo,
        &probe,
        &["-c", &hooks, "update-ref", "--stdin"],
        transaction(old.trim(), new.trim()).as_bytes(),
        &[],
    );
    if result.is_err()
        || !ack(directory, "probe-prepared", b"probe\n")
        || !ack(directory, "probe-committed", b"probe\n")
    {
        return Err("This Git/backend did not prove private hook selection and an atomic current-HEAD/branch update. No user reference update was launched.".into());
    }
    let expected_update = format!("{} {} {branch}\n", old.trim(), new.trim());
    let recorded = std::fs::read(directory.join("probe-updates-prepared"))
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
        return Err(
            "The native HEAD/branch transaction result was not proven; no user update was launched"
                .into(),
        );
    }
    Ok(())
}
