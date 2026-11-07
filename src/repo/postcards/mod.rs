//! Release postcards use immutable Git ranges and an explicit disclosure preview before local PNG publication.
mod render;
use super::{RepoTab, saved_views::reader::Reader};
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
    value.chars().filter(|c|!c.is_control() && !matches!(c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}')).take(max).collect()
}
impl Snapshot {
    fn new(state: &State, commits: &[Commit]) -> Self {
        let names: BTreeSet<_> = commits.iter().map(|c| clean(&c.author, 256)).collect();
        let aliases: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), format!("Contributor {}", i + 1)))
            .collect();
        let contributors = if state.include_names {
            names
                .iter()
                .map(|n| {
                    if state.anonymize {
                        aliases[n].clone()
                    } else {
                        n.clone()
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
                    format!("Commit {}", i + 1)
                } else if state.include_messages {
                    clean(&c.summary, 96)
                } else {
                    format!("Commit {}", i + 1)
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
                format!("History through {}", state.target)
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
    context: Option<crate::connections::Context>,
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
            name: "Release history".into(),
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
    pub(super) fn postcard_is_open(&self) -> bool {
        self.postcards.open
    }
    pub(super) fn postcard_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("History postcard…").clicked() {
            ui.close();
            self.postcards.open = true;
            if self.postcards.target.is_empty() {
                self.postcards.target = self
                    .selected
                    .and_then(|i| self.commits.get(i))
                    .filter(|c| git::investigations::oid(&c.hash))
                    .map(|c| c.hash.clone())
                    .unwrap_or_else(|| self.refs.head.clone());
                self.postcards.date = self
                    .selected
                    .and_then(|i| self.commits.get(i))
                    .map(|c| clean(&c.date, 10))
                    .unwrap_or_default();
            }
        }
    }
    pub(super) fn poll_postcards(&mut self, ctx: &egui::Context) {
        let profile_changed = self
            .postcards
            .context
            .as_ref()
            .is_some_and(|c| c != &crate::connections::current());
        if !self.shown || !self.postcards.open || profile_changed {
            self.postcards.reader.cancel();
            self.postcards.context = None;
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
        if let Some(answer) = self.postcards.reader.poll(ctx) {
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
                        "Preview ready. The exported PNG contains exactly this composition.".into(),
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
        egui::Window::new("Release history postcard").id(egui::Id::new(("release-postcard",&self.path))).open(&mut open).default_size([780.0,660.0]).vscroll(true).show(ctx,|ui|{
            ui.label("Choose the excluded base and included release tip. Git reads a pinned range without fetching missing objects (up to 200 commits).");
            for (label,is_base) in [("Base (excluded; blank means full ancestry)",true),("Release tip (included)",false)] {
                ui.label(label);
                let value=if is_base {&mut self.postcards.base}else{&mut self.postcards.target};
                range_changed|=ui.add(egui::TextEdit::singleline(value).hint_text("Full immutable commit ID").char_limit(64).desired_width(f32::INFINITY)).changed();
                egui::ComboBox::from_id_salt(("postcard-end",is_base)).selected_text("Choose a loaded tag or branch…").show_ui(ui,|ui|{
                    for branch in self.refs.tags.iter().chain(&self.refs.local) {
                        if git::investigations::oid(&branch.target) && ui.selectable_label(*value==branch.target,&branch.name).clicked() { *value=branch.target.clone();range_changed=true; }
                    }
                });
            }
            load=ui.add_enabled(!self.postcards.reader.busy()&&git::investigations::oid(&self.postcards.target)&&(self.postcards.base.is_empty()||git::investigations::oid(&self.postcards.base)),egui::Button::new("Read release range")).clicked();
            ui.separator();
            ui.label("Release name");changed|=ui.add(egui::TextEdit::singleline(&mut self.postcards.name).char_limit(80)).changed();
            ui.label("Release date (as displayed)");changed|=ui.add(egui::TextEdit::singleline(&mut self.postcards.date).char_limit(32).hint_text("YYYY-MM-DD")).changed();
            changed|=ui.checkbox(&mut self.postcards.include_names,"Include contributors' names (.mailmap applied; first 256 characters)").changed();
            changed|=ui.checkbox(&mut self.postcards.include_messages,"Include commit subjects (first 96 characters; no message bodies)").changed();
            changed|=ui.checkbox(&mut self.postcards.anonymize,"Anonymize names and replace all subjects with commit numbers").changed();
            ui.small("The release title, date and immutable range IDs remain visible when anonymized. Review those fields before sharing.");
            if let Some(commits)=&self.postcards.commits {
                let snapshot=Snapshot::new(&self.postcards,commits);
                ui.collapsing(format!("Disclosure preview: {} subjects · {} contributor names",snapshot.messages.len(),snapshot.contributors.len()),|ui|{
                    ui.label(format!("Title: {} · Date: {}",snapshot.name,snapshot.date));ui.monospace(&snapshot.range);
                    ui.label(if snapshot.contributors.is_empty(){"No contributor names included".into()}else{format!("Names: {}",snapshot.contributors.join(", "))});
                    egui::ScrollArea::vertical().id_salt("postcard-disclosures").max_height(180.0).show(ui,|ui|{for message in &snapshot.messages {ui.label(message);}});
                });
                compose=ui.add_enabled(!self.postcards.reader.busy()&&!snapshot.name.is_empty()&&!snapshot.date.is_empty(),egui::Button::new("Compose PNG preview")).clicked();
            }
            if self.postcards.reader.busy() { ui.horizontal(|ui|{ui.add(crate::spinner::Spinner::new());cancel=ui.button("Cancel postcard work").clicked();}); }
            if let Some(status)=&self.postcards.status {ui.label(status);}
            if let Some(preview)=&self.postcards.preview {
                ui.separator();ui.label(format!("Included release: {} · {}",preview.snapshot.name,preview.snapshot.date));
                ui.add(egui::Image::new(&preview.texture).max_width(ui.available_width()));
                save=ui.add_enabled(!self.postcards.reader.busy(),egui::Button::new("Save preview as new PNG…")).clicked();
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
            self.postcards.context = Some(crate::connections::current());
            self.postcards.reader.queue(move |active| {
                git::investigations::postcards::range(&repo, &base, &target, active)
                    .map(Answer::Range)
            });
        }
        if compose && let Some(commits) = &self.postcards.commits {
            let snapshot = Snapshot::new(&self.postcards, commits);
            self.postcards.context = Some(crate::connections::current());
            self.postcards.reader.queue(move |active| {
                let (image, png) = render::render(&snapshot, active)?;
                Ok(Answer::Preview(snapshot, image, png))
            });
        }
        if save && let Some(preview) = &self.postcards.preview {
            let png = preview.png.clone();
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save postcard on this machine (new files only)")
                .set_file_name("release-postcard.png")
                .add_filter("PNG image", &["png"])
                .save_file()
            {
                match crate::safe_export::Destination::capture(&path) {
                    Ok(destination) => {
                        self.postcards.context = Some(crate::connections::current());
                        self.postcards.reader.queue(move |active| {
                            destination.publish(&png, active)?;
                            Ok(Answer::Saved)
                        });
                    }
                    Err(error) => self.postcards.status = Some(error),
                }
            }
        }
        self.poll_postcards(ctx);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_change_cancels_a_queued_read_and_discards_prior_disclosures() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/postcard-profile-fixture".into(), &ctx);
        tab.postcards.open = true;
        tab.postcards.commits = Some(vec![Commit {
            author: "Old profile contributor".into(),
            ..Default::default()
        }]);
        tab.postcards.context = Some(crate::connections::Context {
            error: Some("Changed profile".into()),
            ..Default::default()
        });
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
                .as_ref()
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
}
