use super::*;
use crate::package::witness::{CargoFailure, Failure, PackageJsonFailure};

#[test]
fn actual_finish_capture_retains_known_and_unknown_children_through_real_aggregation()
-> Result<(), String> {
    let missing = finish_witnessed("cargo-1234.5", false, b"", b"ignored", true)
        .err()
        .ok_or("missing")?;
    let failed = finish_witnessed("cargo", false, b"", b" \n", false)
        .err()
        .ok_or("empty stderr")?;
    let opaque = finish_witnessed(
        "cargo",
        false,
        b"",
        b"cargo failed; package.json: x; x",
        false,
    )
    .err()
    .ok_or("stderr")?;
    assert!(matches!(&opaque, ToolFailure::Opaque(_)));
    let same_bytes = finish_witnessed("cargo", false, b"", failed.raw().as_bytes(), false)
        .err()
        .ok_or("lookalike")?;
    assert_eq!(same_bytes.raw(), failed.raw());
    assert!(matches!(&same_bytes, ToolFailure::Opaque(_)));
    let compose = |tool| -> Result<Failure, String> {
        Ok(crate::package::tools::finish_packages(
            vec![],
            Some(CargoFailure::tool(tool)),
            Some(PackageJsonFailure::new(
                "npm said no package; package.json: x; x".into(),
            )),
        )
        .err()
        .ok_or("aggregate")?)
    };
    let known = compose(failed)?;
    let lookalike = compose(same_bytes)?;
    assert_eq!(known.raw(), lookalike.raw());
    let original = known.clone();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(
            known.display(),
            if locale == "de" {
                "cargo: cargo ist fehlgeschlagen; package.json: npm said no package; package.json: x; x"
            } else {
                known.raw()
            }
        );
        assert_eq!(lookalike.display(), lookalike.raw());
        assert_eq!(opaque.display(), opaque.raw());
        assert_eq!(known, original);
        assert_eq!(known.clone().into_raw(), original.raw());
        assert_eq!(missing.display().contains("cargo-1234.5"), true);
        assert_eq!(missing.display() == missing.raw(), locale == "en-US");
    }
    let timeout = timeout_failure("cargo-1234.5");
    let run = run_failure("npm-1234.5", "cargo failed; ; /1234.5".into());
    assert!(matches!(timeout, ToolFailure::Timeout { minutes: 5, .. }));
    assert_eq!(
        timeout.raw(),
        "cargo-1234.5 took longer than 5 minutes (waiting for a lock, or the network?): stopped"
    );
    assert_eq!(
        run.raw(),
        "Couldn't run npm-1234.5: cargo failed; ; /1234.5"
    );
    assert_eq!(
        finish("cargo", false, b"", b"", false)
            .err()
            .ok_or("raw wrapper")?,
        "cargo failed"
    );
    assert_eq!(
        finish_witnessed("cargo", true, b"/1234.5/ref/main\n", b"", false)
            .map_err(|error| format!("{}: {error:?}", "success"))?,
        "/1234.5/ref/main\n"
    );
    assert!(matches!(Failure::from(opaque), Failure::Opaque(_)));

    Ok(())
}
