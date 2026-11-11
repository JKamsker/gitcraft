//! Real repositories, real git, stand-in tools.

use std::path::{Path, PathBuf};

use super::super::plan::{self, Place};
use super::super::preset::{Kind, Preset, ToolChoice};
use super::super::target::DiffTarget;
use super::super::{MergeOutcome, commit_target, open_diff, range_target, resolve_conflict};
use crate::git::run;

/// A file name with spaces, accents and a symbol, in a folder with a space.
const FILE: &str = "dír a/ñame ✓.txt";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    fn new(name: &str) -> Result<Repo, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-tool-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dír a")).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            // Nothing of the user's own config picks a tool for these tests.
            &["config", "diff.tool", ""],
            &["config", "merge.tool", ""],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.0), args).map_err(|e| format!("git {args:?}: {e}"))
    }

    fn write(&self, rel: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(rel), text).map_err(|e| e.to_string())
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.0.join(rel)).unwrap_or_default()
    }

    fn commit(&self, rel: &str, text: &str, message: &str) -> Result<String, String> {
        self.write(rel, text)?;
        self.git(&["add", "--", rel])?;
        self.git(&["commit", "-q", "-m", message])?;
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    /// A stand-in tool: `body` as a `sh` script, made runnable; returns its path.
    fn tool(&self, name: &str, body: &str) -> Result<String, String> {
        use std::os::unix::fs::PermissionsExt;
        let path = self.0.with_extension(format!("{name}.sh"));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// Sets git's `diff.tool` to a tool that copies both sides it is shown to `<log>.local`
    /// and `<log>.remote`, then exits with `code`.
    fn diff_tool(&self, code: i32) -> Result<PathBuf, String> {
        let log = self.0.with_extension("seen");
        let script = self.tool(
            "difftool",
            &format!(
                "cat \"$1\" > '{0}.local'; cat \"$2\" > '{0}.remote'; exit {code}",
                log.display()
            ),
        )?;
        self.git(&["config", "diff.tool", "fake"])?;
        self.git(&[
            "config",
            "difftool.fake.cmd",
            &format!("'{script}' \"$LOCAL\" \"$REMOTE\""),
        ])?;
        Ok(log)
    }

    /// Sets git's `merge.tool` to a tool running `body` with the merged file as `$1`.
    fn merge_tool(&self, body: &str, trust: bool) -> Result<(), String> {
        let script = self.tool("mergetool", body)?;
        self.git(&["config", "merge.tool", "fake"])?;
        self.git(&[
            "config",
            "mergetool.fake.cmd",
            &format!("'{script}' \"$MERGED\""),
        ])?;
        self.git(&[
            "config",
            "mergetool.fake.trustExitCode",
            if trust { "true" } else { "false" },
        ])?;
        Ok(())
    }

    /// Both branches changed `FILE`: merging leaves it conflicted.
    fn conflicted(name: &str) -> Result<Repo, String> {
        let repo = Repo::new(name)?;
        repo.commit(FILE, "base\n", "base")?;
        repo.git(&["checkout", "-q", "-b", "theirs"])?;
        repo.commit(FILE, "theirs\n", "theirs")?;
        repo.git(&["checkout", "-q", "main"])?;
        repo.commit(FILE, "ours\n", "ours")?;
        assert!(run(Some(repo.path()), &["merge", "theirs"]).is_err());
        Ok(repo)
    }

    fn still_conflicted(&self) -> Result<bool, String> {
        Ok(!self.git(&["ls-files", "-u"])?.trim().is_empty())
    }
}

fn configured() -> ToolChoice {
    ToolChoice::default()
}

fn seen(log: &Path) -> (String, String) {
    let read = |ext: &str| {
        std::fs::read_to_string(log.with_extension(format!("seen.{ext}"))).unwrap_or_default()
    };
    (read("local"), read("remote"))
}

#[test]
fn a_merge_tool_that_saves_resolves_the_file() -> Result<(), String> {
    let repo = Repo::conflicted("merge-saved")?;
    // git sees a save by the file's time, to the second: a tool that saved at once wouldn't be.
    repo.merge_tool("sleep 1.1; printf 'merged ✓\\n' > \"$1\"", false)?;
    let outcome = resolve_conflict(repo.path(), &configured(), FILE);
    assert_eq!(outcome, Ok(MergeOutcome::Resolved));
    assert!(!repo.still_conflicted()?);
    assert_eq!(repo.read(FILE), "merged ✓\n");
    // git's default: a backup of the file with its conflict markers.
    assert!(repo.read(&format!("{FILE}.orig")).contains("<<<<<<<"));
    Ok(())
}

#[test]
fn a_merge_tool_closed_without_saving_leaves_the_conflict() -> Result<(), String> {
    let repo = Repo::conflicted("merge-cancelled")?;
    repo.merge_tool("exit 0", false)?;
    let outcome = resolve_conflict(repo.path(), &configured(), FILE);
    assert!(
        matches!(&outcome, Ok(MergeOutcome::Unresolved(why)) if why.contains("closed without saving")),
        "{outcome:?}"
    );
    assert!(repo.still_conflicted()?);
    assert!(repo.read(FILE).contains("<<<<<<<"));
    Ok(())
}

#[test]
fn a_trusted_exit_code_decides_whether_the_merge_worked() -> Result<(), String> {
    // Trusted: saved, but it said it failed.
    let repo = Repo::conflicted("merge-trusted-fail")?;
    repo.merge_tool("echo merged > \"$1\"; exit 1", true)?;
    let outcome = resolve_conflict(repo.path(), &configured(), FILE);
    assert!(
        matches!(&outcome, Ok(MergeOutcome::Unresolved(why)) if why.contains("didn't report success")),
        "{outcome:?}"
    );
    assert!(repo.still_conflicted()?);
    // Not trusted: saving is what counts, whatever it exits with.
    let repo = Repo::conflicted("merge-untrusted-fail")?;
    repo.merge_tool("sleep 1.1; echo merged > \"$1\"; exit 1", false)?;
    let outcome = resolve_conflict(repo.path(), &configured(), FILE);
    assert_eq!(outcome, Ok(MergeOutcome::Resolved));
    assert!(!repo.still_conflicted()?);
    Ok(())
}

#[test]
fn no_backup_is_kept_when_keep_backup_is_off() -> Result<(), String> {
    let repo = Repo::conflicted("merge-no-backup")?;
    repo.git(&["config", "mergetool.keepBackup", "false"])?;
    repo.merge_tool("echo merged > \"$1\"", true)?;
    assert_eq!(
        resolve_conflict(repo.path(), &configured(), FILE),
        Ok(MergeOutcome::Resolved)
    );
    assert!(!repo.path().join(format!("{FILE}.orig")).exists());
    Ok(())
}

#[test]
fn a_preset_merge_runs_the_program_with_trust_from_git_config() -> Result<(), String> {
    let repo = Repo::conflicted("merge-preset")?;
    // KDiff3's options: the merged file follows `-o`.
    let program = repo.tool(
        "kdiff3",
        "while [ $# -gt 0 ]; do [ \"$1\" = -o ] && echo merged > \"$2\"; shift; done; exit 0",
    )?;
    let choice = ToolChoice {
        preset: Some(Preset::KDiff3),
        program,
    };
    assert_eq!(
        resolve_conflict(repo.path(), &choice, FILE),
        Ok(MergeOutcome::Resolved)
    );
    assert_eq!(repo.read(FILE), "merged\n");
    Ok(())
}

#[test]
fn conflicts_a_merge_tool_cannot_take_are_refused_before_starting_it() -> Result<(), String> {
    let repo = Repo::new("merge-deleted")?;
    repo.commit(FILE, "base\n", "base")?;
    repo.git(&["checkout", "-q", "-b", "theirs"])?;
    repo.git(&["rm", "-q", "--", FILE])?;
    repo.git(&["commit", "-q", "-m", "deleted"])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit(FILE, "ours\n", "ours")?;
    assert!(run(Some(repo.path()), &["merge", "theirs"]).is_err());
    let marker = repo.path().with_extension("ran");
    repo.merge_tool(&format!("touch '{}'", marker.display()), true)?;
    let refused = resolve_conflict(repo.path(), &configured(), FILE);
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("deleted on one side")),
        "{refused:?}"
    );
    assert!(!marker.exists());
    // A file with no conflict.
    let clean = Repo::new("merge-clean")?;
    clean.commit(FILE, "x\n", "x")?;
    let none = resolve_conflict(clean.path(), &configured(), FILE);
    assert!(
        none.as_ref().is_err_and(|e| e.contains("no conflict left")),
        "{none:?}"
    );
    Ok(())
}

