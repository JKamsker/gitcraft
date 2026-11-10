use super::*;

#[test]
fn explicit_origin_then_first_usable_and_no_usable_remote() {
    let hosts = vec![
        ("fork".into(), hosting("JKamsker")),
        ("origin".into(), hosting("Vancete")),
    ];
    let automatic = Selection::default();
    assert_eq!(
        automatic.resolve(&hosts).map(|(n, _)| n.as_str()),
        Some("origin")
    );
    let explicit = Selection {
        name: "fork".into(),
        url: String::new(),
    };
    assert_eq!(
        explicit.resolve(&hosts).map(|(_, h)| &h.path),
        Some(&"JKamsker/gitcraft".into())
    );
    assert_eq!(
        automatic.resolve(&hosts[..1]).map(|(n, _)| n.as_str()),
        Some("fork")
    );
    assert!(automatic.resolve(&[]).is_none());
}

#[test]
fn selection_follows_unique_rename_and_same_name_url_changes_but_removal_falls_back() {
    let mut choice = Selection {
        name: "fork".into(),
        url: remote("fork", "JKamsker").url,
    };
    choice.reconcile(&[remote("renamed", "JKamsker"), remote("origin", "Vancete")]);
    assert_eq!(choice.name, "renamed");
    choice.reconcile(&[remote("renamed", "other")]);
    assert_eq!(choice.url, remote("renamed", "other").url);
    choice.reconcile(&[remote("origin", "Vancete")]);
    assert_eq!(choice, Selection::default());
    choice = Selection {
        name: "gone".into(),
        url: remote("gone", "JKamsker").url,
    };
    choice.reconcile(&[remote("one", "JKamsker"), remote("two", "JKamsker")]);
    assert_eq!(
        choice,
        Selection::default(),
        "ambiguous rename must not guess"
    );
}

#[test]
fn preference_survives_context_serialization_and_is_per_repository() -> Result<(), String> {
    let ctx = egui::Context::default();
    let choice = Selection {
        name: "fork".into(),
        url: remote("fork", "JKamsker").url,
    };
    choice.store(&ctx, Path::new("/repo/one"));
    let encoded = ctx.memory(ron::to_string).map_err(|e| e.to_string())?;
    let restored = egui::Context::default();
    restored.memory_mut(|m| *m = ron::from_str(&encoded).unwrap_or_default());
    assert_eq!(Selection::load(&restored, Path::new("/repo/one")), choice);
    assert_eq!(
        Selection::load(&restored, Path::new("/repo/two")),
        Selection::default()
    );
    Ok(())
}
