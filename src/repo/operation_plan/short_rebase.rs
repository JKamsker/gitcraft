use super::*;
#[test]
fn saved_prefixed_pulls_are_refused_by_dispatch_before_preparation() -> Result<(), String> {
    for argv in [
        vec!["-c", "pull.rebase=true", "pull"],
        vec!["-cpull.rebase=true", "pull"],
        vec!["--config-env", "pull.rebase=SETTING", "pull"],
        vec!["--config-env=pull.rebase=SETTING", "pull"],
        vec!["-C", "other", "pull"],
        vec!["--git-dir", "elsewhere", "pull"],
        vec!["--work-tree=elsewhere", "pull"],
        vec!["--unknown", "pull"],
        vec!["-c"],
    ] {
        let saved: UndoEntry = serde_json::from_value(serde_json::json!({
            "label": "Pull", "undo": [], "redo": [argv], "heads": null
        }))
        .map_err(|e| e.to_string())?;
        let (result, _, stopped) = super::super::run_steps_counted(
            Path::new("/unopened-recording-repository"),
            &saved.redo,
            OpKind::Redo(saved.clone()),
            None,
            true,
            None,
        );
        assert!(
            result.is_err_and(|e| e.contains("run") || e.contains("Run")),
            "{argv:?}"
        );
        assert_eq!(stopped, 0);
    }
    let mut reads = Vec::new();
    for argv in [
        ops(&[&["pull", "--ff-only"]]),
        ops(&[&["-c", "x=y", "rebase", "--continue"]]),
    ] {
        super::super::check_command_envelopes(&argv, true, &mut |args| {
            reads.push(args.join(" "));
            Ok("pull\nrebase\n".into())
        })?;
    }
    assert_eq!(reads, ["--list-cmds=builtins", "--list-cmds=builtins"]);
    assert!(
        super::super::check_command_envelopes(&ops(&[&["saved-pull-alias"]]), true, &mut |_| Ok(
            "pull\n".into()
        ))
        .is_err()
    );
    Ok(())
}

#[test]
fn redo_route_expectations_reach_proof_validation_and_submodule_prefixes_refuse()
-> Result<(), String> {
    let binding = super::recorded_route()?;
    let proof = vec![
        super::super::EXPECT_HTTPS_ROUTE.into(),
        serde_json::to_string(&binding).map_err(|e| e.to_string())?,
    ];
    for command in ["pull", "fetch"] {
        let steps = vec![proof.clone(), vec![command.into(), "--quiet".into()]];
        let mut reads = Vec::new();
        super::super::check_command_envelopes(&steps, true, &mut |args| {
            reads.push(args.join(" "));
            Ok("pull\nfetch\n".into())
        })?;
        assert_eq!(reads, ["--list-cmds=builtins"]);
        let mut verified = 0;
        super::super::check_https_routes(&steps, |actual| {
            assert_eq!(actual, &binding);
            verified += 1;
            Ok(())
        })?;
        assert_eq!(verified, 1);
        assert!(super::super::check_https_routes(&steps, |_| Err("route changed".into())).is_err());
    }
    for argv in [
        vec!["submodule", "--quiet", "update", "--init", "--recursive"],
        vec!["submodule", "-q", "update"],
        vec!["submodule", "--cached", "update"],
        vec!["-c", "submodule.recurse=true", "submodule", "update"],
        vec!["-C", "elsewhere", "submodule", "--quiet", "update"],
        vec!["--unknown", "pull"],
    ] {
        let saved: UndoEntry = serde_json::from_value(serde_json::json!({
            "label":"Replay", "undo":[], "redo":[argv], "heads":null
        }))
        .map_err(|e| e.to_string())?;
        let (result, _, stopped) = super::super::run_steps_counted(
            Path::new("/unopened-submodule-envelope-fixture"),
            &saved.redo,
            OpKind::Redo(saved.clone()),
            None,
            true,
            None,
        );
        assert!(
            result.is_err_and(|e| e.contains("Run") || e.contains("run")),
            "{argv:?}"
        );
        assert_eq!(stopped, 0);
    }
    super::super::check_command_envelopes(
        &ops(&[&["submodule", "update", "--quiet", "--recursive"]]),
        true,
        &mut |_| Ok("submodule\n".into()),
    )?;
    Ok(())
}

