use super::Outcome;
use crate::git::{BundlePreview, BundleRef};
use crate::repo::Action;
use crate::theme::Tokens;
use egui::Ui;
use std::collections::BTreeSet;

pub(super) fn export(
    ui: &mut Ui,
    refs: &[BundleRef],
    selected: &mut BTreeSet<String>,
    base: &mut String,
    idle: bool,
) -> Outcome {
    ui.set_width(650.0);
    ui.heading("Export Git bundle");
    if refs.is_empty() {
        ui.label("This repository has no committed refs to export.");
    }
    ui.label("Choose refs from this snapshot. The file is saved on this machine, including when the repository is on SSH / WSL.");
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .id_salt("bundle_export_refs")
        .show(ui, |ui| {
            for r in refs {
                let mut on = selected.contains(&r.name);
                if ui
                    .checkbox(
                        &mut on,
                        format!("{} · {}", r.name, &r.oid[..8.min(r.oid.len())]),
                    )
                    .changed()
                {
                    if on {
                        selected.insert(r.name.clone());
                    } else {
                        selected.remove(&r.name);
                    }
                }
            }
        });
    ui.separator();
    ui.label("Optional range: exclude history reachable from this base commit");
    ui.text_edit_singleline(base);
    ui.label("Blank includes full reachable history. An incremental bundle requires the recipient to already have its prerequisite commits. Limit: 128 MiB; 2 minute Git deadline.");
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !selected.is_empty() && idle,
                egui::Button::new("Choose new output file…"),
            )
            .clicked()
        {
            outcome = Outcome::Done(Box::new(Action::ExportBundle {
                refs: refs
                    .iter()
                    .filter(|r| selected.contains(&r.name))
                    .cloned()
                    .collect(),
                base: base.clone(),
            }));
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

pub(super) fn incoming(ui: &mut Ui, label: &str, preview: &BundlePreview) -> Outcome {
    ui.set_width(720.0);
    let t = Tokens::get(ui.ctx());
    ui.heading("Inspect Git bundle");
    ui.label(format!("Local file snapshot: {label}"));
    ui.label("This preview does not import objects or refs. SSH / WSL verification uses a temporary copy on the repository machine.");
    if let Some(error) = &preview.verification_error {
        ui.colored_label(
            t.danger,
            format!("Header/prerequisite verification failed:\n{error}"),
        );
    } else {
        ui.label(
            "Git verified the header and prerequisites. Pack contents have not been validated.",
        );
    }
    ui.collapsing(
        format!("Prerequisites ({})", preview.prerequisites.len()),
        |ui| {
            if preview.prerequisites.is_empty() {
                ui.label("None: full bundle");
            }
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .id_salt("bundle_prereqs")
                .show(ui, |ui| {
                    for r in &preview.prerequisites {
                        ui.monospace(format!("{} {}", r.oid, r.name));
                    }
                });
        },
    );
    ui.label("Incoming refs:");
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .id_salt("bundle_preview_refs")
        .show(ui, |ui| {
            for r in &preview.refs {
                ui.monospace(format!("{} {}", r.oid, r.name));
            }
        });
    ui.label("Import is deferred until resource limits can safely contain unpacking untrusted data. Inspecting this file does not install its objects or refs.");
    if ui.button("Close").clicked() {
        Outcome::Cancel
    } else {
        Outcome::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_dialog_is_read_only_and_escape_cancels_without_ref_changes() -> Result<(), String> {
        use crate::git;
        use crate::repo::{Dialog, RepoTab};
        let root = std::env::temp_dir().join(format!("gitcraft-bundle-ui-{}", std::process::id()));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec![
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "one",
            ],
        ] {
            git::run(Some(&root), &args)?;
        }
        let before = git::run(
            Some(&root),
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        )?;
        let refs = git::bundle_refs(&root)?;
        let head = refs.iter().find(|r| r.name == "HEAD").ok_or("no HEAD")?;
        let preview = git::inspect_bundle(
            &root,
            git::export_bundle(&root, std::slice::from_ref(head), None)?,
        )?;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred(root.clone(), &ctx);
        tab.dialog = Some(Dialog::BundleIncoming {
            label: "selected.bundle".into(),
            preview: Box::new(preview),
        });
        let mut actions = Vec::new();
        let mut text = String::new();
        for frame in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                ..Default::default()
            };
            if frame == 2 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut actions));
            text.push_str(
                &output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t.galley.text()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            output.textures_delta.clear();
        }
        assert!(text.contains("Git verified the header"), "{text}");
        assert!(text.contains("Import is deferred"), "{text}");
        assert!(tab.dialog.is_none());
        assert!(actions.is_empty());
        assert_eq!(
            git::run(
                Some(&root),
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            )?,
            before
        );
        tab.bundle_done(
            "Import bundle refs".into(),
            Err("fatal: Authentication failed for untrusted prerequisite text".into()),
            crate::repo::Refresh::Status,
            &ctx,
        );
        assert!(tab.dialog.is_none());
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(text, error)| *error && text.contains("Authentication failed"))
        );
        Ok(())
    }
}
