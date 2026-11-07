//! Advanced repository views. All commands run on the repository machine through shared Git.
pub(crate) mod all_refs;
pub(crate) mod object;

/// Read plumbing against the selected repository, independent of inherited Git redirection.
pub(super) fn read(
    repo: &std::path::Path,
    args: &[&str],
    input: Vec<u8>,
    cap: usize,
) -> Result<Vec<u8>, String> {
    use crate::git::command;
    const UNSET: &[&str] = &[
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_REPLACE_REF_BASE",
        "GIT_SHALLOW_FILE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ];
    if let Some(target) = crate::remote::parse(repo) {
        let mut request = command::bounded::request(&target, args, input, cap);
        request.unset.extend(UNSET.iter().map(|s| (*s).into()));
        let out = crate::remote::exec(&target.host, &request)?;
        if out.code != 0 {
            return Err(String::from_utf8_lossy(&out.stderr).trim().into());
        }
        if out.stdout.len() > cap {
            return Err("Repository metadata exceeds the inspection limit".into());
        }
        return Ok(out.stdout);
    }
    let mut cmd = command::local_command(Some(repo), &command::Extra::default(), args);
    for key in UNSET {
        cmd.env_remove(key);
    }
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            input: Some(input),
            output_limit: cap,
            stderr_limit: Some(65536),
            timeout: Some(std::time::Duration::from_secs(120)),
            background: true,
            ..Default::default()
        },
    )?
    .complete()?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(out.stdout)
}
pub(crate) mod guard;
pub(crate) mod reftable;
pub(crate) mod remote_config;
pub(crate) mod replacements;
