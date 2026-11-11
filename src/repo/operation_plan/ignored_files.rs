use super::*;

/// Keep pull's authentication, submodule and configuration behavior while making its
/// merge refuse to overwrite ignored files. Read the effective options in the worker,
/// after any preceding checkout, and override them only for this command.
pub(in crate::repo) fn ignored_safe_pull(
    path: &Path,
    args: &[String],
) -> Result<Vec<String>, String> {
    checked_pull_with(
        args,
        || crate::repo::partial_stash::validate_excluded_cleanup(path),
        |args, codes| git::run_ok_codes(Some(path), args, codes),
    )
}

pub(in crate::repo) fn checked_pull_with(
    args: &[String],
    cleanup: impl FnOnce() -> Result<(), String>,
    run: impl FnMut(&[&str], &[i32]) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    cleanup()?;
    ignored_safe_pull_with(args, run)
}

pub(in crate::repo) fn ignored_safe_pull_with(
    args: &[String],
    mut run: impl FnMut(&[&str], &[i32]) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let mode = pull_mode(args)?;
    let recurse = match mode.recurse {
        Some(value) => value,
        None => match run(&["config", "--bool", "--get", "submodule.recurse"], &[0, 1])?.trim() {
            "true" => true,
            "false" | "" => false,
            _ => return Err("Cannot determine safe pull submodule recursion".into()),
        },
    };
    if recurse {
        check_ignored_submodules_with(&mut run)?;
    }
    let branch = run(&["symbolic-ref", "-q", "HEAD"], &[0])?;
    let branch = branch
        .trim()
        .strip_prefix("refs/heads/")
        .ok_or("Pull requires a checked-out local branch")?;
    let configured_ff_only = if !mode.ff_explicit && mode.rebase.is_none() {
        run(&["config", "-z", "--get", "pull.ff"], &[0, 1])?.strip_suffix('\0') == Some("only")
    } else {
        false
    };
    if !mode.ff_only && !configured_ff_only {
        let rebase = match mode.rebase {
            Some(value) => value,
            None => {
                let local_key = format!("branch.{branch}.rebase");
                let local = run(&["config", "-z", "--get", &local_key], &[0, 1])?;
                let (key, value) = if local.is_empty() {
                    (
                        "pull.rebase",
                        run(&["config", "-z", "--get", "pull.rebase"], &[0, 1])?,
                    )
                } else {
                    (local_key.as_str(), local)
                };
                // A valueless boolean means true, whereas an explicitly empty value
                // means false. Git's typed reader preserves that distinction.
                if !value.is_empty() && value.trim_end_matches('\0').is_empty() {
                    run(&["config", "--bool", "--get", key], &[0])?.trim() == "true"
                } else {
                    !matches!(
                        value
                            .trim_end_matches('\0')
                            .trim()
                            .to_ascii_lowercase()
                            .as_str(),
                        "" | "false" | "no" | "off" | "0"
                    )
                }
            }
        };
        // Rebase's checkout does not honor mergeOptions and offers no equivalent
        // no-overwrite-ignore switch. Refuse before fetch can introduce a collision.
        if rebase
            && !run(
                &[
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "--directory",
                    "-z",
                ],
                &[0],
            )?
            .is_empty()
        {
            return Err("Rebase pull cannot safely preserve ignored local files. Move them out of the repository or use fast-forward-only or merge pull.".into());
        }
    }
    let key = format!("branch.{branch}.mergeOptions");
    let old = run(&["config", "-z", "--get", &key], &[0, 1])?;
    let old = old.strip_suffix('\0').unwrap_or(&old);
    Ok([
        vec!["-c".into(), format!("{key}={old} --no-overwrite-ignore")],
        args.to_vec(),
    ]
    .concat())
}

/// A child checkout/rebase does not honor the superproject's mergeOptions.
/// foreach visits populated submodules recursively on the repository's own machine.
pub(in crate::repo) fn check_ignored_submodules_with(
    mut run: impl FnMut(&[&str], &[i32]) -> Result<String, String>,
) -> Result<(), String> {
    let ignored = run(
        &[
            "submodule",
            "foreach",
            "--quiet",
            "--recursive",
            "git ls-files --others --ignored --exclude-standard --directory -z",
        ],
        &[0],
    )?;
    if ignored.is_empty() {
        Ok(())
    } else {
        Err("Submodule update cannot safely preserve ignored local files. Move them out of populated submodules and run the operation again.".into())
    }
}

/// Resolve precisely the execution pathspecs to indexed gitlinks. foreach itself
/// has no pathspec interface, so quote the resolved paths as literal shell data.
pub(in crate::repo) fn check_ignored_submodule_update_with(
    args: &[String],
    mut run: impl FnMut(&[&str], &[i32]) -> Result<String, String>,
) -> Result<(), String> {
    // Scope only the canonical, value-free update envelope emitted by the app.
    // In other forms an option such as --reference can consume a token spelled
    // "--". Inspect every populated submodule rather than guessing its scope.
    let mut separator = None;
    for (index, arg) in args.iter().enumerate().skip(2) {
        match arg.as_str() {
            "--" => {
                separator = Some(index);
                break;
            }
            "--init" | "--recursive" => {}
            _ => return check_ignored_submodules_with(run),
        }
    }
    let Some(separator) = separator else {
        return check_ignored_submodules_with(run);
    };
    if separator + 1 == args.len() {
        return check_ignored_submodules_with(run);
    }
    let mut query = vec!["ls-files", "--stage", "-z", "--"];
    query.extend(args[separator + 1..].iter().map(String::as_str));
    let entries = run(&query, &[0])?;
    let mut paths = Vec::new();
    for entry in entries.split('\0').filter(|entry| !entry.is_empty()) {
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or("Cannot resolve submodule update scope safely")?;
        let metadata: Vec<_> = metadata.split_whitespace().collect();
        if metadata.len() != 3 {
            return Err("Cannot resolve submodule update scope safely".into());
        }
        if metadata[0] == "160000" {
            if metadata[2] != "0" || path.is_empty() {
                return Err("Resolve conflicted submodules before updating".into());
            }
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Ok(());
    } // Git diagnoses unmatched execution pathspecs.
    let recursive = args[..separator].iter().any(|arg| arg == "--recursive");
    let mut patterns = Vec::new();
    for path in paths {
        let literal = crate::remote::sh_quote(path);
        patterns.push(literal.clone());
        if recursive {
            patterns.push(format!("{literal}/*"));
        }
    }
    let script = format!(
        "case \"$displaypath\" in {}) git ls-files --others --ignored --exclude-standard --directory -z ;; *) : ;; esac",
        patterns.join("|")
    );
    if script.len() > 48 * 1024 {
        return Err("Too many selected submodules to inspect safely".into());
    }
    let ignored = run(
        &["submodule", "foreach", "--quiet", "--recursive", &script],
        &[0],
    )?;
    if ignored.is_empty() {
        Ok(())
    } else {
        Err("Submodule update cannot safely preserve ignored local files. Move them out of selected submodules and run the operation again.".into())
    }
}