#[test]
fn prefixed_and_try_stash_commands_refuse_before_any_operation_step() -> Result<(), String> {
    for command in [
        vec!["-c", "advice.statusHints=false", "stash", "push", "-u"],
        vec!["-cadvice.statusHints=false", "stash", "push"],
        vec!["-C", "elsewhere", "stash", "push"],
        vec!["-Celsewhere", "stash", "push"],
        vec!["--git-dir=elsewhere", "stash", "push"],
        vec!["--work-tree", "elsewhere", "stash", "push"],
        vec!["--config-env=x=SETTING", "stash", "push"],
        vec![super::super::TRY, "optional stash", "stash", "push"],
        vec![
            super::super::TRY,
            "optional stash",
            "-c",
            "x=y",
            "stash",
            "push",
        ],
    ] {
        let steps = ops(&[&["reset", "--hard"], &command]);
        let saved: UndoEntry = serde_json::from_value(serde_json::json!({
            "label":"Stash", "undo":[], "redo":steps, "heads":null
        }))
        .map_err(|e| e.to_string())?;
        for kind in [OpKind::plain(), OpKind::Redo(saved.clone())] {
            let (result, _, stopped) = super::super::run_steps_counted(
                Path::new("/unopened-stash-envelope-fixture"),
                &saved.redo,
                kind,
                None,
                true,
                None,
            );
            assert!(
                result.is_err_and(|e| e.contains("checked dispatch") || e.contains("Stash again")),
                "{command:?}"
            );
            assert_eq!(stopped, 0);
        }
    }
    for command in [
        vec!["stash", "push", "--include-untracked"],
        vec!["-c", "x=stash", "status"],
        vec!["-C", "stash", "status"],
    ] {
        super::super::check_command_envelopes(&ops(&[&command]), true, &mut |_| {
            Ok("stash\nstatus\n".into())
        })?;
    }
    Ok(())
}

#[test]
fn classifier_consumes_git_options_and_uses_last_mode() -> Result<(), String> {
    for (tail, rebase, ff_only) in [
        (vec![], None, false),
        (vec!["--compact-summary", "-r"], Some(true), false),
        (vec!["--no-compact-summary", "--ff-only"], None, true),
        (vec!["-r"], Some(true), false),
        (vec!["-rtrue"], Some(true), false),
        (vec!["-rmerges"], Some(true), false),
        (vec!["-rm"], Some(true), false),
        (vec!["-qr"], Some(true), false),
        (vec!["-rfalse"], Some(false), false),
        (vec!["-qrfalse"], Some(false), false),
        (vec!["-r0"], Some(false), false),
        (vec!["-rno"], Some(false), false),
        (vec!["-roff"], Some(false), false),
        (vec!["--rebase="], Some(false), false),
        (vec!["-r2"], Some(true), false),
        (vec!["-r00"], Some(false), false),
        (vec!["-r0x0"], Some(false), false),
        (vec!["-r1k"], Some(true), false),
        (vec!["-r-2"], Some(true), false),
        (vec!["-r", "--no-rebase"], Some(false), false),
        (vec!["--no-rebase", "-qr"], Some(true), false),
        (vec!["-rfalse", "--rebase=merges"], Some(true), false),
        (vec!["--rebase=false", "-rm"], Some(true), false),
        (vec!["-r", "--rebase=false"], Some(false), false),
        (vec!["-srecursive"], None, false),
        (vec!["-qs", "--rebase"], None, false),
        (vec!["-X", "-r"], None, false),
        (vec!["-o", "--ff-only", "-r"], Some(true), false),
        (vec!["-Sr", "-jr", "-Xours"], None, false),
        (vec!["--strategy", "--rebase", "-r"], Some(true), false),
        (vec!["--upload-pack", "-r"], None, false),
        (vec!["--strategy-option=-r"], None, false),
        (vec!["--gpg-sign=r", "--jobs=r"], None, false),
        (vec!["--recurse-submodules", "-r"], Some(true), false),
        (vec!["-r", "false"], Some(true), false),
        (vec!["--rebase", "false"], Some(true), false),
        (vec!["origin", "main", "-r"], Some(true), false),
        (vec!["--", "-r", "--ff-only"], None, false),
        (vec!["--end-of-options", "-r", "--ff-only"], None, false),
        (vec!["--reb"], Some(true), false),
        (vec!["--reb=merges"], Some(true), false),
        (
            vec!["--server-option", "--no-rebase", "-r"],
            Some(true),
            false,
        ),
        (vec!["-r", "--", "--no-rebase"], Some(true), false),
        (vec!["-r", "--ff-only"], Some(true), true),
        (vec!["--ff-only", "--ff", "-r"], Some(true), false),
        (vec!["--ff-only", "--no-ff", "-r"], Some(true), false),
        (vec!["--reb=m"], Some(true), false),
    ] {
        let args = [
            vec!["pull".to_owned()],
            tail.iter().map(|s| (*s).to_owned()).collect(),
        ]
        .concat();
        let mode = super::super::pull_mode(&args)?;
        assert_eq!((mode.rebase, mode.ff_only), (rebase, ff_only), "{args:?}");
    }
    for option in [
        "cleanup",
        "strategy",
        "strategy-option",
        "upload-pack",
        "depth",
        "shallow-since",
        "shallow-exclude",
        "deepen",
        "refmap",
        "server-option",
        "negotiation-tip",
    ] {
        let args = vec![
            "pull".into(),
            format!("--{option}"),
            "--ff-only".into(),
            "-r".into(),
        ];
        assert_eq!(
            super::super::pull_mode(&args)?.rebase,
            Some(true),
            "{args:?}"
        );
        assert!(!super::super::pull_mode(&args)?.ff_only, "{args:?}");
    }
    for tail in [
        vec!["-r=false"],
        vec!["-rq"],
        vec!["-qrq"],
        vec!["--rebase=bogus"],
        vec!["--no-rebase=true"],
        vec!["--strategy"],
        vec!["-X"],
        vec!["--str"],
        vec!["--re"],
        vec!["-r08"],
        vec!["-r++2"],
        vec!["-r-+2"],
        vec!["-r2147483648"],
    ] {
        let args = [
            vec!["pull".to_owned()],
            tail.iter().map(|s| (*s).to_owned()).collect(),
        ]
        .concat();
        assert!(super::super::pull_mode(&args).is_err(), "{args:?}");
    }
    Ok(())
}

