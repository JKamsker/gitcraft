//! Running a forge's own command-line tool (`gh`, `glab`), which holds the user's login.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Forge;

/// Runs `tool` in `repo`; `noun` says what it's for when it isn't installed.
pub(super) fn run(tool: &str, noun: &str, repo: &Path, args: &[&str]) -> Result<String, String> {
    run_with_input(tool, noun, repo, args, None)
}

/// [`run`], with `input` on its stdin.
pub(super) fn run_with_input(
    tool: &str,
    noun: &str,
    repo: &Path,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, String> {
    let mut cmd = tool_command(tool);
    let remote = crate::remote::parse(repo).is_some();
    if remote {
        // The tool runs here, where the user signed in to it, about the repository there:
        // named rather than found from the folder, which it can't read.
        let forge = forge_of(tool)?;
        let (host, path) = remote_location(repo, forge)?;
        cmd.current_dir(std::env::temp_dir()).args(args);
        match forge {
            Forge::GitLab => {
                cmd.env("GITLAB_HOST", &host);
                // `glab api` takes no `--repo` (it would fail on it): its calls name the host
                // and the project themselves.
                if gitlab_takes_repo(args) {
                    cmd.args(["--repo", &path]);
                }
            }
            _ => {
                cmd.env("GH_REPO", format!("{host}/{path}"));
            }
        }
    } else {
        cmd.current_dir(repo).args(args);
    }
    let not_installed = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotFound if remote || repo.is_dir() => {
            format!("{tool} isn't installed: gitcraft uses it for {noun}s")
        }
        _ => format!("couldn't run {tool} in {}: {e}", repo.display()),
    };
    let out = match input {
        None => cmd.output().map_err(not_installed)?,
        Some(input) => {
            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(not_installed)?;
            if let Some(mut stdin) = child.stdin.take() {
                // A tool that stops reading has stopped for a reason, which its exit code and
                // error output below tell better than the broken pipe would.
                let _ = stdin.write_all(input.as_bytes());
            }
            child.wait_with_output().map_err(|e| e.to_string())?
        }
    };
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(failure(tool, &out))
    }
}

/// The forge whose tool `tool` is.
fn forge_of(tool: &str) -> Result<Forge, String> {
    match tool {
        "gh" => Ok(Forge::GitHub),
        "glab" => Ok(Forge::GitLab),
        _ => Err(format!("{tool} can't be run for a repository on another machine")),
    }
}

/// Whether `glab` with `args` takes `--repo`: every command but `api`.
fn gitlab_takes_repo(args: &[&str]) -> bool {
    args.first().is_some_and(|sub| *sub != "api")
}

/// The web host and `owner/repo` of the remote the forge is found from (see
/// [`super::detect`]), for a repository on another machine. Asked there each time (one round
/// trip in a shell kept open), so a changed `origin` is followed at once. For an `http(s)`
/// remote on a port of its own, GitLab's host keeps that port: its web pages and API are on it.
fn remote_location(repo: &Path, forge: Forge) -> Result<(String, String), String> {
    let remotes = crate::git::remotes(repo)?;
    let url = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first())
        .map(|r| r.url.as_str())
        .unwrap_or_default();
    let tool = match forge {
        Forge::GitLab => "glab",
        _ => "gh",
    };
    let (host, path) = tool_location(forge, url)
        .ok_or_else(|| format!("{tool} needs a remote with an owner/repo URL"))?;
    // Named to the tool, a host is used whether or not the user signed in to it (in a folder,
    // the tool skips remotes on hosts it doesn't know): only the forge's own or one signed in
    // to, so a repository can't send the tool, and a token, to a host of its choosing.
    if !trusted_host(forge, tool, &host) {
        return Err(format!(
            "{tool} isn't signed in to {host}: `{tool} auth login --hostname {host}` to use it",
        ));
    }
    Ok((host, path))
}

/// The host and `owner/repo` the tool is given for remote `url`: GitLab's host with the port of
/// an `http(s)` URL, an SSH port left out (it says nothing of the web pages).
fn tool_location(forge: Forge, url: &str) -> Option<(String, String)> {
    let (host, path) = super::location(url)?;
    let host = match (forge, http_port(url)) {
        (Forge::GitLab, Some(port)) => format!("{host}:{port}"),
        _ => host,
    };
    Some((host, path))
}

/// The port of an `http(s)` URL that names one.
fn http_port(url: &str) -> Option<&str> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (_, port) = host.rsplit_once(':')?;
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).then_some(port)
}

/// How long the tool may take to say whether it is signed in to a host.
const AUTH_CHECK_FOR: Duration = Duration::from_secs(10);
/// How long a host the tool isn't signed in to is taken as such before it is asked again (the
/// user may sign in meanwhile).
const NOT_SIGNED_IN_FOR: Duration = Duration::from_secs(60);

