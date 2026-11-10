//! Credential-free local reads and endpoint configuration capture.
use super::*;

pub(super) fn read(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let args: Vec<&str> = ["--no-lazy-fetch", "--no-replace-objects"]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    git::forecast_run(repo, &args, &[("GIT_NO_LAZY_FETCH", "1".into())], &[0], active)
        .map_err(|_| "The local push preview could not be completed. Fetch missing history explicitly, then try again; no push was run.".into())
}

pub(super) fn optional(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let args: Vec<&str> = ["--no-lazy-fetch", "--no-replace-objects"]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    git::forecast_run(
        repo,
        &args,
        &[("GIT_NO_LAZY_FETCH", "1".into())],
        &[0, 1],
        active,
    )
    .map_err(|_| "The local push configuration could not be read; no push was run.".into())
}

pub(super) fn valid_ref(repo: &Path, name: &str, active: &dyn Fn() -> bool) -> Result<(), String> {
    if !name.starts_with("refs/") || read(repo, &["check-ref-format", name], active).is_err() {
        return Err("Choose one complete destination ref, such as refs/heads/main.".into());
    }
    Ok(())
}

pub(super) fn config(repo: &Path, key: &str, active: &dyn Fn() -> bool) -> Result<String, String> {
    optional(repo, &["config", "--get", key], active).map(|s| s.trim_end().to_owned())
}

pub(super) fn config_stamp(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    // Deliberately excludes credential helpers, HTTP headers and token stores. Signing
    // identifiers/paths are compared, never opened. The private plan is not Debug/Serialize.
    let values = optional(
        repo,
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(remote\..*\.(url|pushurl|fetch|push|mirror|receivepack)|branch\..*\.(remote|merge|pushremote|gitcraftpushbranch)|remote\.pushdefault|push\.(default|followtags|recursesubmodules)|url\..*\.(insteadof|pushinsteadof)|user\.(name|email|signingkey)|commit\.gpgsign|gpg\.(format|program)|gpg\..*\.program|core\.(sshcommand|hookspath)|ssh\.variant|lfs\.(url|pushurl|storage)|remote\..*\.(lfsurl|lfspushurl))$",
        ],
        active,
    )?;
    if values.len() > 256 * 1024 {
        return Err("The push routing configuration exceeds the preview's size limit.".into());
    }
    for item in values.split('\0').filter(|s| !s.is_empty()) {
        let (key, value) = item.split_once('\n').unwrap_or((item, ""));
        if key.chars().any(char::is_control) || value.chars().any(char::is_control) {
            return Err("An identity or routing value contains control characters. Correct it before opening a push preview.".into());
        }
    }
    Ok(values)
}

pub(super) fn clean_lfs_config(
    repo: &Path,
    head: &str,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let untracked = read(repo, &["ls-files", "--others", "--", ".lfsconfig"], active)?;
    let flags = read(repo, &["ls-files", "-v", "--", ".lfsconfig"], active)?;
    if !untracked.is_empty() || flags.lines().any(|l| !l.starts_with("H ")) {
        return Err("Commit or remove .lfsconfig changes, and clear skip/assume-unchanged flags on it before reviewing a push.".into());
    }
    let filter = read(repo, &["check-attr", "filter", "--", ".lfsconfig"], active)?;
    if !filter.trim_end().ends_with(": unspecified") && !filter.trim_end().ends_with(": unset") {
        return Err(".lfsconfig must not pass through a content filter when reviewing endpoint configuration.".into());
    }
    let change = read(
        repo,
        &[
            "-c",
            "core.fsmonitor=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--raw",
            "--no-renames",
            head,
            "--",
            ".lfsconfig",
        ],
        active,
    )?;
    if !change.is_empty() {
        return Err("The working .lfsconfig differs from the inspected commit. Commit or stash that routing change before pushing.".into());
    }
    Ok(())
}

pub(super) fn endpoints(
    repo: &Path,
    checkout_oid: &str,
    remote: &str,
    active: &dyn Fn() -> bool,
) -> Result<(String, Vec<(String, String)>), String> {
    clean_lfs_config(repo, checkout_oid, active)?;
    let lfs_file = read(
        repo,
        &["ls-tree", "-z", checkout_oid, "--", ".lfsconfig"],
        active,
    )?;
    if !lfs_file.is_empty() && !lfs_file.starts_with("100644 ") && !lfs_file.starts_with("100755 ")
    {
        return Err(
            "Committed .lfsconfig must be a regular file before its endpoint can be reviewed."
                .into(),
        );
    }
    let receive_pack = config(repo, &format!("remote.{remote}.receivepack"), active)?;
    let mut lfs_config = Vec::new();
    for key in [
        format!("remote.{remote}.lfsurl"),
        format!("remote.{remote}.lfspushurl"),
        "lfs.url".into(),
        "lfs.pushurl".into(),
    ] {
        let mut value = config(repo, &key, active)?;
        if value.is_empty() && !lfs_file.is_empty() && key.starts_with("lfs.") {
            value = optional(
                repo,
                &[
                    "config",
                    "--blob",
                    &format!("{checkout_oid}:.lfsconfig"),
                    "--get",
                    &key,
                ],
                active,
            )?
            .trim_end()
            .to_owned();
        }
        if value.chars().any(char::is_control) {
            return Err("The LFS endpoint contains control characters.".into());
        }
        // Empty higher-priority values freeze the default too: a later working-tree
        // .lfsconfig edit must not redirect an already approved upload.
        lfs_config.push((key, value));
    }
    Ok((receive_pack, lfs_config))
}