#[test]
fn a_merge_tool_that_is_not_installed_is_an_error() -> Result<(), String> {
    // One of git's own tools, and gitcraft's VS Code command, whose program the shell
    // doesn't find: an error saying what to do, not "closed without saving".
    for (preset, name) in [(Preset::P4Merge, "P4Merge"), (Preset::VsCode, "VS Code")] {
        let repo = Repo::conflicted(&format!("merge-missing-{}", preset.id()))?;
        let choice = ToolChoice {
            preset: Some(preset),
            program: repo
                .path()
                .join("missing tool")
                .to_string_lossy()
                .into_owned(),
        };
        let outcome = resolve_conflict(repo.path(), &choice, FILE);
        assert!(
            outcome
                .as_ref()
                .is_err_and(|e| e.starts_with(&format!("{name} could not open"))
                    && e.contains("Install it, or set where it is")),
            "{outcome:?}"
        );
        assert!(repo.still_conflicted()?);
        let unstaged = DiffTarget::Unstaged {
            path: FILE.to_owned(),
        };
        repo.git(&["merge", "--abort"])?;
        repo.write(FILE, "changed\n")?;
        let diff = open_diff(repo.path(), &choice, &unstaged);
        assert!(
            diff.as_ref()
                .is_err_and(|e| e.contains("Install it, or set where it is")),
            "{diff:?}"
        );
    }
    Ok(())
}

