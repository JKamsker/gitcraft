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
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn identity(repo: &Path) -> Result<(String, String), String> {
    let name = git::run(Some(repo), &["config", "user.name"])?;
    let email = git::run(Some(repo), &["config", "user.email"])?;
    if name.trim().is_empty() || email.trim().is_empty() {
        return Err("Configure your Git name and email before posting an intent".into());
    }
    Ok((name.trim().into(), email.trim().into()))
}
fn save(repo: &Path, reference: &str, card: &Card) -> Result<(), String> {
    let old = git::run(Some(repo), &["rev-parse", "--verify", reference]).ok();
    let data = serde_json::to_string(card).map_err(|e| e.to_string())?;
    let blob = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &data)?;
    let tree = git::run_input(
        Some(repo),
        &["mktree"],
        &format!("100644 blob {}\tintent.json\n", blob.trim()),
    )?;
    let mut args = vec![
        "-c",
        "commit.gpgSign=false",
        "commit-tree",
        tree.trim(),
        "-m",
        "gitcraft: advisory intent",
    ];
    if let Some(old) = &old {
        args.extend(["-p", old.trim()]);
    }
    let commit = git::run(Some(repo), &args)?;
    git::run(
        Some(repo),
        &[
            "update-ref",
            reference,
            commit.trim(),
            old.as_deref().unwrap_or("").trim(),
        ],
    )
    .map(|_| ())
}
fn post(
    repo: &Path,
    path: String,
    symbol: String,
    message: String,
    minutes: u32,
) -> Result<(), String> {
    if path.trim().is_empty()
        || path.starts_with('/')
        || path.split('/').any(|p| p == "..")
        || message.trim().is_empty()
    {
        return Err("Give a relative code path and a short intent".into());
    }
    let (author, email) = identity(repo)?;
    let created = now();
    let id = format!(
        "{:x}",
        md5::compute(format!(
            "{email}:{path}:{symbol}:{message}:{created}:{}",
            std::process::id()
        ))
    );
    let reference = format!("{ROOT}{:x}/{id}", md5::compute(&email));
    let card = Card {
        id,
        author,
        email,
        path,
        symbol,
        message,
        created,
        expires: created.saturating_add(u64::from(minutes.clamp(1, 1440)) * 60),
    };
    validate(&card, &reference, ROOT, created)?;
    save(repo, &reference, &card)
}
const CACHE_ROOT: &str = "refs/gitcraft/intent-remotes/";
const CARD_LIMIT: usize = 8192;
const IMPORT_LIMIT: usize = 4 * 1024 * 1024;
#[derive(Default)]
struct Reading {
    rows: Vec<Seen>,
    quarantine: Vec<String>,
}
fn source_roots(source: Option<&str>) -> Result<Vec<String>, String> {
    let mut roots = vec![ROOT.to_owned()];
    if let Some(source) = source {
        if source.is_empty()
            || source.len() > 64
            || !source
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("Invalid intent cache identity".into());
        }
        roots.push(format!("{CACHE_ROOT}{source}/"));
    }
    Ok(roots)
}
fn validate(card: &Card, reference: &str, root: &str, at: u64) -> Result<(), String> {
    let hex = |s: &str| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let relative = reference
        .strip_prefix(root)
        .ok_or("Intent is outside the selected source")?;
    let (author, id) = relative
        .split_once('/')
        .ok_or("Invalid intent ref identity")?;
    if !hex(author)
        || !hex(id)
        || id != card.id
        || author != format!("{:x}", md5::compute(&card.email))
    {
        return Err("Intent identity does not match its source ref".into());
    }
    let short = |text: &str, max: usize| text.len() <= max && !text.chars().any(|c| c.is_control());
    if card.path.is_empty()
        || !short(&card.path, 512)
        || card.path.starts_with('/')
        || card.path.contains(['\\', ':'])
        || card
            .path
            .trim_end_matches('/')
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | "..") || p.eq_ignore_ascii_case(".git"))
        || card.author.trim().is_empty()
        || !short(&card.author, 128)
        || card.email.trim().is_empty()
        || !short(&card.email, 254)
        || !short(&card.symbol, 256)
        || card.message.trim().is_empty()
        || !short(&card.message, 1024)
    {
        return Err("Intent fields exceed the short-note bounds or contain an unsafe path/control character".into());
    }
    if card.created > at.saturating_add(300)
        || card.expires < card.created
        || card.expires > card.created.saturating_add(86400)
    {
        return Err("Intent expiry must be within 24 hours of its creation".into());
    }
    Ok(())
}
fn collect_selected(
    source: Option<&str>,
    at: u64,
    mut list: impl FnMut(&str) -> Result<Vec<(String, String)>, String>,
    mut blob: impl FnMut(&str, usize) -> Result<String, String>,
) -> Result<Reading, String> {
    let mut reading = Reading::default();
    let mut cards = BTreeMap::<(String, String), Seen>::new();
    let mut bytes: usize = 0;
    let mut rejected = 0;
    for root in source_roots(source)? {
        // List only this source. Foreign cache content is never parsed or deduplicated.
        let refs = list(&root)?;
        if refs.len() > 5000 {
            return Err("More than 5000 refs in the selected intent source; archive old notes before loading".into());
        }
        for (reference, object) in refs {
            if !reference.starts_with(&root) {
                continue;
            }
            let result: Result<Seen, String> = (|| {
                if bytes >= IMPORT_LIMIT {
                    return Err("Selected intent metadata exceeds the 4MiB import budget".into());
                }
                let text = blob(&object, CARD_LIMIT.min(IMPORT_LIMIT - bytes))?;
                bytes = bytes.saturating_add(text.len());
                if text.len() > CARD_LIMIT || bytes > IMPORT_LIMIT {
                    return Err("Intent exceeds the metadata import budget".into());
                }
                let card: Card =
                    serde_json::from_str(&text).map_err(|e| format!("Invalid intent JSON: {e}"))?;
                validate(&card, &reference, &root, at)?;
                Ok(Seen {
                    reference: reference.clone(),
                    card,
                    local: root == ROOT,
                })
            })();
            match result {
                Ok(seen) => {
                    cards.insert((root.clone(), seen.reference.clone()), seen);
                }
                Err(error) => {
                    rejected += 1;
                    if reading.quarantine.len() < 32 {
                        reading.quarantine.push(format!("{reference}: {error}"));
                    }
                }
            }
            if bytes >= IMPORT_LIMIT {
                break;
            }
        }
    }
    if rejected > reading.quarantine.len() {
        reading.quarantine.push(format!(
            "{} additional invalid records omitted",
            rejected - reading.quarantine.len()
        ));
    }
    reading.rows = cards.into_values().collect();
    Ok(reading)
}
fn read_selected(repo: &Path, source: Option<&str>) -> Result<Reading, String> {
    collect_selected(
        source,
        now(),
        |root| {
            let listing = git::run(
                Some(repo),
                &["for-each-ref", "--format=%(objectname) %(refname)", root],
            )?;
            listing
                .lines()
                .map(|line| {
                    let (object, reference) =
                        line.split_once(' ').ok_or("Malformed intent ref listing")?;
                    Ok((reference.to_owned(), object.to_owned()))
                })
                .collect()
        },
        |object, budget| {
            let path = format!("{object}:intent.json");
            let size = git::run(Some(repo), &["cat-file", "-s", &path])?
                .trim()
                .parse::<usize>()
                .map_err(|_| "Invalid intent blob size")?;
            if size > budget {
                return Err(format!("Intent blob exceeds the {budget}-byte budget"));
            }
            git::run(Some(repo), &["cat-file", "blob", &path])
        },
    )
}
fn read(repo: &Path) -> Result<Vec<Seen>, String> {
    Ok(read_selected(repo, None)?.rows)
}
#[cfg(test)]
fn read_active(repo: &Path, source: Option<&str>) -> Result<Vec<Seen>, String> {
    Ok(read_selected(repo, source)?.rows)
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
            let status = if seen.local {
                "Local intent"
            } else if network::refused().is_some() {
                "Offline · cached"
            } else if s.synced == 0 || now().saturating_sub(s.synced) > 120 {
                "Stale · not recently synchronized"
            } else {
                "Shared intent"
            };
            ui.label(
                egui::RichText::new(format!(
                    "{status}: {} · {}{} (expires in {} minutes)",
                    seen.card.author,
                    seen.card.message,
                    if seen.card.symbol.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", seen.card.symbol)
                    },
                    seen.card.expires.saturating_sub(now()).div_ceil(60)
                ))
                .small(),
            );
        }
    }
}
pub(super) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    remotes: &[git::Remote],
    acts: &mut Vec<Action>,
) {
    ui.heading("Intent cards");
    ui.label("Advisory notes travel through refs/gitcraft/intents. They never lock code or prevent another person's work.");
    let previous_remote = s.remote.clone();
    egui::ComboBox::from_id_salt("intent-remote")
        .selected_text(if s.remote.is_empty() {
            "Choose sharing remote"
        } else {
            &s.remote
        })
        .show_ui(ui, |ui| {
            for r in remotes {
                ui.selectable_value(&mut s.remote, r.name.clone(), &r.name);
            }
        });
    if previous_remote != s.remote {
        s.selection_changed();
    }
    ui.horizontal(|ui| {
        for (label, act) in [
            ("Read cached intents", Act::Read),
            ("Fetch teammate intents", Act::Sync(s.remote.clone())),
            ("Publish my intents", Act::Publish(s.remote.clone())),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Intent(act)));
            }
        }
    });
    if let Some(e) = &s.error {
        ui.label(e);
    }
    if network::refused().is_some() {
        ui.label("Offline: cached observations only");
    } else if s.synced == 0 || now().saturating_sub(s.synced) > 120 {
        ui.label("Stale: shared refs have not been synchronized recently");
    }
    if !s.quarantine.is_empty() {
        egui::CollapsingHeader::new(format!(
            "{} invalid/oversize intent records quarantined",
            s.quarantine.len()
        ))
        .show(ui, |ui| {
            for warning in &s.quarantine {
                ui.label(warning);
            }
        });
    }
    for seen in &s.rows {
        ui.group(|ui| {
            ui.label(format!(
                "{} · {} {} · {}",
                seen.card.author, seen.card.path, seen.card.symbol, seen.card.message
            ));
            let left = seen.card.expires.saturating_sub(now());
            ui.weak(if left == 0 {
                "Expired / ended".into()
            } else {
                format!("Expires in {} minutes", left.div_ceil(60))
            });
            if seen.local && left > 0 && ui.button("End this intent").clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Intent(Act::Expire(
                    seen.reference.clone(),
                ))));
            }
        });
    }
    ui.add(egui::TextEdit::singleline(&mut s.path).hint_text("Repository-relative path or folder"));
    ui.add(egui::TextEdit::singleline(&mut s.symbol).hint_text("Symbol (optional)"));
    ui.add(egui::TextEdit::singleline(&mut s.message).hint_text("Changing token lifetime…"));
    if s.minutes == 0 {
        s.minutes = 60;
    }
    ui.add(egui::Slider::new(&mut s.minutes, 1..=1440).text("Minutes"));
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new("Post local intent (publish when ready)"),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Intent(Act::Post {
            path: s.path.clone(),
            symbol: s.symbol.clone(),
            message: s.message.clone(),
            minutes: s.minutes,
        })));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn card(id: &str) -> Card {
        Card {
            id: id.into(),
            author: "Teammate".into(),
            email: "team@example.test".into(),
            path: "src".into(),
            symbol: "token".into(),
            message: "changing lifetime".into(),
            created: 100,
            expires: 200,
        }
    }
    fn reference(root: &str, card: &Card) -> String {
        format!("{root}{:x}/{}", md5::compute(&card.email), card.id)
    }
    #[test]
    fn r2_pure_intent_sources_are_selected_before_parse_and_keep_same_id_separate()
    -> Result<(), String> {
        let card = card("0123456789abcdef0123456789abcdef");
        let local = reference(ROOT, &card);
        let selected = reference("refs/gitcraft/intent-remotes/selected/", &card);
        let foreign = reference("refs/gitcraft/intent-remotes/foreign/", &card);
        let json = serde_json::to_string(&card).map_err(|e| e.to_string())?;
        let result = collect_selected(
            Some("selected"),
            150,
            |root| {
                assert!(!root.contains("foreign"));
                // Even a malformed foreign row returned by a recorder cannot cross the boundary.
                Ok(vec![
                    (
                        if root == ROOT {
                            local.clone()
                        } else {
                            selected.clone()
                        },
                        "good".into(),
                    ),
                    (foreign.clone(), "foreign-malformed-bomb".into()),
                ])
            },
            |object, _| {
                assert_eq!(object, "good");
                Ok(json.clone())
            },
        )?;
        assert_eq!(result.rows.len(), 2);
        assert!(result.quarantine.is_empty());
        assert_eq!(source_roots(None)?, vec![ROOT]);
        assert!(source_roots(Some("../foreign")).is_err());
        Ok(())
    }
    #[test]
    fn r2_pure_intent_quarantine_keeps_valid_neighbor_and_enforces_size_identity_path_expiry()
    -> Result<(), String> {
        let valid = card("0123456789abcdef0123456789abcdef");
        let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
        let result = collect_selected(
            None,
            150,
            |_| {
                Ok(vec![
                    (reference(ROOT, &valid), "good".into()),
                    (format!("{ROOT}bad"), "invalid".into()),
                    (format!("{ROOT}huge"), "huge".into()),
                ])
            },
            |object, budget| {
                assert!(budget <= CARD_LIMIT);
                match object {
                    "good" => Ok(json.clone()),
                    "invalid" => Ok("{".into()),
                    _ => Ok("x".repeat(CARD_LIMIT + 1)),
                }
            },
        )?;
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.quarantine.len(), 2);
        for path in [
            "../escape",
            "/root",
            "C:/drive",
            ".git/config",
            "src/../../x",
            "src\\x",
            "src\nnext",
        ] {
            let mut bad = valid.clone();
            bad.path = path.into();
            assert!(
                validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err(),
                "{path}"
            );
        }
        let mut bad = valid.clone();
        bad.expires = bad.created + 86401;
        assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
        bad = valid.clone();
        bad.message = "x".repeat(1025);
        assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
        bad = valid.clone();
        bad.id = "different".into();
        assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
        Ok(())
    }
    #[test]
    fn r2_pure_intent_selection_generation_rejects_read_post_and_error_completions() {
        let mut state = State::default();
        state.cache_key = Some("old".into());
        let old = state.generation;
        state.selection_changed();
        assert!(state.cache_key.is_none());
        assert!(!state.accept(
            old,
            Ok((
                Reading {
                    rows: vec![Seen {
                        reference: "foreign".into(),
                        card: card("0123456789abcdef0123456789abcdef"),
                        local: false
                    }],
                    quarantine: vec![]
                },
                Some("old".into())
            ))
        ));
        assert!(state.rows.is_empty());
        assert!(!state.accept(old, Err("old read failure".into())));
        assert!(state.error.is_none());
        assert!(state.accept(state.generation, Ok((Reading::default(), None))));
        assert!(state.cache_key.is_none());
    }
    #[test]
    fn shared_intents_do_not_move_branches_and_expiry_never_locks() -> Result<(), String> {
        let a = git::stack::test_repo::TestRepo::new("intent-a")?;
        let b = git::stack::test_repo::TestRepo::new("intent-b")?;
        let head = a.tip("HEAD")?;
        post(
            a.path(),
            "src".into(),
            "token".into(),
            "changing lifetime".into(),
            1,
        )?;
        b.git(&[
            "fetch",
            a.path().to_str().ok_or("path")?,
            "refs/gitcraft/intents/*:refs/gitcraft/intent-remotes/test/*",
        ])?;
        let notes = read_active(b.path(), Some("test"))?;
        assert_eq!(notes.len(), 1);
        assert!(applies(&notes[0].card, "src/auth.rs", now()));
        assert!(!applies(&notes[0].card, "src2/auth.rs", now()));
        assert!(!applies(
            &notes[0].card,
            "src/auth.rs",
            notes[0].card.expires
        ));
        assert!(read_active(b.path(), Some("replacement-source"))?.is_empty());
        assert_eq!(read_active(b.path(), Some("test"))?.len(), 1);
        assert_eq!(a.tip("HEAD")?, head);
        Ok(())
    }
}
