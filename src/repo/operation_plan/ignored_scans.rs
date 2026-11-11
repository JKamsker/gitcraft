#[test]
fn selected_submodule_scope_is_literal_and_excludes_ignored_siblings() -> Result<(), String> {
    for path in [
        "a",
        "with space",
        "-leading",
        "[a]*",
        ":(glob)literal",
        "quote'path",
    ] {
        let action = crate::repo::keep_submodules::update_one(path);
        let crate::repo::Action::Ops { ops, .. } = action else {
            return Err("unexpected update action".into());
        };
        assert_eq!(ops[0].last(), Some(&format!(":(literal){path}")));
        for ignored in [
            Ok(String::new()),
            Ok("private\0".into()),
            Err("inspection failed".into()),
        ] {
            let mut calls = Vec::new();
            let result = super::super::check_ignored_submodule_update_with(&ops[0], |args, _| {
                calls.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                match args {
                    ["ls-files", "--stage", "-z", "--", selected] => {
                        assert_eq!(*selected, format!(":(literal){path}"));
                        Ok(format!("160000 {} 0\t{path}\0", "a".repeat(40)))
                    }
                    ["submodule", "foreach", "--quiet", "--recursive", script] => {
                        let quoted = crate::remote::sh_quote(path);
                        assert!(script.contains(&format!("{quoted}|{quoted}/*")));
                        assert!(script.contains("case \"$displaypath\""));
                        ignored.clone()
                    }
                    _ => Err(format!("unexpected inspection {args:?}")),
                }
            });
            assert_eq!(result.is_ok(), ignored.as_ref().is_ok_and(String::is_empty));
            assert_eq!(calls.len(), 2);
        }
    }
    Ok(())
}

#[test]
fn value_consuming_submodule_options_never_narrow_the_ignored_scan() -> Result<(), String> {
    for tail in [
        vec!["--recursive", "--reference", "--", "--force"],
        vec!["--reference=--", "--", "a"],
        vec!["--jobs", "2", "--", "a"],
        vec!["--unknown", "--", "a"],
    ] {
        let args: Vec<String> = [vec!["submodule", "update"], tail]
            .concat()
            .into_iter()
            .map(str::to_owned)
            .collect();
        for ignored in [
            Ok(String::new()),
            Ok("private\0".into()),
            Err("inspection failed".into()),
        ] {
            let mut calls = 0;
            let result = super::super::check_ignored_submodule_update_with(&args, |args, _| {
                calls += 1;
                assert_eq!(
                    args,
                    [
                        "submodule",
                        "foreach",
                        "--quiet",
                        "--recursive",
                        "git ls-files --others --ignored --exclude-standard --directory -z"
                    ]
                );
                ignored.clone()
            });
            assert_eq!(calls, 1);
            assert_eq!(result.is_ok(), ignored.as_ref().is_ok_and(String::is_empty));
        }
    }
    Ok(())
}

#[test]
fn recursive_pull_inspects_nested_ignored_files_before_any_mutation() -> Result<(), String> {
    for (tail, configured, scans) in [
        (vec!["--ff-only"], "true\n", true),
        (vec!["--rebase"], "true\n", true),
        (vec!["--recurse-submodules"], "false\n", true),
        (vec!["--recurse-submodules=on-demand"], "false\n", true),
        (vec!["--recurse-submodules=FALSE"], "true\n", false),
        (vec!["--recurse-submodules=2"], "false\n", true),
        (vec!["--recurse-submodules="], "true\n", false),
        (
            vec!["--recurse-submodules", "--no-recurse-submodules"],
            "true\n",
            false,
        ),
        (
            vec!["--no-recurse-submodules", "--recurse-submodules=yes"],
            "false\n",
            true,
        ),
        (vec!["--strategy", "--recurse-submodules"], "false\n", false),
        (vec!["--", "--recurse-submodules"], "false\n", false),
    ] {
        for inspection in [
            Ok(String::new()),
            Ok("nested/private\0".into()),
            Err("inspection failed".into()),
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
                    ["config", "--bool", "--get", "submodule.recurse"] => Ok(configured.into()),
                    [
                        "submodule",
                        "foreach",
                        "--quiet",
                        "--recursive",
                        "git ls-files --others --ignored --exclude-standard --directory -z",
                    ] => inspection.clone(),
                    ["symbolic-ref", "-q", "HEAD"] => Ok("refs/heads/main\n".into()),
                    ["config", ..] | ["ls-files", ..] => Ok(String::new()),
                    _ => Err(format!("unexpected command: {argv:?}")),
                }
            });
            assert_eq!(
                result.is_ok(),
                !scans || inspection.as_ref().is_ok_and(String::is_empty),
                "{args:?}, {inspection:?}: {result:?}"
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| c.starts_with("submodule foreach"))
                    .count(),
                usize::from(scans)
            );
            assert!(
                calls
                    .iter()
                    .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
            );
            if let Ok(prepared) = result {
                assert_eq!(prepared[2..], args);
            }
        }
    }
    Ok(())
}