/// `git mergetool` reads names as patterns: a file named `x*.txt` would merge `xa.txt`
/// too, which nobody asked for.
#[test]
fn a_file_named_like_a_pattern_is_not_handed_to_mergetool() -> Result<(), String> {
    let repo = Repo::new("merge-glob")?;
    for name in ["x*.txt", "xa.txt"] {
        repo.commit(name, "base\n", "base")?;
    }
    repo.git(&["checkout", "-q", "-b", "theirs"])?;
    for name in ["x*.txt", "xa.txt"] {
        repo.commit(name, "theirs\n", "theirs")?;
    }
    repo.git(&["checkout", "-q", "main"])?;
    for name in ["x*.txt", "xa.txt"] {
        repo.commit(name, "ours\n", "ours")?;
    }
    assert!(run(Some(repo.path()), &["merge", "theirs"]).is_err());
    repo.merge_tool("echo merged > \"$1\"", true)?;
    let refused = resolve_conflict(repo.path(), &configured(), "x*.txt");
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("pattern")),
        "{refused:?}"
    );
    assert!(repo.read("xa.txt").contains("<<<<<<<"));
    Ok(())
}

/// A file whose name starts with `-` reaches gitcraft's own command (VS Code) as a file.
#[test]
fn a_file_named_like_an_option_reaches_the_tool_as_a_file() -> Result<(), String> {
    let repo = Repo::new("diff-dash")?;
    repo.commit("-dash.txt", "one\n", "first")?;
    repo.write("-dash.txt", "two\n")?;
    let args = repo.path().with_extension("args");
    let program = repo.tool(
        "code",
        &format!(
            "for a; do printf '%s\\n' \"$a\"; done > '{}'",
            args.display()
        ),
    )?;
    let choice = ToolChoice {
        preset: Some(Preset::VsCode),
        program,
    };
    let unstaged = DiffTarget::Unstaged {
        path: "-dash.txt".to_owned(),
    };
    assert_eq!(open_diff(repo.path(), &choice, &unstaged), Ok(()));
    let got = std::fs::read_to_string(&args).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(lines.first(), Some(&"--wait"), "{got}");
    assert!(lines.iter().skip(2).all(|l| !l.starts_with('-')), "{got}");
    assert!(lines.contains(&"./-dash.txt"), "{got}");
    Ok(())
}

