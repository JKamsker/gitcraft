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
mod real;
