//! How each tool is started, and real `git difftool` / `git mergetool` runs on temporary
//! repositories with stand-in tools (scripts) that succeed, fail or are closed without saving.

use super::plan::{self, AS_FILE, Place, tool_args, windows_program};
use super::preset::{Kind, Preset, Slot, ToolChoice, Word};
use super::target::DiffTarget;

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// The `-c` settings of a plan, as `key=value`.
fn settings(plan: &plan::Plan) -> Vec<String> {
    plan.config
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect()
}

#[test]
fn a_preset_git_knows_is_started_by_its_own_name() {
    for preset in Preset::ALL {
        let Some(git_name) = preset.git_name() else {
            continue;
        };
        for kind in [Kind::Diff, Kind::Merge] {
            for place in [Place::Here, Place::Ssh("build".to_owned())] {
                let p = plan::preset(preset, kind, &place, None, None);
                assert_eq!(p.name, git_name, "{preset:?}");
                assert!(p.config.is_empty(), "{preset:?} {kind:?}: {:?}", p.config);
                assert_eq!(p.label, preset.label());
            }
        }
    }
}

#[test]
fn a_program_set_for_a_preset_is_given_to_git_as_the_tool_path() {
    let program = "/opt/Beyond Compare/bcompare";
    let diff = plan::preset(
        Preset::BeyondCompare,
        Kind::Diff,
        &Place::Here,
        Some(program),
        None,
    );
    assert_eq!(settings(&diff), [format!("difftool.bc.path={program}")]);
    let merge = plan::preset(
        Preset::KDiff3,
        Kind::Merge,
        &Place::Here,
        Some(program),
        None,
    );
    assert_eq!(
        settings(&merge),
        [format!("mergetool.kdiff3.path={program}")]
    );
}

#[test]
fn vs_code_is_started_with_its_wait_and_diff_or_merge_options() {
    let diff = plan::preset(Preset::VsCode, Kind::Diff, &Place::Here, None, None);
    assert_eq!(diff.name, "gitcraft-vscode");
    assert_eq!(
        settings(&diff),
        [format!(
            r#"difftool.gitcraft-vscode.cmd={AS_FILE}; 'code' '--wait' '--diff' "$(gc_a "$LOCAL")" "$(gc_a "$REMOTE")""#
        )]
    );
    let merge = plan::preset(Preset::VsCode, Kind::Merge, &Place::Here, None, None);
    assert_eq!(
        settings(&merge),
        [
            format!(
                r#"mergetool.gitcraft-vscode.cmd={AS_FILE}; 'code' '--wait' '--merge' "$(gc_a "$REMOTE")" "$(gc_a "$LOCAL")" "$(gc_a "$BASE")" "$(gc_a "$MERGED")""#
            ),
            "mergetool.gitcraft-vscode.trustExitCode=false".to_owned(),
        ]
    );
    // A program with a space and a quote in its path stays one word.
    let odd = plan::preset(
        Preset::VsCode,
        Kind::Diff,
        &Place::Ssh("h".to_owned()),
        Some("/opt/my code/it's"),
        None,
    );
    assert!(
        odd.config[0]
            .1
            .contains(r"; '/opt/my code/it'\''s' '--wait'"),
        "{:?}",
        odd.config
    );
}

#[test]
fn every_preset_gets_both_files_to_compare_and_one_to_merge_into() {
    let count = |words: &[Word], slot: Slot| {
        words
            .iter()
            .filter(|w| matches!(w, Word::File(_, s) if *s == slot))
            .count()
    };
    for preset in Preset::ALL {
        let diff = preset.diff_words();
        assert_eq!(count(diff, Slot::Local), 1, "{preset:?}");
        assert_eq!(count(diff, Slot::Remote), 1, "{preset:?}");
        assert_eq!(count(diff, Slot::Merged), 0, "{preset:?}");
        let merge = preset.merge_words();
        for slot in [Slot::Local, Slot::Remote, Slot::Merged] {
            assert_eq!(count(merge, slot), 1, "{preset:?} {slot:?}");
        }
        assert!(!preset.windows_places().is_empty(), "{preset:?}");
    }
}