#[test]
fn native_autostash_cleanup_refusal_precedes_all_pull_preparation() {
    for mode in [
        "--ff-only",
        "--no-rebase",
        "--rebase",
        "--autostash",
        "--no-autostash",
    ] {
        let args = vec!["pull".into(), mode.into()];
        let calls = std::cell::Cell::new(0);
        let result = super::super::checked_pull_with(
            &args,
            || Err("excluded replacement".into()),
            |_, _| {
                calls.set(calls.get() + 1);
                Err("unexpected preparation".into())
            },
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(result, Err("excluded replacement".into()));
    }
}

#[test]
fn recording_saved_redo_refuses_before_pull_or_fetch_dispatch() -> Result<(), String> {
    for option in ["-r", "-rtrue", "-rmerges", "-rm", "-qr"] {
        let saved: UndoEntry = serde_json::from_value(serde_json::json!({
            "label": "Pull", "undo": [], "redo": [["pull", option]], "heads": null
        }))
        .map_err(|e| e.to_string())?;
        let mut calls = Vec::new();
        let result = super::super::ignored_safe_pull_with(&saved.redo[0], |args, _| {
            calls.push(args.join(" "));
            match args[0] {
                "config" if args.last() == Some(&"submodule.recurse") => Ok(String::new()),
                "symbolic-ref" => Ok("refs/heads/main\n".into()),
                "ls-files" => Ok("private\0".into()),
                _ => Err(format!("Unexpected command: {args:?}")),
            }
        });
        assert!(
            result.is_err_and(|e| e.contains("ignored local files")),
            "{option}"
        );
        assert_eq!(calls.len(), 3);
        assert!(
            calls
                .iter()
                .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
        );
    }
    Ok(())
}

#[test]
fn recording_config_fallback_and_overrides_preserve_ignored_files() -> Result<(), String> {
    for (tail, local, global, refused) in [
        (vec![], "", "true\0", true),
        (vec![], "false\0", "true\0", false),
        (vec!["--", "--no-rebase"], "", "true\0", true),
        (vec!["--strategy", "--no-rebase"], "", "true\0", true),
        (vec!["-srecursive"], "", "true\0", true),
        (vec!["-rfalse"], "", "true\0", false),
        (
            vec!["--compact-summary", "--no-rebase"],
            "",
            "true\0",
            false,
        ),
        (
            vec!["--no-compact-summary", "--no-rebase"],
            "",
            "true\0",
            false,
        ),
        (vec!["-qrfalse"], "", "true\0", false),
        (vec!["--no-rebase", "-r"], "false\0", "", true),
        (vec!["-r", "--ff-only"], "", "true\0", false),
        (vec!["--ff-only", "--ff", "-r"], "", "", true),
        (vec![], "", "", false),
    ] {
        let args = [
            vec!["pull".to_owned()],
            tail.iter().map(|s| (*s).to_owned()).collect(),
        ]
        .concat();
        let mut calls = Vec::new();
        let result = super::super::ignored_safe_pull_with(&args, |argv, _| {
            calls.push(argv.join(" "));
            match argv {
                ["config", "--bool", "--get", "submodule.recurse"] => Ok(String::new()),
                ["symbolic-ref", "-q", "HEAD"] => Ok("refs/heads/main\n".into()),
                ["config", "-z", "--get", "branch.main.rebase"] => Ok(local.into()),
                ["config", "-z", "--get", "pull.rebase"] => Ok(global.into()),
                ["config", "-z", "--get", "pull.ff"] => Ok(String::new()),
                ["config", "-z", "--get", "branch.main.mergeOptions"] => Ok("--log=3\0".into()),
                ["ls-files", ..] => Ok("private\0".into()),
                _ => Err(format!("Unexpected command: {argv:?}")),
            }
        });
        if refused {
            assert!(
                result.is_err_and(|e| e.contains("ignored local files")),
                "{args:?}"
            );
        } else {
            let prepared = result?;
            assert_eq!(
                prepared[1],
                "branch.main.mergeOptions=--log=3 --no-overwrite-ignore"
            );
            assert_eq!(prepared[2..], args);
        }
        assert!(
            calls
                .iter()
                .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
        );
    }
    Ok(())
}

#[test]
fn configured_ff_only_respects_explicit_pull_option_precedence() -> Result<(), String> {
    for (tail, ff, allowed) in [
        (vec![], "only\0", true),
        (vec![], "true\0", false),
        (vec![], "ONLY\0", false),
        (vec![], "only \0", false),
        (vec!["--rebase"], "only\0", false),
        (vec!["--ff"], "only\0", false),
        (vec!["--no-ff"], "only\0", false),
        (vec!["--ff-only", "--ff"], "only\0", false),
        (vec!["--ff", "--ff-only"], "only\0", true),
        (vec!["--no-rebase"], "only\0", true),
        (vec!["--strategy", "--ff-only"], "only\0", true),
        (vec!["--", "--rebase"], "only\0", true),
    ] {
        let args = [
            vec!["pull".to_owned()],
            tail.iter().map(|s| s.to_string()).collect(),
        ]
        .concat();
        let mut calls = Vec::new();
        let result = super::super::ignored_safe_pull_with(&args, |argv, _| {
            calls.push(argv.join(" "));
            match argv {
                ["config", "--bool", "--get", "submodule.recurse"] => Ok(String::new()),
                ["symbolic-ref", "-q", "HEAD"] => Ok("refs/heads/main\n".into()),
                ["config", "-z", "--get", "pull.ff"] => Ok(ff.into()),
                ["config", "-z", "--get", "branch.main.rebase"] => Ok("true\0".into()),
                ["config", "-z", "--get", "branch.main.mergeOptions"] => Ok(String::new()),
                ["ls-files", ..] => Ok("private\0".into()),
                _ => Err(format!("Unexpected read: {argv:?}")),
            }
        });
        assert_eq!(result.is_ok(), allowed, "{args:?}, {ff:?}: {result:?}");
        if let Ok(prepared) = result {
            assert_eq!(prepared[2..], args);
            assert!(prepared[1].ends_with("--no-overwrite-ignore"));
        }
        assert!(
            calls
                .iter()
                .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
        );
    }
    assert!(
        super::super::ignored_safe_pull_with(&ops(&[&["pull"]])[0], |argv, _| {
            match argv[0] {
                "symbolic-ref" => Ok("refs/heads/main\n".into()),
                _ => Err("config read failed".into()),
            }
        })
        .is_err()
    );
    Ok(())
}
