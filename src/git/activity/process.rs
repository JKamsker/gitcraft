use std::path::Path;
pub(in crate::git) fn run(
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
    let input = input.unwrap_or_default().to_vec();
    let (output, errors, success) = if let Some(target) = crate::remote::parse(repo) {
        let request =
            super::super::command::bounded::request(&target, &safe, input, 16 * 1024 * 1024);
        let result = crate::remote::exec(&target.host, &request)?;
        (result.stdout, result.stderr, result.code == 0)
    } else {
        let command = super::super::command::local_command(
            Some(repo),
            &super::super::command::Extra::default(),
            &safe,
        );
        let result = crate::processes::run(
            command,
            crate::processes::Options {
                input: Some(input),
                background: true,
                timeout: Some(std::time::Duration::from_secs(120)),
                output_limit: 16 * 1024 * 1024,
                stderr_limit: Some(64 * 1024),
                ..Default::default()
            },
        )?
        .complete()?;
        (result.stdout, result.stderr, result.status.success())
    };
    let error = String::from_utf8_lossy(&errors);
    if !success
        || error
            .lines()
            .any(|line| line.starts_with("error:") || line.starts_with("fatal:"))
    {
        return Err(if error.trim().is_empty() {
            "Git history read failed".into()
        } else {
            error.trim().to_owned()
        });
    }
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
