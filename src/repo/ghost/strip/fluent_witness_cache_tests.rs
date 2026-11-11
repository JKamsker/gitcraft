use super::super::{Ghost, GhostOp, Key, PreviewFailure, Shown};
use super::*;
use crate::git::dry_run::{CANCELLED, Failure};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::Arc;

#[test]
fn fluent_partition_actual_cached_ghost_failure_repaints_typed_witness_without_jobs()
-> Result<(), String> {
    let ui_errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let ui_errors_0 = ui_errors.clone();

    let detail = "raw/ß; in a scratch clone: 日本語\n{ $detail } --argv=raw";
    let cases = [
        PreviewFailure::Backend(Failure::PartialClone),
        PreviewFailure::Backend(Failure::ReplayFallback {
            first: Box::new(Failure::ReplayNeedsBranch),
            scratch: Box::new(Failure::ScratchClone {
                detail: detail.into(),
            }),
        }),
        PreviewFailure::Backend(Failure::Opaque(detail.into())),
        PreviewFailure::Backend(Failure::Opaque("git replay needs a branch".into())),
        PreviewFailure::RawPlan("Invalid timeline commit ID".into()),
        PreviewFailure::from(detail.to_owned()),
    ];
    for failure in cases {
        let raw = failure.raw();
        let result = Arc::new(Err(failure.clone()));
        let retained = Arc::clone(&result);
        let key = Key {
            op: GhostOp::ResetHard {
                to: "refs/heads/raw/ß".into(),
            },
            refs: 1234,
        };
        let mut ghost = Ghost {
            jobs: 7,
            ..Default::default()
        };
        ghost.shown = Some(Shown {
            key: key.clone(),
            history: 29,
            result,
            view: None,
            swapped: false,
        });
        let mut installed = false;
        let mut h = Harness::new_ui_state(
            |ui, ghost: &mut Ghost| {
                let ui_result = (|| -> Result<(), String> {
                    if !installed {
                        crate::theme::install_fonts(ui.ctx());
                        installed = true;
                        ui.ctx().request_discard("install supplied fonts");
                        return Ok(());
                    }
                    let shown = ghost.shown.as_ref().ok_or("missing test value")?;
                    let failure = shown
                        .result
                        .as_ref()
                        .as_ref()
                        .err()
                        .ok_or("expected an error")?;
                    failed_lines(ui, &Tokens::get(ui.ctx()), failure);

                    Ok(())
                })();
                if let Err(error) = ui_result {
                    ui_errors_0.borrow_mut().push(error);
                }
            },
            ghost,
        );
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            h.run();
            h.get_by_label(&failure_text(&failure));
            let ghost = h.state();
            let shown = ghost.shown.as_ref().ok_or("missing test value")?;
            assert_eq!(shown.key, key);
            assert_eq!(shown.history, 29);
            assert_eq!(ghost.jobs, 7);
            assert!(ghost.job.is_none());
            assert!(ghost.cache.is_empty());
            assert!(Arc::ptr_eq(&shown.result, &retained));
            assert_eq!(
                shown
                    .result
                    .as_ref()
                    .as_ref()
                    .err()
                    .ok_or("expected an error")?
                    .raw(),
                raw
            );
            assert_eq!(
                shown
                    .result
                    .as_ref()
                    .as_ref()
                    .err()
                    .ok_or("expected an error")?,
                &failure
            );
            if matches!(failure, PreviewFailure::Backend(Failure::Opaque(_))) {
                assert_eq!(failure.display(), raw);
            }
            if let PreviewFailure::Backend(Failure::ReplayFallback { .. }) = &failure {
                assert!(failure.display().contains(detail));
            }
            if matches!(failure, PreviewFailure::Backend(Failure::PartialClone)) {
                assert_eq!(
                    failure.display(),
                    if language == "de" {
                        "Dies ist ein partieller Klon: Die Berechnung würde Dateien vom Remote herunterladen."
                    } else {
                        "this is a partial clone: working it out would download files from the remote"
                    }
                );
            }
        }
    }
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        assert!(PreviewFailure::Backend(Failure::Opaque(CANCELLED.into())).is_cancelled());
        assert!(PreviewFailure::from(CANCELLED.to_owned()).is_cancelled());
        assert!(!PreviewFailure::from("cancelled: provider detail".to_owned()).is_cancelled());
        assert!(
            !PreviewFailure::Backend(Failure::ReplayFallback {
                first: Box::new(Failure::Opaque(CANCELLED.into())),
                scratch: Box::new(Failure::Opaque("other".into()))
            })
            .is_cancelled()
        );
    }
    crate::i18n::set_language("en-US");

    if let Some(error) = ui_errors.borrow().first() {
        return Err(error.clone());
    }

    Ok(())
}
