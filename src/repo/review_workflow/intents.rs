//! Advisory, expiring notes exchanged as ordinary fast-forward Git refs, never locks.
use super::tools::ToolsAct;
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
    initialized: bool,
    busy: bool,
    rows: Vec<Seen>,
    error: Option<String>,
    remote: String,
    path: String,
    symbol: String,
    message: String,
    minutes: u32,
    synced: u64,
    checked: u64,
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
    save(
        repo,
        &reference,
        &Card {
            id,
            author,
            email,
            path,
            symbol,
            message,
            created,
            expires: created.saturating_add(u64::from(minutes.clamp(1, 1440)) * 60),
        },
    )
}
fn read(repo: &Path) -> Result<Vec<Seen>, String> {
    let mut cards = BTreeMap::<String, Seen>::new();
    for root in [ROOT, "refs/gitcraft/intent-remotes/"] {
        let listing = git::run(Some(repo), &["for-each-ref", "--format=%(refname)", root])?;
        if listing.lines().count() > 5000 {
            return Err(
                "More than 5000 shared intent refs; archive old intents before loading".into(),
            );
        }
        for reference in listing.lines() {
            let text = git::run(Some(repo), &["show", &format!("{reference}:intent.json")])?;
            let card: Card =
                serde_json::from_str(&text).map_err(|e| format!("Invalid shared intent: {e}"))?;
            let seen = Seen {
                reference: reference.into(),
                card,
                local: root == ROOT,
            };
            match cards.get(&seen.card.id) {
                Some(old) if old.local || old.card.created >= seen.card.created => {}
                _ => {
                    cards.insert(seen.card.id.clone(), seen);
                }
            }
        }
    }
    Ok(cards.into_values().collect())
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
        if let Act::Sync(remote) | Act::Publish(remote) = &act {
            if !self.remotes.iter().any(|r| &r.name == remote) {
                self.review_tools.intents.error = Some("Pick a configured remote".into());
                return;
            }
            if let Some(p) = network::refused() {
                self.review_tools.intents.error = Some(p.why().into());
                return;
            }
        }
        self.review_tools.intents.busy = true;
        self.review_tools.intents.checked = now();
        let (repo, creds) = (self.path.clone(), self.credentials.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
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
                    Act::Sync(remote) => {
                        let destination =
                            format!("refs/gitcraft/intent-remotes/{:x}", md5::compute(&remote));
                        git::run_unattended(
                            Some(&repo),
                            &[
                                "fetch",
                                "--no-tags",
                                "--no-write-fetch-head",
                                &remote,
                                &format!("{ROOT}*:{destination}/*"),
                            ],
                            creds.as_ref(),
                        )?;
                    }
                    Act::Publish(remote) => {
                        let (_, email) = identity(&repo)?;
                        let mine = format!("{ROOT}{:x}", md5::compute(email));
                        git::run_unattended(
                            Some(&repo),
                            &["push", &remote, &format!("{mine}/*:{mine}/*")],
                            creds.as_ref(),
                        )?;
                    }
                    Act::Read => {}
                }
                read(&repo)
            },
            move |tab, result, _| {
                let s = &mut tab.review_tools.intents;
                s.busy = false;
                match result {
                    Ok(rows) => {
                        s.rows = rows;
                        s.error = None;
                        if is_network {
                            s.synced = now();
                        }
                    }
                    Err(e) => s.error = Some(e),
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
        let notes = read(b.path())?;
        assert_eq!(notes.len(), 1);
        assert!(applies(&notes[0].card, "src/auth.rs", now()));
        assert!(!applies(&notes[0].card, "src2/auth.rs", now()));
        assert!(!applies(
            &notes[0].card,
            "src/auth.rs",
            notes[0].card.expires
        ));
        assert_eq!(a.tip("HEAD")?, head);
        Ok(())
    }
}