/// Whether `forge`'s tool may be pointed at `host`: the forge's own host, or one the tool is
/// signed in to. A host that isn't a plain name (`a.b:8443` at most) never is. What the tool
/// said is remembered: yes while the app runs, no for [`NOT_SIGNED_IN_FOR`].
fn trusted_host(forge: Forge, tool: &str, host: &str) -> bool {
    use std::sync::{Mutex, OnceLock};
    type Known = Mutex<HashMap<(Forge, String), (bool, Instant)>>;
    static KNOWN: OnceLock<Known> = OnceLock::new();
    let plain = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        && !host.starts_with(['-', '.']);
    if !plain {
        return false;
    }
    let own = match forge {
        Forge::GitLab => "gitlab.com",
        _ => "github.com",
    };
    if host.eq_ignore_ascii_case(own) {
        return true;
    }
    let known = KNOWN.get_or_init(Known::default);
    let lock = || {
        known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let key = (forge, host.to_lowercase());
    if let Some(&(signed_in, at)) = lock().get(&key)
        && (signed_in || at.elapsed() < NOT_SIGNED_IN_FOR)
    {
        return signed_in;
    }
    let signed_in = signed_in_to(tool, host);
    lock().insert(key, (signed_in, Instant::now()));
    signed_in
}

/// Whether `tool` says it is signed in to `host`; no when it doesn't say so within
/// [`AUTH_CHECK_FOR`] (it is stopped).
fn signed_in_to(tool: &str, host: &str) -> bool {
    let mut cmd = tool_command(tool);
    cmd.args(["auth", "status", "--hostname", host])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let until = Instant::now() + AUTH_CHECK_FOR;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The forge's tool, set to answer this repository in plain JSON.
fn tool_command(tool: &str) -> Command {
    let mut cmd = Command::new(tool);
    cmd.env("GH_PROMPT_DISABLED", "1").env("NO_PROMPT", "1");
    // What it prints is read as JSON: `CLICOLOR_FORCE` or `GH_FORCE_TTY` in the user's
    // environment would colour it even into a pipe, escape codes no JSON reader takes.
    cmd.env_remove("CLICOLOR_FORCE")
        .env_remove("GH_FORCE_TTY")
        .env("NO_COLOR", "1");
    // About the repository it runs in and the host it is given, not one the environment names:
    // `GH_REPO` would answer for another repository, `GH_HOST` send `gh api` (and `-R owner/repo`)
    // to another host than github.com, `GITLAB_HOST` point `glab` at another GitLab than the
    // remote's.
    cmd.env_remove("GH_REPO")
        .env_remove("GH_HOST")
        .env_remove("GITLAB_HOST");
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// What a tool that failed said, or that it failed: an empty error would read as nothing wrong.
fn failure(tool: &str, out: &std::process::Output) -> String {
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    let stderr = said(&out.stderr);
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = said(&out.stdout);
    if !stdout.is_empty() {
        return stdout;
    }
    format!("{tool} failed ({})", out.status)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-hosted GitLab on a port of its own is reached on it; an SSH port isn't a web one.
    #[test]
    fn gitlab_keeps_the_port_of_its_web_pages() {
        let at = |forge, url| tool_location(forge, url);
        assert_eq!(
            at(Forge::GitLab, "https://git.example.org:8443/team/app.git"),
            Some(("git.example.org:8443".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "ssh://git@git.example.org:2222/team/app.git"),
            Some(("git.example.org".to_owned(), "team/app".to_owned()))
        );
        assert_eq!(
            at(Forge::GitLab, "git@gitlab.com:team/app.git"),
            Some(("gitlab.com".to_owned(), "team/app".to_owned()))
        );
        // `GH_REPO` takes a plain host.
        assert_eq!(
            at(Forge::GitHub, "https://ghe.example.org:8443/team/app"),
            Some(("ghe.example.org".to_owned(), "team/app".to_owned()))
        );
    }

    /// `glab api` would refuse `--repo`; the commands about a project take it.
    #[test]
    fn only_glab_commands_about_a_project_take_repo() {
        assert!(!gitlab_takes_repo(&[
            "api",
            "--hostname",
            "gitlab.com",
            "projects/x"
        ]));
        assert!(gitlab_takes_repo(&["mr", "list", "-F", "json"]));
        assert!(!gitlab_takes_repo(&[]));
    }

    /// The user's environment can't colour the JSON read from the tools, nor point them at
    /// another repository or host.
    #[test]
    fn tools_answer_in_plain_json_about_this_repository() {
        for tool in ["gh", "glab"] {
            let cmd = tool_command(tool);
            let envs: HashMap<String, Option<String>> = cmd
                .get_envs()
                .map(|(k, v)| {
                    let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
                    (text(k), v.map(text))
                })
                .collect();
            for removed in [
                "CLICOLOR_FORCE",
                "GH_FORCE_TTY",
                "GH_REPO",
                "GH_HOST",
                "GITLAB_HOST",
            ] {
                assert_eq!(envs.get(removed), Some(&None), "{removed}");
            }
            assert_eq!(envs.get("NO_COLOR"), Some(&Some("1".to_owned())));
        }
    }
}
