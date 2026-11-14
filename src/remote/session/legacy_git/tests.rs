use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A Git that prints its arguments one per line; `old` ones reject a leading
/// `--no-lazy-fetch` the way Git before 2.44 does, and `partial` repositories have
/// promisor configuration.
const FAKE_GIT: &str = r#"#!/bin/sh
if [ "$1" = --no-lazy-fetch ] && [ -n "$FAKE_OLD" ]; then
    echo 'unknown option: --no-lazy-fetch' >&2; exit 129
fi
for a do
    case $a in config) [ -n "$FAKE_PARTIAL" ] && { echo remote.origin.promisor true; exit 0; }; exit 1;; esac
done
[ "$1" = --version ] && { echo 'git version 2.39.5'; exit 0; }
for a do printf '%s\n' "$a"; done
"#;

struct Machine(PathBuf);

impl Machine {
    fn new(name: &str) -> Result<Self, String> {
        let root =
            std::env::temp_dir().join(format!("gitcraft-legacy-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("home")).map_err(|e| e.to_string())?;
        let git = bin.join("git");
        std::fs::write(&git, FAKE_GIT).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn shim(&self) -> PathBuf {
        self.0.join("home/.cache/gitcraft/legacy-git/git")
    }

    /// Runs `script` after a new shell's setup, the way commands run there.
    fn run(&self, env: &[(&str, &str)], script: &str) -> Result<std::process::Output, String> {
        let path = format!("{}:/usr/bin:/bin", self.0.join("bin").display());
        Command::new("sh")
            .arg("-c")
            .arg(format!("{}\n{script}", super::setup()))
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.0.join("home"))
            .envs(env.iter().copied())
            .output()
            .map_err(|e| e.to_string())
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn lines(out: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(out)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn exists(path: &Path) -> bool {
    path.try_exists().unwrap_or(false)
}

#[test]
fn an_old_git_runs_passive_reads_of_an_ordinary_repository_without_the_option() -> Result<(), String>
{
    let machine = Machine::new("ordinary")?;
    let out = machine.run(
        &[("FAKE_OLD", "1")],
        "exec git --no-lazy-fetch -c 'a=b c' --no-replace-objects cat-file -p 'x y' --no-lazy-fetch",
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        lines(&out.stdout),
        [
            "-c",
            "a=b c",
            "--no-replace-objects",
            "cat-file",
            "-p",
            "x y",
            "--no-lazy-fetch"
        ]
    );
    assert!(exists(&machine.shim()));
    Ok(())
}

#[test]
fn an_old_git_refuses_a_partial_clone_instead_of_fetching() -> Result<(), String> {
    let machine = Machine::new("partial")?;
    let out = machine.run(
        &[("FAKE_OLD", "1"), ("FAKE_PARTIAL", "1")],
        "git --no-lazy-fetch cat-file -p HEAD",
    )?;
    assert_eq!(out.status.code(), Some(125));
    assert!(out.stdout.is_empty());
    let why = String::from_utf8_lossy(&out.stderr);
    assert!(why.contains("partial clone"), "{why}");
    assert!(why.contains("git version 2.39.5"), "{why}");
    // Commands that don't ask for the guard still run there.
    let out = machine.run(
        &[("FAKE_OLD", "1"), ("FAKE_PARTIAL", "1")],
        "git fetch origin",
    )?;
    assert!(out.status.success());
    assert_eq!(lines(&out.stdout), ["fetch", "origin"]);
    Ok(())
}

#[test]
fn a_git_that_knows_the_option_gets_no_shim() -> Result<(), String> {
    let machine = Machine::new("modern")?;
    let out = machine.run(&[], "git --no-lazy-fetch cat-file -p HEAD")?;
    assert!(out.status.success());
    assert_eq!(
        lines(&out.stdout),
        ["--no-lazy-fetch", "cat-file", "-p", "HEAD"]
    );
    assert!(!exists(&machine.shim()));
    Ok(())
}

#[test]
fn setup_is_silent_and_leaves_the_input_alone() -> Result<(), String> {
    let machine = Machine::new("silent")?;
    let path = format!("{}:/usr/bin:/bin", machine.0.join("bin").display());
    let mut child = Command::new("sh")
        .env_clear()
        .env("PATH", path)
        .env("HOME", machine.0.join("home"))
        .env("FAKE_OLD", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let script = format!("{}\necho ready\n", super::setup());
    std::io::Write::write_all(child.stdin.as_mut().ok_or("no stdin")?, script.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    assert_eq!(lines(&out.stdout), ["ready"]);
    Ok(())
}
