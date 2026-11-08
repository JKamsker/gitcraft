//! One retained read owner per tab, even when its dossier is closed/replaced.
mod summary;
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
}
pub(super) struct State {
    pub open: bool,
    revision: String,
    path: String,
    line: usize,
    reader: Reader<Answer>,
    forge_reader: Reader<crate::forge::history::Evidence>,
    evidence_for: Option<Seed>,
    evidence: Option<Result<crate::forge::history::Evidence, String>>,
    summary_reader: Reader<String>,
    summary: Option<Result<String, String>>,
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
            forge_reader: Default::default(),
            evidence_for: None,
            evidence: None,
            summary_reader: Default::default(),
            summary: None,
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
        self.forge_reader.cancel();
        self.summary_reader.cancel();
        self.evidence_for = None;
        self.evidence = None;
        self.summary = None;
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
            self.dossier.summary_reader.cancel();
            self.dossier.summary = None;
            self.dossier.evidence_for = None;
            self.dossier.selected = 0;
        }
        self.dossier.reader.queue(move |active| {
            dossier::trace(&repo, &seed.commit, &seed.path, seed.line, active)
                .map(|history| Answer::History { append, history })
        });
        ctx.request_repaint();
    }
    pub(super) fn poll_line_dossier(&mut self, ctx: &egui::Context) {
        self.dossier.reader.bind(self.tx.cancel.clone());
        self.dossier.forge_reader.bind(self.tx.cancel.clone());
        self.dossier.summary_reader.bind(self.tx.cancel.clone());
        if let Some(result) = self.dossier.summary_reader.poll(ctx) {
            self.dossier.summary = Some(result);
        }
        if let Some(result) = self.dossier.forge_reader.poll(ctx) {
            self.dossier.evidence = Some(result);
        }

        if !self.shown {
            if self.dossier.open {
                self.dossier.close();
            }
            return;
        }
        if self.dossier.open
            && let Some(card) = self
                .dossier
                .result
                .as_ref()
                .and_then(|r| r.cards.get(self.dossier.selected))
            && self.dossier.evidence_for.as_ref() != Some(&card.at)
        {
            self.dossier.evidence_for = Some(card.at.clone());
            self.dossier.evidence = None;
            let query = crate::forge::history::Query {
                at: card.at.clone(),
                file_oid: card.file_oid.clone(),
                comments: true,
            };
            let repo = self.path.clone();
            let hosting = self.hosting.clone();
            self.dossier.forge_reader.queue(move |active| {
                if let Some(why) = crate::network::refused() {
                    return Err(why.why().into());
                }
                crate::forge::history::lookup(
                    &repo,
                    &hosting
                        .ok_or("No supported forge remote; commit history remains available")?,
                    &query,
                    active,
                )
            });
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
                    self.dossier.summary_reader.cancel();
                    self.dossier.summary = None;
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
                                    history.stopped = "Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history"
                                    .into();
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
        self.dossier.ignore = None;
        self.dossier.ignore_reviewed = false;
        self.dossier.error = None;
        self.dossier.reader.queue(move |active| {
            let prepared = crate::git::investigations::ignore::prepare(&repo, &commit, active)?;
            Ok(Answer::Ignore(Box::new(IgnoreView { prepared })))
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
        self.busy_op = Some("Appending reviewed ignore revision".into());
        self.op_started = Some(std::time::Instant::now());
        super::messages::spawn_mutation(
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
                let token = crate::tasks::cancellation();
                let result =
                    crate::git::investigations::ignore::perform(&preview.prepared, &|| {
                        !token.is_cancelled()
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
        self.status=Some(crate::repo::Notice::plain(match result{
            Ok(crate::git::investigations::ignore::Appended::Changed)=>("Revision appended to .git-blame-ignore-revs; staged contents preserved. Ordinary Git blame can use blame.ignoreRevsFile=.git-blame-ignore-revs.".into(),false),
            Ok(crate::git::investigations::ignore::Appended::AlreadyPresent)=>("Revision already present; ignore file and index unchanged".into(),false),
            Err(error)=>(format!("Ignore revision: {error}"),true),
        }));
        self.reload_status(ctx);
    }
}

impl RepoTab {
    fn summarize_dossier(&mut self, ctx: &egui::Context) {
        let Some(result) = &self.dossier.result else {
            return;
        };
        let cards = result.cards.clone();
        let path = self.path.clone();
        let settings = crate::settings::Settings::get(ctx).ai.clone();
        self.dossier.summary = None;
        self.dossier
            .summary_reader
            .queue(move |active| summary::run(&path, &settings, &cards, active));
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod fluent_r1_cache_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_dossier_cached_error_summary_and_evidence_keep_raw_source() {
        let mut state = State::default();
        state.open = true;
        state.path = "raw: {$x}/日本語".into();
        state.line = 1234;
        state.error = Some("History read cancelled".into());
        state.summary = Some(Err("History worker panicked".into()));
        state.evidence = Some(Err("History worker stopped".into()));
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            for raw in [
                state.error.as_ref().unwrap(),
                state.summary.as_ref().unwrap().as_ref().unwrap_err(),
                state.evidence.as_ref().unwrap().as_ref().unwrap_err(),
            ] {
                assert_eq!(
                    crate::repo::activity::display_workflow_error(raw) == *raw,
                    lang == "en-US"
                );
            }
            assert_eq!(state.path, "raw: {$x}/日本語");
            assert_eq!(state.line, 1234);
            assert!(state.open);
            assert!(!state.reader.busy());
        }
        crate::i18n::set_language("en-US");
    }
}
