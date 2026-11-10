#[test]
fn review20_saved_checkout_guard_preserves_authority_prefix_and_gitlab_case()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    for forge in Forge::ALL {
        let w = World::new(&format!("review20-saved-identity-{}", forge.key()));
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            web: "https://forge.example:4000/proxy".into(),
            path: "Group/Repo".into(),
        };
        run(
            &w.me,
            &[
                "remote",
                "add",
                "ann",
                "https://forge.example:4000/proxy/Group/Repo.git",
            ],
        );
        for (url, valid) in [
            ("https://forge.example:4000/proxy/Group/Repo.git", true),
            ("ssh://git@forge.example:2222/Group/Repo.git", true),
            ("https://forge.example:3000/proxy/Group/Repo.git", false),
            ("https://forge.example:4000/Group/Repo.git", false),
            ("https://forge.example:4000/PROXY/Group/Repo.git", false),
            (
                "https://forge.example:4000/proxy/group/repo.git",
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
            ),
        ] {
            run(&w.me, &["remote", "set-url", "ann", url]);
            let guard = super::guard::CheckoutDestination::new("ann", url, false, &h).step();
            let steps = vec![
                guard,
                vec!["config".into(), "gitcraft.fixtureRan".into(), "true".into()],
            ];
            let steps: Vec<Vec<String>> = serde_json::from_str(&serde_json::to_string(&steps)?)?;
            let result = run_steps(&w.me, &steps, OpKind::plain(), None).0;
            assert_eq!(result.is_ok(), valid, "{forge:?} {url}: {result:?}");
            let ran = git::run(Some(&w.me), &["config", "--get", "gitcraft.fixtureRan"]);
            assert_eq!(ran.is_ok(), valid);
            if valid {
                run(&w.me, &["config", "--unset", "gitcraft.fixtureRan"]);
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_legacy_saved_checkout_without_identity_is_refused_before_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-legacy-checkout");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let mut entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    // Reconstruct the old on-disk record: it had the action label and plain commands.
    entry.label = "Check out #7 to push to ann".into();
    let mut legacy = entry.clone();
    for ops in [&mut legacy.undo, &mut legacy.redo] {
        ops.retain(|s| s.first().is_none_or(|m| m != "\0fork-checkout"));
    }
    let legacy = persisted(legacy)?;
    let head = git::head(&w.me);
    let result = run_steps(
        &w.me,
        &legacy.undo.clone(),
        OpKind::Undo(legacy.clone()),
        None,
    )
    .0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::head(&w.me), head);
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(undone) = done else {
        return Err("missing guarded undo result".into());
    };
    entry = persisted(undone)?;
    let before = git::head(&w.me);
    let result = run_steps(&w.me, &legacy.redo.clone(), OpKind::Redo(legacy), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::head(&w.me), before);
    assert!(git::remotes(&w.me)?.iter().all(|r| r.name != "ann"));
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(result.is_ok(), "{result:?}");
    Ok(())
}

#[test]
fn invalidated_fork_scope_keeps_raw_guard_while_error_paint_tracks_locale() -> Result<(), String> {
    let hosting = crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".into(),
        path: "opaque/project".into(),
        web: "https://github.com".into(),
    };
    let scope = super::guard::Scope::new(
        1234,
        hosting,
        git::Remote {
            name: "opaque 日本語".into(),
            url: "https://github.com/opaque/project".into(),
        },
    );
    scope.check()?;
    scope.invalidate();
    let raw = scope.check().err().ok_or("invalidated scope accepted")?;
    let expected = "The primary forge changed; plan this fork action again. Any completed local rewrite remains undoable.";
    assert_eq!(raw, expected);
    for (language, display) in [
        ("en-US", expected),
        (
            "de",
            "Die primäre Forge wurde geändert; plane diese Fork-Aktion erneut. Abgeschlossene lokale Umschreibungen bleiben rückgängig machbar.",
        ),
        ("en-US", expected),
    ] {
        crate::i18n::set_language(language);
        assert_eq!(git::display_error(&raw), display);
        assert_eq!(scope.check(), Err(expected.to_owned()));
        let opaque = format!("provider log: {expected}\ncommand: refs/日本語");
        assert_eq!(git::display_error(&opaque), opaque);
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
