//! Bounded, noninteractive tool execution on the repository's machine.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use base64::Engine;

use crate::remote;

const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_ERROR: usize = 32 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
const ARGS: &[&str] = &[
    "--display",
    "json",
    "--color",
    "never",
    "--parse-error-limit",
    "0",
    "--byte-limit",
    "2097152",
    "--graph-limit",
    "1000000",
    "--strip-cr",
    "off",
];

/// User display/ignore overrides mustn't silently omit comments, force text, or skip output.
fn removed_env() -> Vec<String> {
    let mut names: Vec<String> = [
        "DFT_CHECK_ONLY",
        "DFT_IGNORE_COMMENTS",
        "DFT_SKIP_UNCHANGED",
        "DFT_EXIT_CODE",
        "DFT_OVERRIDE",
        "DFT_OVERRIDE_BINARY",
        "DFT_IGNORE",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for base in ["DFT_OVERRIDE", "DFT_OVERRIDE_BINARY"] {
        for i in 1..=9 {
            names.push(format!("{base}_{i}"));
        }
    }
    names
}

pub(super) fn compare(repo: &Path, path: &str, old: &str, new: &str) -> Result<Vec<u8>, String> {
    if let Some(target) = remote::parse(repo) {
        let request = request(&target, path, old, new);
        let out = remote::exec(&target.host, &request)?;
        return output(out.code, out.stdout, out.stderr);
    }
    let temp = Temp::new()?;
    let name = file_name(path)?;
    let (before, after) = (
        temp.0.join("before").join(name),
        temp.0.join("after").join(name),
    );
    for (file, text) in [(&before, old), (&after, new)] {
        std::fs::write(file, text)
            .map_err(|e| format!("Couldn't prepare syntax comparison: {e}"))?;
    }
    let mut command = Command::new("difft");
    crate::shell_path::apply(&mut command);
    command
        .current_dir(&temp.0)
        .args(ARGS)
        .arg("--")
        .arg(before)
        .arg(after)
        .env("DFT_UNSTABLE", "yes");
    for name in removed_env() {
        command.env_remove(name);
    }
    let out = run(command, TIMEOUT)?;
    output(out.code, out.stdout, out.stderr)
}

fn file_name(path: &str) -> Result<&str, String> {
    path.rsplit('/')
        .next()
        .filter(|p| !p.is_empty() && !matches!(*p, "." | ".."))
        .ok_or_else(|| "This file has no name to compare.".to_owned())
}

fn output(code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> Result<Vec<u8>, String> {
    if code == 127 {
        return Err(
            "Install Difftastic (difft) on the repository's machine to compare syntax.".to_owned(),
        );
    }
    if stdout.len() > MAX_OUTPUT || stderr.len() > MAX_ERROR {
        return Err("Difftastic's output was too large to show.".to_owned());
    }
    if code != 0 {
        let error = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "Difftastic couldn't compare these files: {}",
            error.trim()
        ));
    }
    Ok(stdout)
}

pub(super) fn request(
    target: &remote::Target,
    path: &str,
    old: &str,
    new: &str,
) -> remote::Request {
    let name = remote::sh_quote(file_name(path).unwrap_or("file"));
    let args = ARGS
        .iter()
        .map(|a| remote::sh_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    // A private directory on that machine, removed whether difft succeeds or fails. The
    // input is data, never interpolated into shell code. No repository programs are run.
    let script = format!(
        "umask 077\n\
         d=$(mktemp -d) || exit 1\n\
         trap 'rm -rf \"$d\"' 0\n\
         mkdir \"$d/before\" \"$d/after\" || exit 1\n\
         IFS= read -r before && IFS= read -r after || exit 1\n\
         printf '%s' \"$before\" | base64 -d > \"$d/before/\"{name} || exit 1\n\
         printf '%s' \"$after\" | base64 -d > \"$d/after/\"{name} || exit 1\n\
         cd \"$d\" || exit 1\n\
         (ulimit -f 32768; difft {args} -- \"$d/before/\"{name} \"$d/after/\"{name}) > \"$d/report\" 2> \"$d/error\"\n\
         code=$?\n\
         head -c {max_error} \"$d/error\" >&2\n\
         [ \"$code\" = 0 ] || exit \"$code\"\n\
         bytes=$(wc -c < \"$d/report\")\n\
         [ \"$bytes\" -le {max_output} ] || {{ echo 'syntax report too large' >&2; exit 1; }}\n\
         cat \"$d/report\"",
        max_error = MAX_ERROR,
        max_output = MAX_OUTPUT,
    );
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    remote::Request {
        cwd: Some(target.path.clone()),
        script,
        stdin: Some(format!("{}\n{}\n", encode(old), encode(new)).into_bytes()),
        env: vec![("DFT_UNSTABLE".to_owned(), "yes".to_owned())],
        unset: removed_env(),
        timeout: Some(TIMEOUT),
        long: true,
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        for _ in 0..100 {
            let path = root.join(format!(
                "gitcraft-syntax-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    let temp = Self(path);
                    for side in ["before", "after"] {
                        std::fs::create_dir(temp.0.join(side)).map_err(|e| e.to_string())?;
                    }
                    return Ok(temp);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Couldn't prepare syntax comparison: {e}")),
            }
        }
        Err("Couldn't create a private folder for syntax comparison.".to_owned())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Drain both pipes while the process runs, with a deadline even if a child holds a pipe.
pub(super) fn run(mut command: Command, timeout: Duration) -> Result<remote::Output, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Install Difftastic (difft) on the repository's machine to compare syntax."
                    .to_owned()
            } else {
                format!("Couldn't start Difftastic: {e}")
            }
        })?;
    let stdout = capture(
        child
            .stdout
            .take()
            .ok_or("Couldn't read Difftastic's output")?,
        MAX_OUTPUT,
    );
    let stderr = capture(
        child
            .stderr
            .take()
            .ok_or("Couldn't read Difftastic's error output")?,
        MAX_ERROR,
    );
    let deadline = Instant::now() + timeout;
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err("Syntax comparison took too long and was stopped.".to_owned());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let receive = |rx: Receiver<Result<Vec<u8>, String>>| {
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "Syntax comparison took too long and was stopped.".to_owned())?
        };
        Ok(remote::Output {
            code: status.code().unwrap_or(-1),
            stdout: receive(stdout)?,
            stderr: receive(stderr)?,
        })
    })();
    if result.is_err() {
        stop(&mut child);
    }
    result
}

fn capture(mut pipe: impl Read + Send + 'static, max: usize) -> Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe
            .by_ref()
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                if bytes.len() > max {
                    Err("Difftastic's output was too large to show.".to_owned())
                } else {
                    Ok(bytes)
                }
            });
        let _ = tx.send(result);
    });
    rx
}

fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .output();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}
