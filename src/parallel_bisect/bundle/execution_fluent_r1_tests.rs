use super::completed;
use crate::parallel_bisect::display::{CleanupContext, cleanup_diagnostic, diagnostic};

#[test]
fn actual_completed_results_need_separate_cleanup_context_for_presentation() -> Result<(), String> {
    // Only this pure Result combinator is called: no backend, workspace, Git or helper.
    for (locale, statement) in [
        ("en-US", "quarantine cleanup failed"),
        ("de", "die Quarantäne konnte nicht bereinigt werden"),
    ] {
        crate::i18n::set_language(locale);
        for primary in [
            "Contained Git failed (1): object denied",
            "Contained Git failed (1): object denied; quarantine cleanup failed: provider detail (exit 7); stderr: raw\n",
        ] {
            let cleanup =
                "cleanup denied; quarantine cleanup failed: provider detail\n$HOME {detail}";
            for import in [false, true] {
                let result = completed(Err(primary.into()), Err(cleanup.into()), import);
                let raw = result.err().ok_or("expected combined failure")?;
                assert_eq!(
                    raw,
                    format!("{primary}; quarantine cleanup failed: {cleanup}")
                );
                let shown =
                    cleanup_diagnostic(&raw, CleanupContext::BundleCompleted { primary, cleanup });
                assert_eq!(shown, format!("{primary}; {statement}: {cleanup}"));
                assert!(shown.contains(primary) && shown.contains(cleanup));
                // Rendering borrows the error; the original Result payload is retained.
                assert_eq!(
                    raw,
                    format!("{primary}; quarantine cleanup failed: {cleanup}")
                );
            }
        }
        for unknown in [
            "provider text; quarantine cleanup failed: provider detail",
            "provider text; workspace cleanup: provider detail",
            "provider text; owned pack-01234567.keep retained",
        ] {
            let raw = completed(Err(unknown.into()), Ok(()), true)
                .err()
                .ok_or("expected propagated primary")?;
            assert_eq!(raw, unknown);
            assert_eq!(diagnostic(&raw), unknown);
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
