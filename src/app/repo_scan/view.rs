//! Repository-scan progress, results, and user requests.

use super::*;

/// What the dialog says of how the scan ended, when it didn't look everywhere.
pub(super) fn stopped_message(stopped: Stopped, found: usize) -> String {
    match stopped {
        Stopped::Repos => {
            crate::i18n::message("interaction-scan-repo-limit", &[("found", found.into())])
        }
        Stopped::Folders => crate::i18n::message(
            "interaction-scan-folder-limit",
            &[("folders", LIMITS.max_dirs.into())],
        ),
        Stopped::Asked => {
            crate::i18n::message("interaction-scan-asked", &[("found", found.into())])
        }
    }
}

/// The progress, the error, or the repositories found with a button each to open it.
pub(super) fn scan_body(ui: &mut egui::Ui, scan: &RepoScan, t: &Tokens) -> Option<ScanClick> {
    let found = match &scan.result {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                let p = scan.progress;
                ui.label(crate::i18n::message(
                    "interaction-scan-progress",
                    &[
                        ("folders", p.folders.into()),
                        ("repositories", p.repos.into()),
                    ],
                ));
            });
            return None;
        }
        Some(Err(e)) => {
            ui.colored_label(
                t.danger,
                display_error(e).unwrap_or_else(|| crate::git::display_error(e)),
            );
            return None;
        }
        Some(Ok(found)) => found,
    };
    if found.repos.is_empty() {
        let said = match found.stopped {
            Some(stopped) => stopped_message(stopped, 0),
            None => {
                crate::i18n::message("interaction-scan-empty", &[("depth", LIMITS.depth.into())])
            }
        };
        ui.label(said);
        return None;
    }
    let mut click = None;
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for repo in &found.repos {
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::i18n::text("interaction-scan-open"))
                        .clicked()
                    {
                        click = Some(ScanClick::Open(repo.clone()));
                    }
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(super::super::home_filter::repo_name(repo))
                                .font(theme::medium(12.5)),
                        );
                        ui.label(
                            RichText::new(crate::remote::display(repo))
                                .color(t.text_faint)
                                .small(),
                        );
                    });
                });
                ui.add_space(2.0);
            }
        });
    if let Some(stopped) = found.stopped {
        ui.add_space(4.0);
        ui.label(
            RichText::new(stopped_message(stopped, found.repos.len()))
                .color(t.text_faint)
                .small(),
        );
    }
    click
}
