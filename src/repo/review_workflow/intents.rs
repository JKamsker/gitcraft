//! Advisory, expiring notes exchanged as ordinary fast-forward Git refs, never locks.
use super::{sharing, tools::ToolsAct};
use crate::{
    git, network,
    repo::{Action, RepoTab},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
const ROOT: &str = "refs/gitcraft/intents/";
#[derive(Clone, Serialize, Deserialize)]
struct Card {
    id: String,
    author: String,
    email: String,
    path: String,
    symbol: String,
    message: String,
    created: u64,
    expires: u64,
}
#[derive(Clone)]
struct Seen {
    reference: String,
    card: Card,
    local: bool,
}
#[derive(Default)]
pub(crate) struct State {
    generation: u64,
    initialized: bool,
    busy: bool,
    rows: Vec<Seen>,
    error: Option<String>,
    quarantine: Vec<String>,
    remote: String,
    path: String,
    symbol: String,
    message: String,
    minutes: u32,
    synced: u64,
    checked: u64,
    cache_key: Option<String>,
    synced_remote: Option<git::Remote>,
}
#[derive(Clone)]
pub(crate) enum Act {
    Read,
    Post {
        path: String,
        symbol: String,
        message: String,
        minutes: u32,
    },
    Expire(String),
    Sync(String),
    Publish(String),
}
impl State {
    fn selection_changed(&mut self) {
        self.generation += 1;
        self.synced = 0;
        self.synced_remote = None;
        self.cache_key = None;
        self.rows.retain(|row| row.local);
        self.quarantine.clear();
    }
    fn accept(
        &mut self,
        generation: u64,
        result: Result<(Reading, Option<String>), String>,
    ) -> bool {
        self.busy = false;
        if generation != self.generation {
            return false;
        }
        match result {
            Ok((reading, source)) => {
                self.rows = reading.rows;
                self.quarantine = reading.quarantine;
                self.cache_key = source;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
                self.synced = 0;
            }
        }
        true
    }
}
fn applies(card: &Card, path: &str, at: u64) -> bool {
    card.expires > at
        && (path == card.path
            || path
                .strip_prefix(&card.path)
                .is_some_and(|tail| tail.starts_with('/')))
}
impl RepoTab {
    pub(in crate::repo) fn intent_frame(&mut self, ctx: &egui::Context) {
        if self
            .review_tools
            .intents
            .synced_remote
            .as_ref()
            .is_some_and(|old| !self.remotes.iter().any(|r| r == old))
        {
            self.review_tools.intents.selection_changed();
        }
        if !self.review_tools.intents.initialized {
            self.review_tools.intents.initialized = true;
            self.intent_act(Act::Read, ctx);
        } else if !self.review_tools.intents.busy
            && !self.review_tools.intents.remote.is_empty()
            && self.review_tools.intents.synced > 0
            && now().saturating_sub(self.review_tools.intents.checked) > 60
            && network::background_may_run()
        {
            self.intent_act(Act::Sync(self.review_tools.intents.remote.clone()), ctx);
        }
    }
    pub(in crate::repo) fn intent_act(&mut self, act: Act, ctx: &egui::Context) {
        if self.review_tools.intents.busy {
            return;
        }
        let is_network = matches!(act, Act::Sync(_));
        let observed = if let Act::Sync(remote) | Act::Publish(remote) = &act {
            let Some(observed) = self.remotes.iter().find(|r| &r.name == remote).cloned() else {
                self.review_tools.intents.error = Some("Pick a configured remote".into());
                return;
            };
            if let Some(p) = network::refused() {
                self.review_tools.intents.error = Some(p.why().into());
                return;
            }
            Some(observed)
        } else {
            None
        };
        if matches!(act, Act::Read) {
            self.review_tools.intents.synced = 0;
        }
        let generation = self.review_tools.intents.generation;
        let selected_remote = self.review_tools.intents.remote.clone();
        let synced_observed = observed.clone();
        let mut active_source = self.review_tools.intents.cache_key.clone();
        self.review_tools.intents.busy = true;
        self.review_tools.intents.checked = now();
        let (repo, creds) = (self.path.clone(), self.credentials.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let target = observed
                    .as_ref()
                    .map(|r| sharing::Target::capture(&repo, r))
                    .transpose()?;
                match act {
                    Act::Post {
                        path,
                        symbol,
                        message,
                        minutes,
                    } => post(&repo, path, symbol, message, minutes)?,
                    Act::Expire(reference) => {
                        let (_, email) = identity(&repo)?;
                        let mine = format!("{ROOT}{:x}/", md5::compute(email));
                        if !reference.starts_with(&mine) {
                            return Err("You can only end your own local intents".into());
                        }
                        let mut seen = read(&repo)?
                            .into_iter()
                            .find(|c| c.reference == reference && c.local)
                            .ok_or("Intent no longer present")?;
                        seen.card.expires = now();
                        seen.card.created = now();
                        save(&repo, &reference, &seen.card)?;
                    }
                    Act::Sync(_) => {
                        let target = target.as_ref().ok_or("No captured sharing destination")?;
                        let cache_key = target.cache_key();
                        let destination = format!("refs/gitcraft/intent-remotes/{cache_key}/");
                        target.fetch(
                            &repo,
                            false,
                            &[format!("+{ROOT}*:{destination}*")],
                            &destination,
                            true,
                            creds.as_ref(),
                        )?;
                        active_source = Some(cache_key);
                    }
                    Act::Publish(_) => {
                        let target = target.as_ref().ok_or("No captured sharing destination")?;
                        let (_, email) = identity(&repo)?;
                        let mine = format!("{ROOT}{:x}/", md5::compute(email));
                        let refs = target.source_refs(&mine)?;
                        if refs.is_empty() {
                            return Err("No local intents to publish".into());
                        }
                        let specs: Vec<_> = refs
                            .iter()
                            .map(|(reference, oid)| format!("{oid}:{reference}"))
                            .collect();
                        let mut args = vec!["push", "--", target.push_url()];
                        args.extend(specs.iter().map(String::as_str));
                        target.run(&repo, &args, creds.as_ref())?;
                    }
                    Act::Read => {}
                }
                if let Some(target) = &target {
                    target.guard(&repo)?;
                }
                Ok((
                    read_selected(&repo, active_source.as_deref())?,
                    active_source,
                ))
            },
            move |tab, result, _| {
                let current_source = synced_observed
                    .as_ref()
                    .is_none_or(|old| tab.remotes.iter().any(|r| r == old));
                let s = &mut tab.review_tools.intents;
                if generation != s.generation || selected_remote != s.remote || !current_source {
                    s.busy = false;
                    s.synced = 0;
                    return;
                }
                let succeeded = result.is_ok();
                if s.accept(generation, result) && is_network && succeeded {
                    s.synced = now();
                    s.synced_remote = synced_observed;
                }
            },
        );
    }
    pub(in crate::repo) fn intent_badge(&self, ui: &mut egui::Ui, path: &str) {
        let s = &self.review_tools.intents;
        for seen in s.rows.iter().filter(|s| applies(&s.card, path, now())) {
            let text = if seen.local {
                crate::i18n::message(
                    "repo-workflows-review-message-local-intent-author-message-symbol-minutes-one-e26b68",
                    &[
                        ("author", (seen.card.author.as_str()).into()),
                        ("message", (seen.card.message.as_str()).into()),
                        ("symbol", (seen.card.symbol.as_str()).into()),
                        (
                            "minutes",
                            (seen.card.expires.saturating_sub(now()).div_ceil(60)).into(),
                        ),
                    ],
                )
            } else if network::refused().is_some() {
                crate::i18n::message(
                    "repo-workflows-review-message-offline-cached-author-message-symbol-minutes-o-5044d7",
                    &[
                        ("author", (seen.card.author.as_str()).into()),
                        ("message", (seen.card.message.as_str()).into()),
                        ("symbol", (seen.card.symbol.as_str()).into()),
                        (
                            "minutes",
                            (seen.card.expires.saturating_sub(now()).div_ceil(60)).into(),
                        ),
                    ],
                )
            } else if s.synced == 0 || now().saturating_sub(s.synced) > 120 {
                crate::i18n::message(
                    "repo-workflows-review-message-stale-not-recently-synchronized-author-message-cd8c1a",
                    &[
                        ("author", (seen.card.author.as_str()).into()),
                        ("message", (seen.card.message.as_str()).into()),
                        ("symbol", (seen.card.symbol.as_str()).into()),
                        (
                            "minutes",
                            (seen.card.expires.saturating_sub(now()).div_ceil(60)).into(),
                        ),
                    ],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-review-message-shared-intent-author-message-symbol-minutes-on-0422e3",
                    &[
                        ("author", (seen.card.author.as_str()).into()),
                        ("message", (seen.card.message.as_str()).into()),
                        ("symbol", (seen.card.symbol.as_str()).into()),
                        (
                            "minutes",
                            (seen.card.expires.saturating_sub(now()).div_ceil(60)).into(),
                        ),
                    ],
                )
            };
            ui.label(egui::RichText::new(text).small());
        }
    }
}
#[cfg(test)]
#[path = "intents/fluent_review_r1_tests.rs"]
mod fluent_review_r1_tests;
#[cfg(test)]
#[path = "intents/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "intents/tests.rs"]
mod tests;

mod local_cards;
use local_cards::{identity, now, post, save};

mod reading;
#[cfg(test)]
use reading::{collect_selected, read_active, source_roots};
use reading::{read, read_selected, validate};

use reading::Reading;
#[cfg(test)]
use reading::{CACHE_ROOT, CARD_LIMIT, IMPORT_LIMIT};

mod presentation;
pub(super) use presentation::show;
