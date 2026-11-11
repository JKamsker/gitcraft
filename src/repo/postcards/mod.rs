//! Release postcards use immutable Git ranges and an explicit disclosure preview before local PNG publication.
mod dialog;

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
                        &[("i-1-1", (i + 1).into())],
                    )
                } else if state.include_messages {
                    clean(&c.summary, 96)
                } else {
                    crate::i18n::message(
                        "repo-workflows-postcards-new-53bf7c37",
                        &[("i-1-1", (i + 1).into())],
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
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
