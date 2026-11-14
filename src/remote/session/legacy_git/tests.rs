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
        Self::with_git(name, FAKE_GIT)
    }

    fn with_git(name: &str, git_script: &str) -> Result<Self, String> {
        let root =
            std::env::temp_dir().join(format!("gitcraft-legacy-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("home")).map_err(|e| e.to_string())?;
        let git = bin.join("git");
        std::fs::write(&git, git_script).map_err(|e| e.to_string())?;
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

/// Real Git that refuses `--no-lazy-fetch` like Git before 2.44. Without the option it
/// lazy-fetches just as those versions do even when told not to.
const OLD_REAL_GIT: &str = r#"#!/bin/sh
[ "$1" = --no-lazy-fetch ] && { echo 'unknown option: --no-lazy-fetch' >&2; exit 129; }
exec "$REAL_GIT" "$@"
"#;

/// A superproject whose submodule is a blob-less partial clone lacking the blob of the
/// submodule's older commit, diffed with the submodule shown inline. Prints whether that
/// blob is present before and after, and the diff's exit status.
const SUBMODULE_SCENARIO: &str = r#"set -e
cd "$WORK"
git init -q src
git -C src config uploadpack.allowFilter true
git -C src config uploadpack.allowAnySHA1InWant true
echo one >src/a
git -C src add a
git -C src commit -qm one
old=$(git -C src rev-parse HEAD)
blob=$(git -C src rev-parse HEAD:a)
echo two >src/a
git -C src commit -qam two
new=$(git -C src rev-parse HEAD)
git init -q super
cd super
git clone -q --filter=blob:none "file://$WORK/src" sub
printf '[submodule "sub"]\n\tpath = sub\n\turl = ../src\n' >.gitmodules
git add .gitmodules
git update-index --add --cacheinfo "160000,$old,sub"
git commit -qm old
git update-index --cacheinfo "160000,$new,sub"
git commit -qm new
present() {
    git -C sub cat-file --batch-all-objects --batch-check='%(objectname)' | grep -qx "$blob" && echo yes || echo no
}
echo "before $(present)"
set +e
git --no-lazy-fetch -c diff.submodule=diff diff HEAD~1 HEAD >/dev/null 2>&1
echo "diff $?"
echo "after $(present)"
"#;

#[test]
fn an_old_git_keeps_a_partial_clone_submodule_from_fetching() -> Result<(), String> {
    let real = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .map_err(|e| e.to_string())?;
    let real = String::from_utf8_lossy(&real.stdout).trim().to_owned();
    assert!(real.starts_with('/'), "no Git to test with: {real:?}");
    let machine = Machine::with_git("submodule", OLD_REAL_GIT)?;
    let work = machine.0.join("work");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let work = work.display().to_string();
    let out = machine.run(
        &[
            ("REAL_GIT", real.as_str()),
            ("WORK", work.as_str()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_AUTHOR_NAME", "t"),
            ("GIT_AUTHOR_EMAIL", "t@example.com"),
            ("GIT_COMMITTER_NAME", "t"),
            ("GIT_COMMITTER_EMAIL", "t@example.com"),
        ],
        SUBMODULE_SCENARIO,
    )?;
    assert!(exists(&machine.shim()));
    // The superproject is ordinary, so the diff runs; only the submodule's fetch fails.
    assert_eq!(
        lines(&out.stdout),
        ["before no", "diff 0", "after no"],
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}
