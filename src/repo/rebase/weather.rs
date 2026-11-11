//! Replays trees only: no index, checkout, hooks, ref movement or user's merge drivers.
//! Git's temporary objects are unreachable and cleaned up by its normal garbage collection.

mod replay;
use replay::*;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::weather_git::ForecastGit;
use super::{Entry, Step};
use crate::repo::messages::spawn_or;
use crate::repo::{Msg, RepoTab};
use crate::theme::Tokens;

#[derive(Clone)]
struct Applied {
    tree: String,
    forecast: Forecast,
}

const DEBOUNCE: Duration = Duration::from_millis(350);
type Plan = Vec<(String, Step)>;
type Cache = HashMap<(String, String), Applied>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Forecast {
    Clean,
    Empty,
    Conflict(Vec<(String, String)>),
    Unknown(String),
    Dropped,
}

#[derive(Default)]
pub(super) struct Weather {
    scope: crate::tasks::Scope,
    plan: Plan,
    mergiraf: Option<bool>,
    generation: Arc<AtomicU64>,
    edited: Option<Instant>,
    running: bool,
    cache: Cache,
    pub(super) rows: Vec<Forecast>,
}

impl Weather {
    pub(super) fn refresh(&mut self, ctx: &egui::Context) {
        self.scope.0.cancel();
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.edited = Some(Instant::now());
        self.rows.clear();
        self.cache.clear();
        self.running = false;
        ctx.request_repaint_after(DEBOUNCE);
    }

    pub(super) fn observe(&mut self, entries: &[Entry], ctx: &egui::Context) {
        let plan: Plan = entries
            .iter()
            .map(|e| (e.commit.hash.clone(), e.step))
            .collect();
        if self.plan != plan {
            self.plan = plan;
            self.scope.0.cancel();
            self.generation.fetch_add(1, Ordering::Relaxed);
            self.edited = Some(Instant::now());
            self.rows.clear();
            self.running = false;
            ctx.request_repaint_after(DEBOUNCE);
        }
    }

    fn arrived(&mut self, generation: u64, rows: Vec<Forecast>, cache: Cache) {
        // A cancelled worker may finish after its replacement: it owns neither the guard
        // nor that replacement's rows/cache.
        if self.generation.load(Ordering::Relaxed) == generation {
            self.running = false;
            self.cache = cache;
            self.rows = rows;
        }
    }
}

impl RepoTab {
    pub(super) fn poll_rebase_weather(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.rebase else {
            return;
        };
        let mergiraf = crate::settings::Settings::get(ctx).mergiraf;
        if editor.weather.mergiraf != Some(mergiraf) {
            editor.weather.refresh(ctx);
            editor.weather.mergiraf = Some(mergiraf);
        }
        editor.weather.observe(&editor.entries, ctx);
        let weather = &mut editor.weather;
        let Some(edited) = weather.edited else { return };
        if weather.running || editor.starting {
            return;
        }
        if edited.elapsed() < DEBOUNCE {
            ctx.request_repaint_after(DEBOUNCE - edited.elapsed());
            return;
        }
        weather.edited = None;
        weather.running = true;
        let (id, onto) = (editor.id, editor.onto.clone());
        let plan = weather.plan.clone();
        let generation = weather.generation.load(Ordering::Relaxed);
        let alive = Arc::downgrade(&weather.generation);
        let mut cache = weather.cache.clone();
        let repo = self.path.clone();
        weather.scope.0 = self.tx.cancel.child();
        let mut sender = self.tx.clone();
        sender.cancel = weather.scope.0.clone();
        spawn_or(
            &sender,
            ctx,
            move |reason| {
                Msg::Then(Box::new(move |tab, _| {
                    if let Some(editor) = tab.rebase.as_mut().filter(|e| e.id == id) {
                        let rows = editor
                            .entries
                            .iter()
                            .map(|_| Forecast::Unknown(reason.clone()))
                            .collect();
                        editor.weather.arrived(generation, rows, Cache::new());
                    }
                }))
            },
            move || {
                let task = crate::tasks::cancellation();
                let active = || {
                    !task.is_cancelled()
                        && alive
                            .upgrade()
                            .is_some_and(|g| g.load(Ordering::Relaxed) == generation)
                };
                let rows = replay(&repo, &onto, &plan, &mut cache, mergiraf, active);
                Msg::Then(Box::new(move |tab, _| {
                    if let Some(editor) = tab.rebase.as_mut().filter(|e| e.id == id) {
                        editor.weather.arrived(generation, rows, cache);
                    }
                }))
            },
        );
    }
}

pub(super) fn badge(ui: &mut egui::Ui, forecast: Option<&Forecast>, dropped: bool, t: &Tokens) {
    let (icon, color, tip) = if dropped {
        (
            "minus",
            t.text_faint,
            crate::i18n::text("repo-core-3282").to_owned(),
        )
    } else {
        match forecast {
            Some(Forecast::Clean) => (
                "sun",
                t.accent,
                crate::i18n::text("repo-core-3284").to_owned(),
            ),
            Some(Forecast::Conflict(files)) => (
                "cloud-lightning",
                t.danger,
                crate::i18n::message(
                    "repo-core-3286",
                    &[(
                        "n",
                        files
                            .iter()
                            .map(|(path, result)| format!("{path}\n  {result}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                            .to_string()
                            .into(),
                    )],
                ),
            ),
            Some(Forecast::Empty) => (
                "cloud",
                t.text_dim,
                crate::i18n::text("repo-core-3288").to_owned(),
            ),
            Some(Forecast::Unknown(reason)) => ("cloud", t.text_dim, reason.clone()),
            _ => (
                "cloud",
                t.text_faint,
                crate::i18n::text("repo-core-3289").to_owned(),
            ),
        }
    };
    ui.add(crate::icons::image(icon, 18.0, color).sense(egui::Sense::hover()))
        .on_hover_text(tip);
}

#[cfg(test)]
mod tests;
