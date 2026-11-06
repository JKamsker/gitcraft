//! Running a project's own tool (`cargo`, `npm`) in the repository, on whichever machine it is:
//! here, or in WSL or over SSH through the shells kept open there.

use std::path::Path;
use std::time::Duration;

/// How long a listing may take.
const TIME: Duration = Duration::from_secs(300);

/// Runs `program` with `args` in `dir` (a folder of the repository); its output, or what it said
/// when it failed.
pub(crate) fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    if let Some(target) = crate::remote::parse(dir) {
        let words: Vec<String> = std::iter::once(program)
            .chain(args.iter().copied())
            .map(crate::remote::sh_quote)
            .collect();
        let out = crate::remote::exec(
            &target.host,
            &crate::remote::Request {
                cwd: Some(target.path.clone()),
                script: format!("exec {}", words.join(" ")),
                long: true,
                timeout: Some(TIME),
                ..crate::remote::Request::default()
            },
        )?;
        return finish(
            program,
            out.code == 0,
            &out.stdout,
            &out.stderr,
            out.code == 127,
        );
    }
    // npm is a batch file on Windows.
    let exe = if cfg!(windows) && program == "npm" {
        "npm.cmd"
    } else {
        program
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        // Plain output, as it is read.
        .env("NO_COLOR", "1")
        .env("CARGO_TERM_COLOR", "never");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(out) => finish(
            program,
            out.status.success(),
            &out.stdout,
            &out.stderr,
            false,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(missing(program)),
        Err(e) => Err(format!("Couldn't run {program}: {e}")),
    }
}

fn missing(program: &str) -> String {
    format!("{program} isn't installed where the repository is: it says what a package holds")
}

fn finish(
    program: &str,
    ok: bool,
    stdout: &[u8],
    stderr: &[u8],
    not_found: bool,
) -> Result<String, String> {
    if ok {
        return Ok(String::from_utf8_lossy(stdout).into_owned());
    }
    if not_found {
        return Err(missing(program));
    }
    let said = String::from_utf8_lossy(stderr).trim().to_owned();
    Err(if said.is_empty() {
        format!("{program} failed")
    } else {
        said
    })
}
