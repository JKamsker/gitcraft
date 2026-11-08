use super::*;

#[test]
fn actual_restore_and_placement_combinators_retain_previous_identity_and_repair_paths() {
    let old = Path::new("/1234.5/old; ;");
    let target = Path::new("/1234.5/ref/main; ;");
    let detail = "Could not copy the new version; ; the version kept";
    for version in [
        None,
        Some("1234.5-dev.6789".into()),
        Some("the version kept".into()),
    ] {
        let previous = Previous {
            version: version.clone(),
            path: PathBuf::from("unused"),
        };
        let single = restore_failure(&previous, detail.into(), old, target, false);
        let both = restore_failure(&previous, detail.into(), old, target, true);
        assert!(
            matches!(&single, Failure::RestorePrevious { previous: saved, .. } if saved == &version)
        );
        assert!(matches!(&both, Failure::RestoreBoth { previous: saved, .. } if saved == &version));
        let raw = format!(
            "Could not put gitcraft {} back in place ({detail}), nor this version: it is at {}; rename it back to {} by hand",
            previous.label(),
            old.display(),
            target.display()
        );
        assert_eq!(both.raw(), raw);
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(both.clone().into_raw(), raw);
            assert!(both.display().contains(detail));
            assert!(both.display().contains(&old.display().to_string()));
            assert!(both.display().contains(&target.display().to_string()));
            assert_eq!(single.display() == single.raw(), locale == "en-US");
            let label = match (&version, locale) {
                (None, "de") => "die aufbewahrte Version",
                (None, _) => "the version kept",
                (Some(version), _) => version,
            };
            assert!(single.display().contains(label));
        }
    }
    let missing = no_previous();
    let aside = move_aside_failure(target, detail.into());
    assert_eq!(missing.raw(), "No previous version of gitcraft is kept");
    assert_eq!(
        aside.raw(),
        format!("Could not move {} aside: {detail}", target.display())
    );
}
