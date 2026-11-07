//! One retained read owner per tab, even when its dossier is closed/replaced.
#[cfg(test)]
mod tests;
mod view;
use super::saved_views::reader::Reader;
use super::{Action, RepoTab};
use crate::git::investigations::dossier::{self, Dossier, Seed};
enum Answer {
    History { append: bool, history: Dossier },
}
pub(super) struct State {
    pub open: bool,
    revision: String,
    path: String,
    line: usize,
    reader: Reader<Answer>,
    pub result: Option<Dossier>,
    error: Option<String>,
    selected: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            revision: "HEAD".into(),
            path: String::new(),
            line: 1,
            reader: Default::default(),
            result: None,
            error: None,
            selected: 0,
        }
    }
}
impl State {
    pub fn close(&mut self) {
        self.open = false;
        self.reader.cancel();
        self.result = None;
        self.error = None;
        self.selected = 0;
    }
}
impl RepoTab {
    pub(super) fn open_line_dossier(&mut self, seed: Option<Seed>, ctx: &egui::Context) {
        self.dossier.close();
        self.dossier.open = true;
        if let Some(seed) = seed {
            self.trace_line_dossier(seed, false, ctx);
        } else {
            self.dossier.revision = if crate::git::investigations::oid(&self.refs.head) {
                self.refs.head.clone()
            } else {
                "HEAD".into()
            };
            self.dossier.path = self
                .file_view
                .as_ref()
                .map_or_else(String::new, |v| v.path.clone());
            self.dossier.line = 1;
        }
    }
    fn trace_line_dossier(&mut self, seed: Seed, append: bool, ctx: &egui::Context) {
        if !self.dossier.open {
            return;
        }
        let repo = self.path.clone();
        self.dossier.revision = seed.commit.clone();
        self.dossier.path = seed.path.clone();
        self.dossier.line = seed.line;
        self.dossier.error = None;
        if !append {
            self.dossier.result = None;
            self.dossier.selected = 0;
        }
        self.dossier.reader.queue(move |active| {
            dossier::trace(&repo, &seed.commit, &seed.path, seed.line, active)
                .map(|history| Answer::History { append, history })
        });
        ctx.request_repaint();
    }
    pub(super) fn poll_line_dossier(&mut self, ctx: &egui::Context) {
        if !self.shown {
            if self.dossier.open {
                self.dossier.close();
            }
            return;
        }
        if let Some(answer) = self.dossier.reader.poll(ctx) {
            match answer {
                Ok(Answer::History {
                    append,
                    mut history,
                }) => {
                    if !self.dossier.open {
                        return;
                    }
                    if append {
                        if let Some(old) = self.dossier.result.as_mut() {
                            let mut bytes = old.cards.iter().map(|c| c.text.len()).sum::<usize>();
                            for card in history.cards {
                                if old.cards.iter().any(|c| c.at == card.at) {
                                    continue;
                                }
                                if old.cards.len() >= 256
                                    || bytes.saturating_add(card.text.len()) > 32 * 1024 * 1024
                                {
                                    history.stopped="Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history".into();
                                    history.continuation = None;
                                    history.alternatives.clear();
                                    break;
                                }
                                bytes += card.text.len();
                                old.cards.push(card);
                            }
                            old.stopped = history.stopped;
                            old.continuation = history.continuation;
                            old.alternatives = history.alternatives;
                        } else {
                            self.dossier.result = Some(history);
                        }
                    } else {
                        self.dossier.result = Some(history);
                    }
                }
                Err(error) => {
                    if self.dossier.open {
                        self.dossier.error = Some(error);
                    }
                }
            }
        }
    }
}
