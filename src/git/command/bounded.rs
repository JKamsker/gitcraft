//! Binary Git streams with input/output limits and a deadline, also on remote machines.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::remote;

const ERROR_LIMIT: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

pub(in crate::git) fn run(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if let Some(target) = remote::parse(repo) {
        return there(&target, args, input, limit);
    }
    local(repo, args, input, limit, TIMEOUT)
}

pub(in crate::git) fn local(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut cmd = super::local_command(Some(repo), &super::Extra::default(), args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("Could not write Git input")?;
    let (written, writer) = channel();
    std::thread::spawn(move || {
        let _ = written.send(stdin.write_all(&input).map_err(|e| e.to_string()));
    });
    let stdout = capture(
        child.stdout.take().ok_or("Could not read Git output")?,
        limit,
    );
    let stderr = capture(
        child.stderr.take().ok_or("Could not read Git errors")?,
        ERROR_LIMIT,
    );
    let deadline = Instant::now() + timeout;
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err("Bundle operation timed out".to_owned());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let out = receive(stdout, deadline)?;
        let err = receive(stderr, deadline)?;
        if !status.success() {
            let message = String::from_utf8_lossy(&err).trim().to_owned();
            return Err(if message.is_empty() {
                format!("Git failed: {status}")
            } else {
                message
            });
        }
        writer
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "Git input writer timed out".to_owned())??;
        Ok(out)
    })();
    if result.is_err() {
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
    result
}

fn capture(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
) -> Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe.by_ref().take(limit as u64 + 1).read_to_end(&mut bytes)
            .map_err(|e| e.to_string()).and_then(|_| {
                if bytes.len() > limit { Err("Bundle output exceeds the size limit; use Git in the terminal for a larger bundle".to_owned()) }
                else { Ok(bytes) }
            });
        let _ = tx.send(result);
    });
    rx
}
fn receive(rx: Receiver<Result<Vec<u8>, String>>, deadline: Instant) -> Result<Vec<u8>, String> {
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "Bundle operation timed out".to_owned())?
}

fn there(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let request = request(target, args, input, limit);
    let out = remote::exec(&target.host, &request)?;
    if out.stdout.len() > limit || out.stderr.len() > ERROR_LIMIT {
        return Err("Bundle output exceeds the size limit".to_owned());
    }
    if out.code != 0 {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("Git failed with exit code {}", out.code)
        } else {
            message
        });
    }
    Ok(out.stdout)
}

pub(in crate::git) fn request(
    target: &remote::Target,
    args: &[&str],
    input: Vec<u8>,
    limit: usize,
) -> remote::Request {
    let mut req = super::remote_request(target, &super::Extra::default(), args);
    let git = req.script;
    // Git stdout is binary. Bound disk use before passing it through the remote protocol.
    // The inner subshell keeps `exec git` from replacing the cleanup shell.
    req.script = format!(
        "umask 077\nd=$(mktemp -d) || exit 1\ntrap 'rm -rf \"$d\"' 0\ntrap 'exit 1' HUP INT TERM PIPE\n\
         (ulimit -f {blocks} || exit 1; {git}) >\"$d/out\" 2>\"$d/err\"\n\
         code=$?\nhead -c {errors} \"$d/err\" >&2\n[ \"$code\" = 0 ] || exit \"$code\"\n\
         [ \"$(wc -c <\"$d/out\")\" -le {limit} ] || {{ echo 'Bundle output exceeds the size limit' >&2; exit 1; }}\n\
         cat \"$d/out\"",
        // Unbundle writes the incoming pack into the object store, so the file limit
        // must cover the bundle size even when only a short ref list is returned.
        blocks = (limit.max(128 * 1024 * 1024) / 512) + 1,
        errors = ERROR_LIMIT,
    );
    req.stdin = Some(input);
    req.timeout = Some(TIMEOUT);
    req.long = true;
    req
}
