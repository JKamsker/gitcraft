//! One retained read owner per tab, even when its dossier is closed/replaced.
#[cfg(test)]
mod tests;
mod view;
use super::saved_views::reader::Reader;
use super::{Action, RepoTab};
use crate::git::investigations::dossier::{self, Dossier, Seed};
enum Answer {
    History { append: bool, history: Dossier },
    Ignore(Box<IgnoreView>),
}
struct IgnoreView {
    prepared: crate::git::investigations::ignore::Preview,
    patch: Result<super::file_text::FileText, String>,
    existing: Result<super::file_text::FileText, String>,
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
    ignore: Option<IgnoreView>,
    ignore_reviewed: bool,
    show_existing: bool,
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
            ignore: None,
            ignore_reviewed: false,
            show_existing: false,
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
        self.ignore = None;
        self.ignore_reviewed = false;
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
        self.dossier.ignore = None;
        self.dossier.ignore_reviewed = false;
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
                Ok(Answer::Ignore(preview)) => {
                    if self.dossier.open {
                        self.dossier.ignore = Some(*preview);
                        self.dossier.ignore_reviewed = false;
                        self.dossier.show_existing = false;
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

impl RepoTab {
    fn prepare_dossier_ignore(&mut self, commit: String, ctx: &egui::Context) {
        let repo = self.path.clone();
        let dark = ctx.style().visuals.dark_mode;
        self.dossier.ignore = None;
        self.dossier.ignore_reviewed = false;
        self.dossier.error = None;
        self.dossier.reader.queue(move |active| {
            let prepared = crate::git::investigations::ignore::prepare(&repo, &commit, active)?;
            let patch =
                super::file_text::FileText::new(prepared.patch.as_bytes(), "formatting.diff", dark);
            let existing = super::file_text::FileText::new(
                prepared.existing.as_bytes(),
                "ignore-revisions.txt",
                dark,
            );
            Ok(Answer::Ignore(Box::new(IgnoreView {
                prepared,
                patch: Ok(patch),
                existing: Ok(existing),
            })))
        });
        ctx.request_repaint();
    }
    fn append_dossier_ignore(&mut self, ctx: &egui::Context) {
        if !self.dossier.ignore_reviewed || self.busy_op.is_some() {
            return;
        }
        let Some(preview) = self.dossier.ignore.take() else {
            return;
        };
        self.dossier.ignore_reviewed = false;
        let context = crate::connections::current();
        self.busy_op = Some("Appending reviewed ignore revision".into());
        self.op_started = Some(std::time::Instant::now());
        super::messages::spawn_or(
            &self.tx,
            ctx,
            |error| {
                super::Msg::Then(Box::new(move |tab, ctx| {
                    tab.dossier_ignore_finished(Err(error), ctx)
                }))
            },
            move || {
                // Deliberately confirmed mutation owns its lifetime and Profile. UI close
                // cannot retract a completed append or discard its acknowledgement.
                let result = context.run(|| {
                    crate::git::investigations::ignore::perform(&preview.prepared, &|| true)
                });
                super::Msg::Then(Box::new(move |tab, ctx| {
                    tab.dossier_ignore_finished(result, ctx)
                }))
            },
        );
    }
    fn dossier_ignore_finished(
        &mut self,
        result: Result<crate::git::investigations::ignore::Appended, String>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.as_deref() == Some("Appending reviewed ignore revision") {
            self.busy_op = None;
            self.op_started = None;
        }
        self.status=Some(match result{
            Ok(crate::git::investigations::ignore::Appended::Changed)=>("Revision appended to .git-blame-ignore-revs; staged contents preserved. Ordinary Git blame can use blame.ignoreRevsFile=.git-blame-ignore-revs.".into(),false),
            Ok(crate::git::investigations::ignore::Appended::AlreadyPresent)=>("Revision already present; ignore file and index unchanged".into(),false),
            Err(error)=>(format!("Ignore revision: {error}"),true),
        });
        self.reload_status(ctx);
    }
}