/// Over SSH the tool is that machine's: a program typed for this one isn't used there.
#[test]
fn a_typed_program_is_not_used_over_ssh() -> Result<(), String> {
    let repo = Repo::new("program-place")?;
    let typed = r"C:\Tools\BComp.exe";
    let ssh = Place::Ssh("build".to_owned());
    let program_for = super::super::run::program_for;
    assert_eq!(
        program_for(repo.path(), Preset::BeyondCompare, typed, &ssh, Kind::Diff),
        Ok(None)
    );
    assert_eq!(
        program_for(
            repo.path(),
            Preset::BeyondCompare,
            typed,
            &Place::Here,
            Kind::Diff
        ),
        Ok(Some(typed.to_owned()))
    );
    // VS Code in WSL is its own `code` there, not a Windows program to look for.
    assert_eq!(
        program_for(repo.path(), Preset::VsCode, "", &Place::Wsl, Kind::Merge),
        Ok(None)
    );
    Ok(())
}

/// The command git in WSL runs a Windows tool with, run here with stand-ins for `wslpath`
/// and the program: it gets `\\wsl.localhost` paths of real files.
#[test]
fn in_wsl_the_windows_tool_gets_wsl_localhost_paths() -> Result<(), String> {
    use std::process::Command;
    let repo = Repo::new("wsl-script")?;
    let bin = repo.path().join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let args = repo.path().join("args");
    let program = repo.tool(
        "windows-tool",
        &format!(
            "for a; do printf '%s\\n' \"$a\"; done > '{}'",
            args.display()
        ),
    )?;
    // `-u` gives the program; `-w` a path as Windows sees it.
    let wslpath = format!(
        r#"if [ "$1" = -u ]; then echo '{program}'; else printf '%s%s\n' '\\wsl.localhost\Test' "$(printf '%s' "$2" | tr / '\\')"; fi"#
    );
    let wslpath_file = bin.join("wslpath");
    std::fs::write(&wslpath_file, format!("#!/bin/sh\n{wslpath}\n")).map_err(|e| e.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wslpath_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    repo.write("merged file.txt", "x")?;
    let plan = plan::preset(
        Preset::BeyondCompare,
        Kind::Merge,
        &Place::Wsl,
        Some(r"C:\Tools\BComp.exe"),
        None,
    );
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("sh")
        .arg("-c")
        .arg(&plan.config[0].1)
        .current_dir(repo.path())
        .env("PATH", path)
        .env("TMPDIR", repo.path())
        .env("LOCAL", "/dev/null")
        .env("REMOTE", repo.path().join("merged file.txt"))
        .env("BASE", repo.path().join("no base yet"))
        .env("MERGED", "merged file.txt")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = std::fs::read_to_string(&args).unwrap_or_default();
    let win = |p: &Path| {
        format!(
            r"\\wsl.localhost\Test{}",
            p.display().to_string().replace('/', r"\")
        )
    };
    let empty = repo.path().join("gitcraft-empty-file");
    let expected = [
        win(&empty),
        win(&repo.path().join("merged file.txt")),
        win(&repo.path().join("no base yet")),
        format!("-mergeoutput={}", win(&repo.path().join("merged file.txt"))),
    ];
    assert_eq!(got.lines().collect::<Vec<_>>(), expected);
    // `/dev/null` and a missing base became empty files the tool can open.
    assert_eq!(std::fs::read(&empty).map(|b| b.len()).ok(), Some(0));
    assert!(repo.path().join("no base yet").exists());
    Ok(())
}

mod diff;
