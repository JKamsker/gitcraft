//! Release postcards use immutable Git ranges and an explicit disclosure preview before local PNG publication.
mod export;
mod render;
use super::{RepoTab, history_reader::Reader};
use crate::git::{self, Commit};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
#[derive(Clone)]
struct Snapshot {
    name: String,
    date: String,
    range: String,
    commits: Vec<Commit>,
    messages: Vec<String>,
    contributors: Vec<String>,
    anonymized: bool,
}
fn clean(value: &str, max: usize) -> String {
    let mut chars = value.chars().filter(|c|!c.is_control() && !matches!(c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}'));
    let mut text: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        text.push('…');
    }
    text
}
impl Snapshot {
    fn new(state: &State, commits: &[Commit]) -> Self {
        let names: BTreeSet<_> = commits.iter().map(|c| c.author.as_str()).collect();
        let aliases: BTreeMap<_, _> = names.iter().enumerate().map(|(i, n)| (*n, i + 1)).collect();
        let contributors = if state.include_names {
            names
                .iter()
                .map(|n| {
                    if state.anonymize {
                        crate::i18n::message(
                            "repo-workflows-postcard-contributor",
                            &[("index", (aliases[n]).into())],
                        )
                    } else {
                        let label = clean(n, 256);
                        if label.ends_with('…') {
                            crate::i18n::message(
                                "repo-workflows-postcard-shortened-contributor",
                                &[
                                    ("label", (label.as_str()).into()),
                                    ("index", (aliases[n]).into()),
                                ],
                            )
                        } else {
                            label
                        }
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let messages = commits
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if state.anonymize {
                    crate::i18n::message(
                        "repo-workflows-postcards-new-53bf7c37",
                        &[("i-1-1", (i + 1).clone().into())],
                    )
                } else if state.include_messages {
                    clean(&c.summary, 96)
                } else {
                    crate::i18n::message(
                        "repo-workflows-postcards-new-53bf7c37",
                        &[("i-1-1", (i + 1).clone().into())],
                    )
                }
            })
            .collect();
        // Render input contains no private names/messages when anonymization is selected.
        let commits = commits
            .iter()
            .map(|c| Commit {
                hash: c.hash.clone(),
                parents: c.parents.clone(),
                ..Default::default()
            })
            .collect();
        Self {
            name: clean(&state.name, 80),
            date: clean(&state.date, 32),
            range: if state.base.is_empty() {
                crate::i18n::message(
                    "repo-workflows-postcards-new-aa1252d7",
                    &[("state-target-1", (state.target).clone().into())],
                )
            } else {
                format!("{}..{}", state.base, state.target)
            },
            commits,
            messages,
            contributors,
            anonymized: state.anonymize,
        }
    }
}
enum Answer {
    Range(Vec<Commit>),
    Preview(Snapshot, egui::ColorImage, Vec<u8>),
    Saved,
}
struct Preview {
    snapshot: Snapshot,
    texture: egui::TextureHandle,
    png: Arc<Vec<u8>>,
}
pub(super) struct State {
    open: bool,
    reader: Reader<Answer>,
    context: Option<Option<std::path::PathBuf>>,
    base: String,
    target: String,
    name: String,
    date: String,
    include_names: bool,
    include_messages: bool,
    anonymize: bool,
    commits: Option<Vec<Commit>>,
    preview: Option<Preview>,
    status: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            reader: Default::default(),
            context: None,
            base: String::new(),
            target: String::new(),
            name: crate::i18n::text("repo-workflows-postcards-default-f952fee7").into(),
            date: String::new(),
            include_names: false,
            include_messages: true,
            anonymize: false,
            commits: None,
            preview: None,
            status: None,
        }
    }
}
impl RepoTab {
    fn queue_postcard_export(
        &mut self,
        path: std::path::PathBuf,
        png: Arc<Vec<u8>>,
        capture: impl FnOnce(&std::path::Path) -> Result<export::Destination, String> + Send + 'static,
    ) {
        let profile = crate::git::profile_config();
        self.postcards.context = Some(profile.clone());
        self.postcards.reader.queue(move |owned| {
            let active = || owned() && crate::git::profile_config() == profile;
            if !active() {
                return Err(crate::i18n::text(
                    "repo-workflows-postcards-queue-postcard-export-ce687765",
                )
                .into());
            }
            let destination = capture(&path)?;
            destination.publish(&png, &active)?;
            Ok(Answer::Saved)
        });
    }
    pub(super) fn has_postcard_draft(&self) -> bool {
        self.postcards.open
    }

