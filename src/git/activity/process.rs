use std::path::Path;
pub(super) fn run(
    repo: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if !active() {
        return Err("Analysis cancelled".into());
    }
    let mut safe = vec![
        "--no-replace-objects",
        "--no-lazy-fetch",
        "-c",
        "core.quotePath=false",
        "-c",
        "core.hooksPath=",
        "-c",
        "log.showSignature=false",
    ];
    safe.extend_from_slice(args);
    let output = super::super::command::bounded::run(
        repo,
        &safe,
        input.unwrap_or_default().to_vec(),
        16 * 1024 * 1024,
    )?;
    if !active() {
        return Err("Analysis cancelled".into());
    }
    Ok(output)
}
pub(super) fn config(
    repo: &Path,
    key: &str,
    path: bool,
    active: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    let mut args = vec!["config", "--default=", "--get"];
    if path {
        args.push("--path");
    }
    args.push(key);
    let data = run(repo, &args, None, active)?;
    let value = String::from_utf8(data).map_err(|_| "Mailmap configuration is not UTF-8")?;
    let value = value.trim_end_matches(['\r', '\n']);
    Ok((!value.is_empty()).then(|| value.to_owned()))
}