#[test]
fn in_wsl_every_preset_is_the_windows_program_given_windows_paths() {
    let exe = r"C:\Program Files\Tool\tool.exe";
    for preset in Preset::ALL {
        for kind in [Kind::Diff, Kind::Merge] {
            let p = plan::preset(preset, kind, &Place::Wsl, Some(exe), None);
            assert_eq!(p.name, format!("gitcraft-{}", preset.id()));
            let (key, script) = &p.config[0];
            assert_eq!(
                key,
                &format!("{}.gitcraft-{}.cmd", kind.command(), preset.id())
            );
            assert!(script.starts_with("gc_w() {"), "{script}");
            assert!(
                script.contains(r#""$(wslpath -u 'C:\Program Files\Tool\tool.exe')""#),
                "{script}"
            );
            // Every file goes through gc_w, never bare.
            assert!(script.contains(r#""$(gc_w "$LOCAL")""#), "{script}");
            assert_eq!(
                script.matches(r#""$LOCAL""#).count(),
                script.matches(r#"gc_w "$LOCAL""#).count(),
                "{script}"
            );
            if kind == Kind::Merge {
                assert!(script.contains(r#""$(gc_w "$MERGED")""#), "{script}");
                assert_eq!(p.config.len(), 2);
            }
        }
    }
    // A program typed as a path there (or a bare name) is run as it is, with the paths there.
    let p = plan::preset(
        Preset::Meld,
        Kind::Diff,
        &Place::Wsl,
        Some("/usr/bin/meld"),
        None,
    );
    assert!(
        p.config[0]
            .1
            .contains(r#"; '/usr/bin/meld' "$(gc_a "$LOCAL")""#),
        "{:?}",
        p.config
    );
    assert!(!p.config[0].1.contains("wslpath"), "{:?}", p.config);
    let code = plan::preset(Preset::VsCode, Kind::Diff, &Place::Wsl, None, None);
    assert!(
        code.config[0].1.contains("; 'code' '--wait'"),
        "{:?}",
        code.config
    );
    assert!(!code.config[0].1.contains("wslpath"), "{:?}", code.config);
    // An option glued to a file keeps them together.
    let bc = plan::preset(
        Preset::BeyondCompare,
        Kind::Merge,
        &Place::Wsl,
        Some(exe),
        None,
    );
    assert!(
        bc.config[0]
            .1
            .ends_with(r#"'-mergeoutput='"$(gc_w "$MERGED")""#),
        "{:?}",
        bc.config
    );
}

#[test]
fn trust_in_the_exit_code_is_the_users_setting_or_gits_default() {
    let trust = |preset, setting| {
        plan::preset(preset, Kind::Merge, &Place::Wsl, Some("x"), setting).config[1]
            .1
            .clone()
    };
    assert_eq!(trust(Preset::KDiff3, None), "true");
    assert_eq!(trust(Preset::KDiff3, Some(false)), "false");
    assert_eq!(trust(Preset::WinMerge, None), "false");
    assert_eq!(trust(Preset::P4Merge, Some(true)), "true");
}

#[test]
fn a_configured_tool_is_used_by_its_name_unless_it_needs_a_terminal() {
    let p = plan::configured(" bc4 \n", Kind::Diff);
    assert_eq!(p.map(|p| p.name), Ok("bc4".to_owned()));
    assert!(plan::configured("gvimdiff", Kind::Merge).is_ok());
    assert!(plan::configured("my-vimdiff-wrapper", Kind::Merge).is_ok());
    for terminal in ["vimdiff", "vimdiff3", "nvimdiff", "nvimdiff1"] {
        let e = plan::configured(terminal, Kind::Merge)
            .err()
            .unwrap_or_default();
        assert!(e.contains("runs in a terminal"), "{terminal}: {e}");
        assert!(e.contains("git mergetool"), "{e}");
    }
    let none = plan::configured("", Kind::Diff).err().unwrap_or_default();
    assert!(none.contains("No diff tool is set"), "{none}");
    assert!(none.contains("Settings → Git → External tools"), "{none}");
    assert!(none.contains("diff.tool"), "{none}");
}

#[test]
fn the_git_command_has_its_settings_first_and_literal_paths() {
    let p = plan::preset(Preset::VsCode, Kind::Diff, &Place::Here, None, None);
    let args = tool_args(&p, Kind::Diff, &strings(&["--", "a b/[x].rs"]));
    assert_eq!(args[0], "-c");
    assert!(args[1].starts_with("difftool.gitcraft-vscode.cmd="));
    assert_eq!(
        &args[2..],
        strings(&[
            "--literal-pathspecs",
            "difftool",
            "--tool=gitcraft-vscode",
            "-y",
            "--",
            "a b/[x].rs"
        ])
    );
    let merge = plan::configured("meld", Kind::Merge).map(|p| tool_args(&p, Kind::Merge, &[]));
    assert_eq!(
        merge,
        Ok(strings(&[
            "--literal-pathspecs",
            "mergetool",
            "--tool=meld",
            "-y"
        ]))
    );
}

#[test]
fn what_is_compared_becomes_diff_arguments() {
    let args = |t: DiffTarget| t.args();
    assert_eq!(
        args(DiffTarget::Unstaged {
            path: "dír/ñ ✓.txt".to_owned()
        }),
        Ok(strings(&["--", "dír/ñ ✓.txt"]))
    );
    assert_eq!(
        args(DiffTarget::Staged {
            path: "new".to_owned(),
            old_path: Some("old".to_owned())
        }),
        Ok(strings(&["--cached", "-M", "--", "new", "old"]))
    );
    assert_eq!(
        args(DiffTarget::Revisions {
            from: "abc".to_owned(),
            to: Some("def".to_owned()),
            path: "f".to_owned(),
            old_path: Some("f".to_owned()),
        }),
        Ok(strings(&["-M", "abc", "def", "--", "f"]))
    );
    assert_eq!(
        args(DiffTarget::Revisions {
            from: "abc".to_owned(),
            to: None,
            path: "f".to_owned(),
            old_path: None,
        }),
        Ok(strings(&["-M", "abc", "--", "f"]))
    );
    for bad in ["--output=/tmp/x", ""] {
        assert!(
            args(DiffTarget::Revisions {
                from: bad.to_owned(),
                to: None,
                path: "f".to_owned(),
                old_path: None,
            })
            .is_err()
        );
    }
}

#[test]
fn a_windows_program_is_found_where_its_installer_puts_it() {
    let env = |var: &str| match var {
        "ProgramFiles" => Some(r"C:\Program Files\".to_owned()),
        "ProgramFiles(x86)" => Some(r"C:\Program Files (x86)".to_owned()),
        "LOCALAPPDATA" => Some("  ".to_owned()),
        _ => None,
    };
    let found = windows_program(Preset::BeyondCompare, env, |p| {
        p == r"C:\Program Files (x86)\Beyond Compare 4\BComp.exe"
    });
    assert_eq!(
        found.as_deref(),
        Some(r"C:\Program Files (x86)\Beyond Compare 4\BComp.exe")
    );
    // The newest first.
    let found = windows_program(Preset::BeyondCompare, env, |p| p.contains("Beyond Compare"));
    assert_eq!(
        found.as_deref(),
        Some(r"C:\Program Files\Beyond Compare 5\BComp.exe")
    );
    // An empty folder variable isn't the root of the drive.
    assert_eq!(
        windows_program(Preset::VsCode, env, |p| p.starts_with(r"\")),
        None
    );
    assert_eq!(windows_program(Preset::Meld, |_| None, |_| true), None);
}

#[test]
fn a_choice_says_what_it_is() {
    let own = ToolChoice::default();
    assert_eq!(
        own.label(Kind::Merge),
        "the merge tool set in git (merge.tool)"
    );
    let preset = ToolChoice {
        preset: Some(Preset::WinMerge),
        program: String::new(),
    };
    assert_eq!(preset.label(Kind::Diff), "WinMerge");
}

#[cfg(unix)]
mod real {
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
            let dir =
                std::env::temp_dir().join(format!("gitcraft-tool-{name}-{}", std::process::id()));
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
    fn the_diff_tool_is_shown_each_kind_of_change() -> Result<(), String> {
        let repo = Repo::new("diff-kinds")?;
        let first = repo.commit(FILE, "one\n", "first")?;
        let second = repo.commit(FILE, "two\n", "second")?;
        let log = repo.diff_tool(0)?;
        let shown = |target: &DiffTarget| {
            open_diff(repo.path(), &configured(), target).map(|()| seen(&log))
        };

        // A commit: its parent against it; the first commit: nothing against it.
        let commit = commit_target(repo.path(), &second, FILE);
        let commit = commit.and_then(|t| shown(&t));
        assert_eq!(commit, Ok(("one\n".to_owned(), "two\n".to_owned())));
        let root = commit_target(repo.path(), &first, FILE).and_then(|t| shown(&t));
        assert_eq!(root, Ok((String::new(), "one\n".to_owned())));

        // Staged, then unstaged on top.
        repo.write(FILE, "three\n")?;
        repo.git(&["add", "--", FILE])?;
        repo.write(FILE, "four\n")?;
        let staged = DiffTarget::Staged {
            path: FILE.to_owned(),
            old_path: None,
        };
        assert_eq!(
            shown(&staged),
            Ok(("two\n".to_owned(), "three\n".to_owned()))
        );
        let unstaged = DiffTarget::Unstaged {
            path: FILE.to_owned(),
        };
        assert_eq!(
            shown(&unstaged),
            Ok(("three\n".to_owned(), "four\n".to_owned()))
        );

        // A comparison of a commit with the working tree.
        let range = range_target(repo.path(), &first, None, FILE);
        assert_eq!(shown(&range), Ok(("one\n".to_owned(), "four\n".to_owned())));
        Ok(())
    }

    #[test]
    fn a_renamed_file_is_shown_against_what_it_was() -> Result<(), String> {
        let repo = Repo::new("diff-rename")?;
        let text = "a line long enough to be found again after the rename\n".repeat(20);
        let first = repo.commit("old name.txt", &text, "first")?;
        repo.git(&["mv", "old name.txt", FILE])?;
        repo.write(FILE, &format!("{text}one more\n"))?;
        repo.git(&["add", "--", FILE])?;
        repo.git(&["commit", "-q", "-m", "renamed"])?;
        let renamed = repo.git(&["rev-parse", "HEAD"])?.trim().to_owned();
        let log = repo.diff_tool(0)?;
        let target = commit_target(repo.path(), &renamed, FILE);
        assert!(
            matches!(&target, Ok(DiffTarget::Revisions { old_path: Some(o), .. }) if o == "old name.txt"),
            "{target:?}"
        );
        let shown = target.and_then(|t| open_diff(repo.path(), &configured(), &t));
        assert_eq!(shown, Ok(()));
        assert_eq!(seen(&log), (text.clone(), format!("{text}one more\n")));
        let range = range_target(repo.path(), &first, Some(&renamed), FILE);
        assert!(matches!(
            range,
            DiffTarget::Revisions {
                old_path: Some(_),
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn a_diff_tool_that_fails_or_has_nothing_to_show_says_so() -> Result<(), String> {
        let repo = Repo::new("diff-fails")?;
        repo.commit(FILE, "one\n", "first")?;
        let unstaged = DiffTarget::Unstaged {
            path: FILE.to_owned(),
        };
        // No tool set anywhere.
        repo.write(FILE, "two\n")?;
        let none = open_diff(repo.path(), &configured(), &unstaged);
        assert!(
            none.as_ref()
                .is_err_and(|e| e.contains("No diff tool is set")),
            "{none:?}"
        );
        // Nothing changed.
        repo.diff_tool(0)?;
        repo.write(FILE, "one\n")?;
        let same = open_diff(repo.path(), &configured(), &unstaged);
        assert!(
            same.as_ref()
                .is_err_and(|e| e.contains("nothing to compare")),
            "{same:?}"
        );
        // The tool exits with an error: git ignores it (Beyond Compare exits so whenever the
        // files differ) unless `difftool.trustExitCode` says otherwise.
        repo.diff_tool(3)?;
        repo.write(FILE, "two\n")?;
        assert_eq!(open_diff(repo.path(), &configured(), &unstaged), Ok(()));
        repo.git(&["config", "difftool.trustExitCode", "true"])?;
        let failed = open_diff(repo.path(), &configured(), &unstaged);
        assert!(
            failed
                .as_ref()
                .is_err_and(|e| e.starts_with("fake could not show")),
            "{failed:?}"
        );
        // A preset that isn't installed.
        let missing = ToolChoice {
            preset: Some(Preset::Araxis),
            program: repo
                .path()
                .join("no such program")
                .to_string_lossy()
                .into_owned(),
        };
        let missing = open_diff(repo.path(), &missing, &unstaged);
        assert!(
            missing
                .as_ref()
                .is_err_and(|e| e.starts_with("Araxis Merge could not show")),
            "{missing:?}"
        );
        Ok(())
    }

    #[test]
    fn a_preset_runs_its_program_with_the_files() -> Result<(), String> {
        let repo = Repo::new("diff-preset")?;
        repo.commit(FILE, "one\n", "first")?;
        repo.write(FILE, "two\n")?;
        let args = repo.path().with_extension("args");
        // Records its arguments, one a line, and what the last one holds.
        let program = repo.tool(
            "preset",
            &format!(
                "for a; do printf '%s\\n' \"$a\"; done > '{0}'; eval last=\\${{$#}}; cat \"$last\" >> '{0}'",
                args.display()
            ),
        )?;
        let unstaged = DiffTarget::Unstaged {
            path: FILE.to_owned(),
        };
        for (preset, first) in [
            (Preset::BeyondCompare, None),
            (Preset::VsCode, Some("--wait")),
        ] {
            let choice = ToolChoice {
                preset: Some(preset),
                program: program.clone(),
            };
            assert_eq!(open_diff(repo.path(), &choice, &unstaged), Ok(()));
            let got = std::fs::read_to_string(&args).unwrap_or_default();
            let lines: Vec<&str> = got.lines().collect();
            if let Some(first) = first {
                assert_eq!(lines.first(), Some(&first), "{got}");
            }
            // The working tree's own file is the right side.
            assert!(lines.iter().any(|l| l.ends_with(FILE)), "{preset:?}: {got}");
            assert_eq!(lines.last(), Some(&"two"), "{got}");
        }
        Ok(())
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
        std::fs::write(&wslpath_file, format!("#!/bin/sh\n{wslpath}\n"))
            .map_err(|e| e.to_string())?;
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
}