    pub(super) fn postcard_button(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-postcards-postcard-button-457c26a3",
            ))
            .clicked()
        {
            ui.close();
            self.postcards.open = true;
            if self.postcards.target.is_empty() {
                self.postcards.target = self
                    .selected
                    .and_then(|i| self.commits.get(i))
                    .filter(|c| git::postcards::oid(&c.hash))
                    .map(|c| c.hash.clone())
                    .unwrap_or_else(|| self.refs.head.clone());
                self.postcards.date = self
                    .selected
                    .and_then(|i| self.commits.get(i))
                    .map(|c| clean(&c.date.chars().take(10).collect::<String>(), 10))
                    .unwrap_or_default();
            }
        }
    }
    pub(super) fn poll_postcards(&mut self, ctx: &egui::Context) {
        let profile_changed = self
            .postcards
            .context
            .as_ref()
            .is_some_and(|c| c != &crate::git::profile_config());
        if !self.shown || !self.postcards.open || profile_changed {
            self.postcards.reader.cancel();
            self.postcards.context = None;
            self.postcards.commits = None;
            self.postcards.preview = None;
            if !self.shown {
                self.postcards.open = false;
            }
            if profile_changed {
                self.postcards.commits = None;
                self.postcards.preview = None;
                self.postcards.status = Some(
                    "Profile changed. Read the release range again before composing a postcard."
                        .into(),
                );
            }
            return;
        }
        if let Some(answer) = self.postcards.reader.poll(&self.tx, ctx) {
            match answer {
                Ok(Answer::Range(commits)) => {
                    self.postcards.commits = Some(commits);
                    self.postcards.status=Some("Pinned release range loaded. Review the disclosure list, then compose the PNG preview.".into());
                }
                Ok(Answer::Preview(snapshot, image, png)) => {
                    let texture = ctx.load_texture(
                        "release-postcard-preview",
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.postcards.preview = Some(Preview {
                        snapshot,
                        texture,
                        png: Arc::new(png),
                    });
                    self.postcards.status = Some(
                        "Preview ready. The exported PNG contains exactly this composition; ellipses mark shortened or width-limited text.".into(),
                    );
                }
                Ok(Answer::Saved) => {
                    self.postcards.status = Some("PNG saved as a new local file.".into())
                }
                Err(error) => self.postcards.status = Some(error),
            }
        }
    }
    pub(super) fn postcard_window(&mut self, ctx: &egui::Context) {
        if !self.postcards.open {
            return;
        }
        let mut open = true;
        let mut range_changed = false;
        let mut changed = false;
        let mut load = false;
        let mut compose = false;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-postcards-postcard-window-3304153d",
        ))
        .id(egui::Id::new(("release-postcard", &self.path)))
        .open(&mut open)
        .default_size([780.0, 660.0])
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-674f7ae2",
            ));
            for (label, is_base) in [
                (
                    crate::i18n::text("repo-workflows-postcards-postcard-window-2a433807"),
                    true,
                ),
                (
                    crate::i18n::text("repo-workflows-postcards-postcard-window-07ec4bb7"),
                    false,
                ),
            ] {
                ui.label(label);
                let value = if is_base {
                    &mut self.postcards.base
                } else {
                    &mut self.postcards.target
                };
                range_changed |= ui
                    .add(
                        egui::TextEdit::singleline(value)
                            .hint_text(crate::i18n::text(
                                "repo-workflows-postcards-postcard-window-5daeb684",
                            ))
                            .char_limit(64)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
                egui::ComboBox::from_id_salt(("postcard-end", is_base))
                    .selected_text(crate::i18n::text(
                        "repo-workflows-postcards-postcard-window-f4dc4046",
                    ))
                    .show_ui(ui, |ui| {
                        for branch in self.refs.tags.iter().chain(&self.refs.local) {
                            if git::postcards::oid(&branch.target)
                                && ui
                                    .selectable_label(*value == branch.target, &branch.name)
                                    .clicked()
                            {
                                *value = branch.target.clone();
                                range_changed = true;
                            }
                        }
                    });
            }
            load = ui
                .add_enabled(
                    !self.postcards.reader.busy()
                        && git::postcards::oid(&self.postcards.target)
                        && (self.postcards.base.is_empty()
                            || git::postcards::oid(&self.postcards.base)),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-postcards-postcard-window-fdf4459d",
                    )),
                )
                .clicked();
            ui.separator();
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-85f01282",
            ));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut self.postcards.name).char_limit(80))
                .changed();
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-2a22a9cd",
            ));
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.postcards.date)
                        .char_limit(32)
                        .hint_text("YYYY-MM-DD"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.include_names,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-5725fbec"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.include_messages,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-15a06afd"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.anonymize,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-7fc35df9"),
                )
                .changed();
            ui.small(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-cc45ca72",
            ));
            if let Some(commits) = &self.postcards.commits {
                let snapshot = Snapshot::new(&self.postcards, commits);
                ui.collapsing(
                    crate::i18n::message(
                        "repo-workflows-postcards-postcard-window-77a98200",
                        &[
                            (
                                "snapshot-messages-len-1",
                                (snapshot.messages.len()).clone().into(),
                            ),
                            (
                                "snapshot-contributors-len-2",
                                (snapshot.contributors.len()).clone().into(),
                            ),
                        ],
                    ),
                    |ui| {
                        ui.label(crate::i18n::message(
                            "repo-workflows-postcards-postcard-window-69241bd8",
                            &[
                                ("snapshot-name-1", (snapshot.name).clone().into()),
                                ("snapshot-date-2", (snapshot.date).clone().into()),
                            ],
                        ));
                        ui.monospace(&snapshot.range);
                        ui.label(if snapshot.contributors.is_empty() {
                            crate::i18n::text("repo-workflows-postcards-postcard-window-5db03eee")
                                .into()
                        } else {
                            crate::i18n::message(
                                "repo-workflows-postcards-postcard-window-3772a00f",
                                &[(
                                    "snapshot-contributors-join-1",
                                    (snapshot.contributors.join(", ")).clone().into(),
                                )],
                            )
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("postcard-disclosures")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for message in &snapshot.messages {
                                    ui.label(message);
                                }
                            });
                    },
                );
                compose = ui
                    .add_enabled(
                        !self.postcards.reader.busy()
                            && !snapshot.name.is_empty()
                            && !snapshot.date.is_empty(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-7f9661c5",
                        )),
                    )
                    .clicked();
            }
            if self.postcards.reader.busy() {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    cancel = ui
                        .button(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-5c4a8747",
                        ))
                        .clicked();
                });
            }
            if let Some(status) = &self.postcards.status {
                ui.label(crate::repo::activity::display_workflow_error(status));
            }
            if let Some(preview) = &self.postcards.preview {
                ui.separator();
                ui.label(crate::i18n::message(
                    "repo-workflows-postcards-postcard-window-5b3e5c5e",
                    &[
                        (
                            "preview-snapshot-name-1",
                            (preview.snapshot.name).clone().into(),
                        ),
                        (
                            "preview-snapshot-date-2",
                            (preview.snapshot.date).clone().into(),
                        ),
                    ],
                ));
                ui.add(egui::Image::new(&preview.texture).max_width(ui.available_width()));
                save = ui
                    .add_enabled(
                        !self.postcards.reader.busy(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-1aae6bb1",
                        )),
                    )
                    .clicked();
            }
        });
        if !open {
            self.postcards.open = false;
            self.postcards.reader.cancel();
            self.postcards.preview = None;
            return;
        }
        if range_changed || changed {
            self.postcards.reader.cancel();
            self.postcards.preview = None;
            self.postcards.status = None;
            if range_changed {
                self.postcards.commits = None;
            }
        }
        if cancel {
            self.postcards.reader.cancel();
            self.postcards.status = Some("Postcard work cancelled.".into());
        }
        if load {
            self.postcards.preview = None;
            self.postcards.commits = None;
            let repo = self.path.clone();
            let base = self.postcards.base.clone();
            let target = self.postcards.target.clone();
            let profile = crate::git::profile_config();
            self.postcards.context = Some(profile.clone());
            self.postcards.reader.queue(move |owned| {
                let active = || owned() && crate::git::profile_config() == profile;
                git::postcards::range(&repo, &base, &target, &active).map(Answer::Range)
            });
        }
        if compose && let Some(commits) = &self.postcards.commits {
            let snapshot = Snapshot::new(&self.postcards, commits);
            let profile = crate::git::profile_config();
            self.postcards.context = Some(profile.clone());
            self.postcards.reader.queue(move |owned| {
                let active = || owned() && crate::git::profile_config() == profile;
                let (image, png) = render::render(&snapshot, &active)?;
                Ok(Answer::Preview(snapshot, image, png))
            });
        }
        if save && let Some(preview) = &self.postcards.preview {
            let png = preview.png.clone();
            if let Some(path) = rfd::FileDialog::new()
                .set_title(crate::i18n::text(
                    "repo-workflows-postcards-postcard-window-06d79850",
                ))
                .set_file_name("release-postcard.png")
                .add_filter(
                    crate::i18n::text("repo-workflows-postcards-postcard-window-6ef9788d"),
                    &["png"],
                )
                .save_file()
            {
                self.queue_postcard_export(path, png, export::Destination::capture);
            }
        }
        self.poll_postcards(ctx);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_capture_runs_only_in_the_owned_export_worker() -> Result<(), String> {
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/export-capture".into(), &ctx);
        let ui_thread = std::thread::current().id();
        let (captured, received) = mpsc::channel();
        let path = std::path::PathBuf::from("/nonexistent/gitcraft-postcard-parent/output.png");
        tab.queue_postcard_export(path, Arc::new(Vec::new()), move |path| {
            assert_ne!(std::thread::current().id(), ui_thread);
            captured.send(()).map_err(|e| e.to_string())?;
            export::Destination::capture(path)
        });
        assert!(received.try_recv().is_err());
        assert!(tab.postcards.reader.poll(&tab.tx, &ctx).is_none());
        received
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let start = Instant::now();
        loop {
            if let Some(result) = tab.postcards.reader.poll(&tab.tx, &ctx) {
                assert!(result.is_err());
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                return Err("Export did not finish".into());
            }
            std::thread::yield_now();
        }
        Ok(())
    }
    #[test]
    fn shortened_contributor_names_keep_distinct_disclosed_identities() {
        let prefix = "A".repeat(300);
        let commits = vec![
            Commit {
                author: format!("{prefix}one"),
                ..Default::default()
            },
            Commit {
                author: format!("{prefix}two"),
                ..Default::default()
            },
        ];
        let snapshot = Snapshot::new(
            &State {
                include_names: true,
                ..Default::default()
            },
            &commits,
        );
        assert_eq!(snapshot.contributors.len(), 2);
        assert_ne!(snapshot.contributors[0], snapshot.contributors[1]);
        assert!(
            snapshot
                .contributors
                .iter()
                .all(|n| n.contains('…') && n.len() < 300)
        );
        assert_eq!(clean("abcd", 3), "abc…");
    }
    #[test]
    fn closing_discards_owned_disclosures_before_a_later_profile_reopens() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/postcard-closed-owner".into(), &ctx);
        tab.postcards.commits = Some(vec![Commit {
            author: "Old private contributor".into(),
            ..Default::default()
        }]);
        tab.postcards.context = Some(crate::git::profile_config());
        tab.postcards.open = false;
        tab.poll_postcards(&ctx);
        assert!(tab.postcards.commits.is_none());
        assert!(tab.postcards.preview.is_none());
        tab.postcards.open = true;
        tab.postcards.context = Some(Some("/different/profile".into()));
        tab.poll_postcards(&ctx);
        assert!(tab.postcards.commits.is_none());
    }
    #[test]
    fn profile_change_cancels_a_queued_read_and_discards_prior_disclosures() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/postcard-profile-fixture".into(), &ctx);
        tab.postcards.open = true;
        tab.postcards.commits = Some(vec![Commit {
            author: "Old profile contributor".into(),
            ..Default::default()
        }]);
        tab.postcards.context = Some(Some("/different/profile".into()));
        tab.postcards
            .reader
            .queue(|_| Err("A cancelled queued read must not start".into()));
        tab.poll_postcards(&ctx);
        assert!(!tab.postcards.reader.busy());
        assert!(tab.postcards.commits.is_none());
        assert!(tab.postcards.preview.is_none());
        assert!(
            tab.postcards
                .status
                .as_deref()
                .is_some_and(|status| status.contains("Profile changed"))
        );
    }
    #[test]
    fn anonymization_removes_names_and_subjects_from_render_input_and_preview() {
        let mut state = State {
            anonymize: true,
            include_names: true,
            target: "a".repeat(40),
            ..Default::default()
        };
        let commits = vec![Commit {
            hash: "a".repeat(40),
            author: "Private Person".into(),
            email: "secret@example.com".into(),
            summary: "Private release message".into(),
            body: "Private body".into(),
            ..Default::default()
        }];
        let snapshot = Snapshot::new(&state, &commits);
        assert_eq!(snapshot.messages, vec!["Commit 1"]);
        assert_eq!(snapshot.contributors, vec!["Contributor 1"]);
        assert!(
            snapshot.commits[0].author.is_empty()
                && snapshot.commits[0].email.is_empty()
                && snapshot.commits[0].summary.is_empty()
                && snapshot.commits[0].body.is_empty()
        );
        state.anonymize = false;
        let snapshot = Snapshot::new(&state, &commits);
        assert_eq!(snapshot.messages, vec!["Private release message"]);
        assert_eq!(snapshot.contributors, vec!["Private Person"]);
    }
    #[test]
    fn composed_png_decodes_at_expected_size_and_contains_painted_graph_and_text()
    -> Result<(), String> {
        let state = State {
            name: "Release 1.0".into(),
            date: "2026-11-07".into(),
            target: "a".repeat(40),
            anonymize: true,
            ..Default::default()
        };
        let snapshot = Snapshot::new(
            &state,
            &[Commit {
                hash: "a".repeat(40),
                ..Default::default()
            }],
        );
        let (image, png) = render::render(&snapshot, &|| true)?;
        let decoded = image::load_from_memory(&png).map_err(|e| e.to_string())?;
        assert_eq!(decoded.width(), image.size[0] as u32);
        assert_eq!(decoded.height(), image.size[1] as u32);
        let bg = image.pixels[0];
        assert!(image.pixels.iter().filter(|p| **p != bg).count() > 400);
        assert!(render::render(&snapshot, &|| false).is_err());
        Ok(())
    }
    #[test]
    fn postcard_controls_anonymize_then_compose_the_displayed_preview() -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/postcard-ui".into(), &ctx);
        tab.postcards = State {
            open: true,
            name: "Release".into(),
            date: "2026-11-07".into(),
            target: "a".repeat(40),
            include_names: true,
            commits: Some(vec![Commit {
                hash: "a".repeat(40),
                author: "Private Person".into(),
                summary: "Private subject".into(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 1000.0))
            .build_ui_state(|ui, tab: &mut RepoTab| tab.postcard_window(ui.ctx()), tab);
        harness.run();
        harness
            .get_by_label("Anonymize names and replace all subjects with commit numbers")
            .click();
        harness.run();
        assert!(harness.state().postcards.anonymize);
        harness.get_by_label("Compose PNG preview").click();
        harness.run_steps(2);
        let began = std::time::Instant::now();
        while harness.state().postcards.preview.is_none() {
            if began.elapsed() > std::time::Duration::from_secs(4) {
                return Err(format!(
                    "Preview did not finish: {:?}",
                    harness.state().postcards.status
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            harness.run_steps(1);
        }
        let preview = harness
            .state()
            .postcards
            .preview
            .as_ref()
            .ok_or("preview")?;
        assert_eq!(preview.snapshot.messages, vec!["Commit 1"]);
        assert_eq!(preview.snapshot.contributors, vec!["Contributor 1"]);
        assert!(preview.png.starts_with(b"\x89PNG"));
        Ok(())
    }
}
