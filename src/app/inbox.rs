//! The start page's pull-request inbox: across every repository opened (recent ones of every
//! profile, the open tabs) and every forge, the pull requests whose review is asked of you, and
//! yours with failing checks or comments you haven't seen. A click opens one in its
//! repository's tab (in the browser for a forge gitcraft has no page for, or one in a
//! repository not opened here).
//!
//! It asks in two steps, both in the background: which forge each repository is on (git's
//! remotes), then each forge repository once, however many clones of it there are; a forge
//! whose search covers a whole host (Forgejo, Gitea) is asked once per host. It runs unasked,
//! so it uses only logins at hand and never brings up a password or keyring prompt.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{Color32, FontId, Galley, Rect, RichText, Sense, Ui, pos2, vec2};

use super::App;
use crate::across::{self, runner};
use crate::forge::inbox::{self as forge_inbox, Answer, InboxPr, Reason, Seen};
use crate::forge::{self, Forge, Hosting};
use crate::git;
use crate::icons;
use crate::theme::{self, Tokens};

/// How often the inbox asks again by itself while the start page shows.
const ASK_EVERY: Duration = Duration::from_secs(300);

/// How long a forge may take to answer (its tool or API, for one repository or host: two
/// lists, then each of yours), and on top of it what reaching another machine may take.
const ASK_FOR: Duration = Duration::from_secs(120).saturating_add(across::SLACK);

/// How many forges are asked at once, and of one host.
const ASKS: runner::Limits = runner::Limits {
    total: 4,
    per_group: 2,
};

/// How many pull requests the start page lists before "Show N more".
const SHOWN: usize = 8;

/// A repository and the forge it is on.
type Hosted = (PathBuf, Hosting);

/// A batch of jobs running: what they are for, by index, and their results as they come.
struct Batch<T, R> {
    what: Vec<T>,
    rx: Receiver<runner::Done<R>>,
    got: usize,
}

impl<T, R> Batch<T, R> {
    /// The results come so far, and whether that was all of them. All of them are counted
    /// rather than waiting for the channel to close, which may come after the last repaint.
    fn take(&mut self) -> (Vec<runner::Done<R>>, bool) {
        let mut done = Vec::new();
        let mut closed = false;
        loop {
            match self.rx.try_recv() {
                Ok(result) => {
                    self.got += 1;
                    done.push(result);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        (done, closed || self.got >= self.what.len())
    }
}

/// One question of step two: a forge repository, or a whole host for a forge asked so.
#[derive(Clone)]
struct Ask {
    /// The clone the forge's tool runs in.
    repo: PathBuf,
    hosting: Hosting,
    /// Every clone found of it (of any repository there, for a host), for a click to open a
    /// pull request in.
    clones: Vec<Hosted>,
}

impl Ask {
    fn per_host(&self) -> bool {
        self.hosting.forge.provider().inbox_per_host()
    }

    /// What giving up on it goes by: the forge and the host.
    fn place(&self) -> (Forge, String) {
        (self.hosting.forge, self.hosting.host.clone())
    }

    /// "GitHub · owner/repo", or "Forgejo · codeberg.org" for a host.
    fn where_is(&self) -> String {
        let what = if self.per_host() {
            &self.hosting.host
        } else {
            &self.hosting.path
        };
        format!("{} · {what}", self.hosting.forge.name())
    }

    /// The start of the addresses of the pull requests it answers for (lowercase): the host's,
    /// or the repository's.
    fn covers(&self) -> String {
        let web = self.hosting.web.trim_end_matches('/');
        let at = if self.per_host() {
            format!("{web}/")
        } else {
            format!("{web}/{}/", self.hosting.path)
        };
        at.to_lowercase()
    }

    /// The clone to open `pr` in: the one asked through, or for a host the clone of the
    /// repository it is in (none when none was opened here).
    fn clone_for(&self, pr: &InboxPr) -> Option<PathBuf> {
        if !self.per_host() {
            return Some(self.repo.clone());
        }
        self.clones
            .iter()
            .find(|(_, h)| !pr.project.is_empty() && h.path.eq_ignore_ascii_case(&pr.project))
            .map(|(repo, _)| repo.clone())
    }
}

/// One pull request in the list: the clone to open it in, its forge, and it.
#[derive(Clone)]
struct Item {
    repo: Option<PathBuf>,
    forge: Forge,
    pr: InboxPr,
}

/// A forge that couldn't be asked: where, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Problem {
    forge: Forge,
    host: String,
    at: String,
    why: String,
}

/// A pull request to open once its tab is there and has read its forge.
struct Opening {
    repo: PathBuf,
    number: u64,
    url: String,
}

#[derive(Default)]
pub(super) struct Inbox {
    /// Step one running: which forge each repository is on.
    finding: Option<Batch<PathBuf, Option<Hosting>>>,
    found: Vec<Hosted>,
    /// Step two running.
    asking: Option<Batch<Ask, Answer>>,
    /// What the last round found, and what this one has so far.
    items: Vec<Item>,
    next: Vec<Item>,
    problems: Vec<Problem>,
    next_problems: Vec<Problem>,
    /// What was left unread ("GitHub · o/r: only the 50 latest … were read").
    cut: Vec<String>,
    next_cut: Vec<String>,
    /// The addresses the last round's answers cover (see [`Ask::covers`]).
    covered: Vec<String>,
    next_covered: Vec<String>,
    /// Forges and hosts not asked again until ↻: their tool isn't installed, or there's no
    /// login. What they said stands for them meanwhile.
    given_up: HashMap<(Forge, String), Problem>,
    /// When the last round started.
    asked_at: Option<Instant>,
    opening: Option<Opening>,
    /// The whole list shown, not only the first [`SHOWN`].
    show_all: bool,
}

impl Inbox {
    fn busy(&self) -> bool {
        self.finding.is_some() || self.asking.is_some()
    }

    /// Starts a round over `repos`, unless one runs; `again` (↻) asks also the forges given
    /// up on.
    fn ask(&mut self, repos: Vec<PathBuf>, again: bool, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        if again {
            self.given_up.clear();
        }
        self.asked_at = Some(Instant::now());
        self.next.clear();
        self.next_problems.clear();
        self.next_cut.clear();
        self.next_covered.clear();
        self.found.clear();
        let wake = ctx.clone();
        let rx = runner::run_bounded(
            repos.clone(),
            across::LOOKS,
            across::LOOK_WAIT,
            |p: &PathBuf| across::machine_key(p),
            |p: PathBuf| Ok(forge::detect(&p, &git::remotes(&p)?)),
            move || wake.request_repaint(),
        );
        self.finding = Some(Batch {
            what: repos,
            rx,
            got: 0,
        });
    }

    /// Takes in what came; says when a round has just ended.
    fn poll(&mut self, ctx: &egui::Context) -> bool {
        if let Some(batch) = &mut self.finding {
            let (done, all) = batch.take();
            for (i, result) in done {
                // Not on a forge, or not readable now: nothing to ask.
                if let (Ok(Some(hosting)), Some(repo)) = (result, batch.what.get(i)) {
                    self.found.push((repo.clone(), hosting));
                }
            }
            if all {
                self.finding = None;
                self.start_asking(ctx);
            }
        }
        let Some(batch) = &mut self.asking else {
            return false;
        };
        let (done, all) = batch.take();
        for (i, result) in done {
            let Some(ask) = batch.what.get(i) else {
                continue;
            };
            match result {
                Ok(answer) => {
                    self.next.extend(answer.prs.into_iter().map(|pr| Item {
                        repo: ask.clone_for(&pr),
                        forge: ask.hosting.forge,
                        pr,
                    }));
                    let at = ask.where_is();
                    self.next_cut
                        .extend(answer.cut.iter().map(|c| format!("{at}: {c}")));
                    self.next_covered.push(ask.covers());
                }
                Err(why) => {
                    let problem = Problem {
                        forge: ask.hosting.forge,
                        host: ask.hosting.host.clone(),
                        at: ask.where_is(),
                        why,
                    };
                    if forge_inbox::gives_up(&problem.why) {
                        self.given_up.insert(ask.place(), problem.clone());
                    }
                    self.next_problems.push(problem);
                }
            }
        }
        if !all {
            return false;
        }
        self.asking = None;
        self.end_round();
        true
    }

    fn end_round(&mut self) {
        self.items = dedupe(std::mem::take(&mut self.next));
        self.problems = std::mem::take(&mut self.next_problems);
        self.cut = std::mem::take(&mut self.next_cut);
        self.covered = std::mem::take(&mut self.next_covered);
    }

    /// Asks each forge repository (or host) once; those given up on say again why instead.
    fn start_asking(&mut self, ctx: &egui::Context) {
        let mut todo = Vec::new();
        for ask in asks(std::mem::take(&mut self.found)) {
            match self.given_up.get(&ask.place()) {
                Some(problem) => self.next_problems.push(problem.clone()),
                None => todo.push(ask),
            }
        }
        if todo.is_empty() {
            self.end_round();
            return;
        }
        let wake = ctx.clone();
        let rx = runner::run_bounded(
            todo.clone(),
            ASKS,
            ASK_FOR,
            |ask: &Ask| ask.hosting.host.clone(),
            |ask: Ask| {
                // One at a time each: one that hangs past its time isn't asked again on top
                // of it.
                let key = if ask.per_host() {
                    PathBuf::from(format!("{}:{}", ask.hosting.forge.key(), ask.hosting.web))
                } else {
                    ask.repo.clone()
                };
                across::once_at_a_time("inbox", &key, || {
                    ask.hosting.forge.provider().inbox(&ask.repo, &ask.hosting)
                })
            },
            move || wake.request_repaint(),
        );
        self.asking = Some(Batch {
            what: todo,
            rx,
            got: 0,
        });
    }
}

/// The questions of step two: each forge repository once (through the first clone of it met),
/// and a forge that answers for a whole host once per host.
fn asks(found: Vec<Hosted>) -> Vec<Ask> {
    let mut asks: Vec<Ask> = Vec::new();
    for (repo, hosting) in found {
        let per_host = hosting.forge.provider().inbox_per_host();
        let same = asks.iter_mut().find(|a| {
            a.hosting.forge == hosting.forge
                && a.hosting.host == hosting.host
                && (per_host || a.hosting.path == hosting.path)
        });
        match same {
            Some(ask) => ask.clones.push((repo, hosting)),
            None => asks.push(Ask {
                repo: repo.clone(),
                hosting: hosting.clone(),
                clones: vec![(repo, hosting)],
            }),
        }
    }
    asks
}

/// Each pull request once, the most recently changed first.
fn dedupe(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut items: Vec<Item> = items
        .into_iter()
        .filter(|i| seen.insert(i.pr.url.clone()))
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.pr.updated));
    items
}

/// The items that need you now, with why, given what was seen of them.
fn waiting<'a>(items: &'a [Item], seen: &BTreeMap<String, Seen>) -> Vec<(&'a Item, Vec<Reason>)> {
    items
        .iter()
        .map(|i| {
            (
                i,
                forge_inbox::needs_you(&i.pr, seen.get(&i.pr.url).copied()),
            )
        })
        .filter(|(_, reasons)| !reasons.is_empty())
        .collect()
}

/// After a round: yours met for the first time are seen as they are, so that only what is
/// said from now on is news (not every comment they ever had); and what was seen of pull
/// requests no longer listed (merged, closed) is let go, where the forge that would list them
/// answered.
fn remember_round(seen: &mut BTreeMap<String, Seen>, items: &[Item], covered: &[String]) {
    for item in items.iter().filter(|i| i.pr.mine) {
        if !seen.contains_key(&item.pr.url) {
            seen.insert(item.pr.url.clone(), forge_inbox::seen_now(&item.pr, None));
        }
    }
    let listed: HashSet<&str> = items.iter().map(|i| i.pr.url.as_str()).collect();
    seen.retain(|url, _| {
        let url_lower = url.to_lowercase();
        listed.contains(url.as_str()) || !covered.iter().any(|c| url_lower.starts_with(c))
    });
}

/// The problems once each by forge, host and reason, with where each was met.
fn grouped(problems: &[&Problem]) -> Vec<String> {
    let mut groups: Vec<(&Problem, Vec<&str>)> = Vec::new();
    for p in problems {
        match groups
            .iter_mut()
            .find(|(g, _)| g.forge == p.forge && g.host == p.host && g.why == p.why)
        {
            Some((_, at)) => {
                if !at.contains(&p.at.as_str()) {
                    at.push(&p.at);
                }
            }
            None => groups.push((p, vec![&p.at])),
        }
    }
    groups
        .into_iter()
        .map(|(p, at)| format!("{}: {}", at.join(", "), forge_inbox::display_error(&p.why)))
        .collect()
}

impl App {
    /// The repositories the inbox goes through: those opened, of every profile, reachable
    /// without asking for a password (here, or on a machine connected now) unless `all`.
    fn inbox_repos(&self, all: bool) -> Vec<PathBuf> {
        let open = self.open_repos().0;
        let others: Vec<PathBuf> = self
            .profile_states
            .values()
            .flat_map(|s| s.recent.iter().chain(&s.open).cloned())
            .collect();
        let mut seen = HashSet::new();
        self.recent
            .iter()
            .chain(&open)
            .chain(&others)
            .filter(|p| seen.insert((*p).clone()))
            .filter(|p| {
                all || crate::remote::parse(p).is_none_or(|t| crate::remote::is_connected(&t.host))
            })
            .cloned()
            .collect()
    }

    /// The inbox on the start page.
    pub(super) fn inbox_section(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        if self.inbox.poll(&ctx) {
            remember_round(&mut self.inbox_seen, &self.inbox.items, &self.inbox.covered);
        }
        match self.inbox.asked_at.map(|at| at.elapsed()) {
            Some(since) if since <= ASK_EVERY => {
                // Idle, the window draws nothing: it's woken for the next round. While a
                // round runs, each answer wakes it.
                if !self.inbox.busy() {
                    ctx.request_repaint_after(ASK_EVERY.saturating_sub(since));
                }
            }
            _ => {
                let repos = self.inbox_repos(false);
                self.inbox.ask(repos, false, &ctx);
            }
        }
        let mut refresh = false;
        let mut open = None;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-inbox-heading"))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                if self.inbox.busy() {
                    ui.add(crate::spinner::Spinner::new().size(11.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = crate::i18n::text("interaction-home-ask-again");
                    refresh = icons::small_button(ui, "refresh-cw", tip).clicked();
                });
            });
            let (quiet, failed): (Vec<&Problem>, Vec<&Problem>) = self
                .inbox
                .problems
                .iter()
                .partition(|p| forge_inbox::is_no_login(&p.why));
            let list = waiting(&self.inbox.items, &self.inbox_seen);
            // All clear only when every forge answered.
            if list.is_empty()
                && failed.is_empty()
                && !self.inbox.busy()
                && self.inbox.asked_at.is_some()
            {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-all-clear"))
                        .color(t.text_faint)
                        .small(),
                );
            }
            ui.spacing_mut().item_spacing.y = 2.0;
            let total = list.len();
            let shown = if self.inbox.show_all {
                total
            } else {
                total.min(SHOWN)
            };
            for (item, reasons) in list.into_iter().take(shown) {
                if inbox_row(ui, item, &reasons, &t).clicked() {
                    open = Some(item.clone());
                }
            }
            if total > SHOWN {
                let text = if self.inbox.show_all {
                    crate::i18n::text("interaction-home-fewer").to_owned()
                } else {
                    crate::i18n::message(
                        "interaction-home-show-more",
                        &[("count", (total - SHOWN).into())],
                    )
                };
                if ui.link(RichText::new(text).small()).clicked() {
                    self.inbox.show_all = !self.inbox.show_all;
                }
            }
            let failed = grouped(&failed);
            if !failed.is_empty() {
                let text = crate::i18n::message(
                    "interaction-home-ask-failed",
                    &[("count", failed.len().into())],
                );
                ui.label(RichText::new(text).color(t.text_faint).small())
                    .on_hover_text(failed.join("\n\n"));
            }
            let mut unread = grouped(&quiet);
            unread.extend(self.inbox.cut.iter().cloned());
            if !unread.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-unread"))
                        .color(t.text_faint)
                        .small(),
                )
                .on_hover_text(unread.join("\n\n"));
            }
        });
        if refresh {
            let repos = self.inbox_repos(true);
            self.inbox.ask(repos, true, &ctx);
        }
        if let Some(item) = open {
            self.open_from_inbox(item, &ctx);
        }
    }

    /// Opens `item` in its repository's tab (once the tab has read its forge), or in the
    /// browser for a forge gitcraft shows no pull requests of, or with no clone opened here;
    /// what it said so far is seen.
    fn open_from_inbox(&mut self, item: Item, ctx: &egui::Context) {
        let before = self.inbox_seen.get(&item.pr.url).copied();
        let seen = forge_inbox::seen_now(&item.pr, before);
        self.inbox_seen.insert(item.pr.url.clone(), seen);
        let browser = || ctx.open_url(egui::OpenUrl::new_tab(&item.pr.url));
        let Some(repo) = item.repo.filter(|_| item.forge.has_pages()) else {
            browser();
            return;
        };
        self.open_repo(&repo, ctx);
        // Only once there is a tab, or one being connected to: a repository that couldn't be
        // opened (it said why) leaves the browser.
        if self.has_repo_tab(&repo) || self.connecting_to(&repo) {
            self.inbox.opening = Some(Opening {
                repo,
                number: item.pr.number,
                url: item.pr.url,
            });
        } else {
            browser();
        }
    }

    fn has_repo_tab(&self, repo: &std::path::Path) -> bool {
        self.tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
            .any(|t| matches!(t, super::Tab::Repo(r) if r.path == repo))
    }

    #[cfg(test)]
    pub(super) fn recorded_inbox_open(&mut self, repo: PathBuf, ctx: &egui::Context) -> bool {
        self.open_from_inbox(
            Item {
                repo: Some(repo),
                forge: Forge::GitHub,
                pr: InboxPr {
                    number: 7,
                    url: "https://github.com/o/r/pull/7".into(),
                    ..Default::default()
                },
            },
            ctx,
        );
        self.inbox
            .opening
            .as_ref()
            .is_some_and(|opening| opening.number == 7)
    }

    fn connecting_to(&self, repo: &std::path::Path) -> bool {
        let label = repo.display().to_string();
        self.connecting.iter().any(|c| c.label == label)
    }

    /// Opens the pull request asked from the inbox once its tab has read its forge: in the
    /// tab when it shows that forge's pull requests, else in the browser. Given up when the
    /// tab goes (closed, another profile) or connecting to it fails. However long connecting
    /// or a password takes, it waits.
    pub(super) fn poll_inbox_opening(&mut self, ctx: &egui::Context) {
        let Some(opening) = &self.inbox.opening else {
            return;
        };
        let connecting = self.connecting_to(&opening.repo);
        let tab = self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
            .find_map(|t| match t {
                super::Tab::Repo(r) if r.path == opening.repo => Some(r),
                _ => None,
            });
        match tab {
            Some(tab) if tab.shows_pull_requests() => {
                tab.queue(crate::repo::Action::OpenPr(opening.number));
                self.inbox.opening = None;
                ctx.request_repaint();
            }
            // Read, and on no forge gitcraft has pages for (or it failed to read).
            Some(tab) if !tab.opening() => {
                ctx.open_url(egui::OpenUrl::new_tab(&opening.url));
                self.inbox.opening = None;
            }
            // Loading, or being connected to: what it reads, or the connection, wakes the
            // window.
            Some(_) => {}
            None if connecting => {}
            None => self.inbox.opening = None,
        }
    }
}

/// `text` on one line, cut with an ellipsis at `width`.
fn one_line(ui: &Ui, text: String, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job =
        egui::text::LayoutJob::single_section(text, egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// One pull request: its number and title, its repository and author, and why it's here.
fn inbox_row(ui: &mut Ui, item: &Item, reasons: &[Reason], t: &Tokens) -> egui::Response {
    let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 40.0));
    let resp = ui.interact(
        rect,
        ui.id().with(("inbox-row", item.forge, &item.pr.url)),
        Sense::click(),
    );
    crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
        let reasons = reasons
            .iter()
            .map(|reason| reason.label())
            .collect::<Vec<_>>()
            .join(", ");
        crate::i18n::message(
            "interaction-home-inbox-a11y",
            &[
                ("forge", item.forge.name().into()),
                ("noun", item.forge.noun_label().into()),
                ("number", item.pr.number.to_string().into()),
                ("title", item.pr.title.as_str().into()),
                ("author", item.pr.author.as_str().into()),
                ("project", item.pr.project.as_str().into()),
                ("reasons", reasons.into()),
            ],
        )
    });
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 20.0, y), vec2(18.0, 18.0)),
        "git-pull-request",
        16.0,
        t.icon,
    );
    // The reasons on the right, the first always and the others as far as half the row
    // goes; the title keeps what is left.
    let font = FontId::proportional(11.0);
    let labels: Vec<(Arc<Galley>, Color32)> = reasons
        .iter()
        .map(|reason| {
            let color = match reason {
                Reason::ReviewRequested => t.accent,
                Reason::ChecksFailing => t.danger,
                Reason::NewComments(_) => t.warning,
            };
            let galley = ui
                .painter()
                .layout_no_wrap(reason.label(), font.clone(), color);
            (galley, color)
        })
        .collect();
    let mut used = 0.0;
    let mut fit = 0;
    for (galley, _) in &labels {
        let w = galley.size().x + if fit > 0 { 12.0 } else { 0.0 };
        if fit > 0 && used + w > rect.width() * 0.5 {
            break;
        }
        used += w;
        fit += 1;
    }
    let mut x = rect.right() - 8.0;
    for (galley, color) in labels.into_iter().take(fit).rev() {
        x -= galley.size().x;
        let at = pos2(x, y - galley.size().y / 2.0);
        ui.painter().galley(at, galley, color);
        x -= 12.0;
    }
    let left = rect.left() + 40.0;
    let width = x - 4.0 - left;
    let title = one_line(
        ui,
        crate::i18n::message(
            if item.pr.draft {
                "interaction-home-pr-draft-title"
            } else {
                "interaction-home-pr-title"
            },
            &[
                ("number", item.pr.number.to_string().into()),
                ("title", item.pr.title.as_str().into()),
            ],
        ),
        theme::medium(12.5),
        t.text,
        width,
    );
    ui.painter()
        .galley(pos2(left, y - 8.0 - title.size().y / 2.0), title, t.text);
    let repo = match &item.repo {
        Some(path) => super::workspaces::repo_name(path),
        None => item.pr.project.clone(),
    };
    let sub = one_line(
        ui,
        format!("{repo} · {} · {}", item.forge.name(), item.pr.author),
        FontId::proportional(11.0),
        t.text_faint,
        width,
    );
    ui.painter()
        .galley(pos2(left, y + 8.0 - sub.size().y / 2.0), sub, t.text_faint);
    let how = if item.forge.has_pages() && item.repo.is_some() {
        crate::i18n::text("interaction-home-open-tab")
    } else {
        crate::i18n::text("interaction-home-open-browser")
    };
    resp.on_hover_text(format!("{}\n{how}", item.pr.url))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosting(forge: Forge, host: &str, path: &str) -> Hosting {
        Hosting {
            forge,
            host: host.into(),
            path: path.into(),
            web: format!("https://{host}"),
        }
    }

    fn item(url: &str, updated: i64, mine: bool, requested: bool) -> Item {
        Item {
            repo: Some(PathBuf::from("/r")),
            forge: Forge::GitHub,
            pr: InboxPr {
                url: url.into(),
                updated,
                mine,
                review_requested: requested,
                said: vec![100],
                ..InboxPr::default()
            },
        }
    }

    #[test]
    fn each_forge_repository_is_asked_once_and_a_searchable_host_once() {
        let found = vec![
            (
                PathBuf::from("/a"),
                hosting(Forge::GitHub, "github.com", "o/r"),
            ),
            (
                PathBuf::from("/a-clone"),
                hosting(Forge::GitHub, "github.com", "o/r"),
            ),
            (
                PathBuf::from("/b"),
                hosting(Forge::GitHub, "github.com", "o/other"),
            ),
            (
                PathBuf::from("/fork"),
                hosting(Forge::Forgejo, "codeberg.org", "me/app"),
            ),
            (
                PathBuf::from("/up"),
                hosting(Forge::Forgejo, "codeberg.org", "up/app"),
            ),
        ];
        let todo = asks(found);
        let repos: Vec<_> = todo.iter().map(|a| a.repo.clone()).collect();
        assert_eq!(
            repos,
            [
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/fork")
            ]
        );
        assert_eq!(todo[2].where_is(), "Forgejo · codeberg.org");
        assert_eq!(todo[2].covers(), "https://codeberg.org/");
        assert_eq!(todo[0].covers(), "https://github.com/o/r/");
        // A pull request found on the host opens in the clone of its repository, if any.
        let upstream = InboxPr {
            project: "Up/App".into(),
            ..InboxPr::default()
        };
        assert_eq!(todo[2].clone_for(&upstream), Some(PathBuf::from("/up")));
        let elsewhere = InboxPr {
            project: "x/y".into(),
            ..InboxPr::default()
        };
        assert_eq!(todo[2].clone_for(&elsewhere), None);
        assert_eq!(todo[0].clone_for(&elsewhere), Some(PathBuf::from("/a")));
    }

    #[test]
    fn the_list_is_each_once_newest_first_and_only_what_needs_you() {
        let items = dedupe(vec![
            item("u1", 10, false, true),
            item("u2", 30, true, false),
            item("u1", 10, false, true),
            item("u3", 20, false, false),
        ]);
        let urls: Vec<&str> = items.iter().map(|i| i.pr.url.as_str()).collect();
        assert_eq!(urls, ["u2", "u3", "u1"]);
        let mut seen = BTreeMap::new();
        let list = waiting(&items, &seen);
        let urls: Vec<&str> = list.iter().map(|(i, _)| i.pr.url.as_str()).collect();
        assert_eq!(urls, ["u2", "u1"], "u3 is neither yours nor asks you");
        // Opened from the inbox: its comment is seen.
        seen.insert("u2".to_owned(), Seen { at: 100 });
        let list = waiting(&items, &seen);
        assert_eq!(list.len(), 1);
    }

    /// The first time one of yours is met, what it already had isn't news; what is no longer
    /// listed is let go where its forge answered, kept where it didn't.
    #[test]
    fn a_round_remembers_yours_and_lets_go_of_the_gone() {
        let url = |n: u32| format!("https://github.com/o/r/pull/{n}");
        let mut seen = BTreeMap::new();
        seen.insert(url(9), Seen { at: 5 });
        seen.insert(
            "https://codeberg.org/x/y/pulls/1".to_owned(),
            Seen { at: 5 },
        );
        let mut first = item(&url(1), 50, true, false);
        first.pr.said = vec![40, 45];
        let asked = item(&url(2), 50, false, true);
        let items = vec![first, asked];
        remember_round(&mut seen, &items, &["https://github.com/o/r/".to_owned()]);
        assert_eq!(seen.get(&url(1)), Some(&Seen { at: 50 }));
        assert!(
            waiting(&items, &seen)
                .iter()
                .all(|(i, _)| i.pr.url == url(2))
        );
        assert!(!seen.contains_key(&url(2)), "only yours");
        assert!(!seen.contains_key(&url(9)), "merged or closed since");
        assert!(seen.contains_key("https://codeberg.org/x/y/pulls/1"));
        // A comment after that is new.
        let mut items = items;
        items[0].pr.said.push(60);
        let list = waiting(&items, &seen);
        assert_eq!(list[0].1, vec![Reason::NewComments(1)]);
    }

    #[test]
    fn problems_are_said_once_each() {
        let problem = |at: &str, why: &str| Problem {
            forge: Forge::GitHub,
            host: "github.com".into(),
            at: at.into(),
            why: why.into(),
        };
        let a = problem("GitHub · o/r", "gh isn't installed");
        let b = problem("GitHub · o/s", "gh isn't installed");
        let c = problem("GitHub · o/s", "HTTP 502");
        assert_eq!(
            grouped(&[&a, &b, &c, &a]),
            [
                "GitHub · o/r, GitHub · o/s: gh isn't installed",
                "GitHub · o/s: HTTP 502"
            ]
        );
    }

    /// A round over repositories on no forge (or not repositories) ends, with nothing to ask.
    #[test]
    fn a_round_over_repositories_on_no_forge_ends_empty() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["init", "-q"])?;
        git::run(Some(&dir), &["remote", "add", "origin", "/srv/plain.git"])?;
        let ctx = egui::Context::default();
        let mut inbox = Inbox::default();
        inbox.ask(vec![dir.clone(), dir.join("missing")], false, &ctx);
        assert!(inbox.busy());
        // A second round isn't started over a running one.
        inbox.ask(vec![dir.clone()], false, &ctx);
        let until = Instant::now() + Duration::from_secs(30);
        let mut ended = false;
        while inbox.busy() {
            if Instant::now() > until {
                return Err("the round never ended".to_owned());
            }
            ended |= inbox.poll(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(inbox.items.is_empty() && inbox.problems.is_empty());
        // Nothing asked of a forge: it ended in step one.
        assert!(!ended);
        Ok(())
    }

    /// A forge given up on (no tool, no login) isn't asked again until ↻, and still counts as
    /// not asked.
    #[test]
    fn a_forge_given_up_on_is_left_until_asked_again() {
        let ctx = egui::Context::default();
        let mut inbox = Inbox::default();
        let gone = Problem {
            forge: Forge::GitHub,
            host: "github.com".into(),
            at: "GitHub · o/r".into(),
            why: "gh isn't installed: gitcraft uses it for pull requests".into(),
        };
        inbox
            .given_up
            .insert((Forge::GitHub, "github.com".into()), gone.clone());
        inbox.found = vec![(
            PathBuf::from("/a"),
            hosting(Forge::GitHub, "github.com", "o/r"),
        )];
        inbox.start_asking(&ctx);
        assert!(!inbox.busy(), "not asked");
        assert_eq!(inbox.problems, [gone]);
        inbox.ask(Vec::new(), true, &ctx);
        assert!(inbox.given_up.is_empty());
    }
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn inbox_exposes_forge_repository_author_and_reasons_and_activates() {
        let item = Item {
            repo: None,
            forge: Forge::GitHub,
            pr: InboxPr {
                number: 42,
                title: "Fix 日本語".into(),
                url: "https://github.com/a/b/pull/42".into(),
                author: "Jonas".into(),
                project: "a/b".into(),
                ..Default::default()
            },
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, opened: &mut bool| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                *opened |= inbox_row(
                    ui,
                    &item,
                    &[Reason::ReviewRequested, Reason::ChecksFailing],
                    &t,
                )
                .clicked();
                crate::a11y::end_frame(ui.ctx());
            },
            false,
        );
        h.run();
        let label =
            "GitHub pull request 42: Fix 日本語, by Jonas, a/b. review requested, checks failing";
        h.get_by_role_and_label(egui::accesskit::Role::Button, label);
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label(label).is_focused());
        h.key_press(egui::Key::Enter);
        h.run();
        assert!(*h.state());
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn german_actual_row_activates_with_opaque_title_number_and_identity() {
        crate::i18n::set_language("de");
        let item = Item {
            repo: None,
            forge: Forge::GitLab,
            pr: InboxPr {
                number: 1234,
                title: "Fix 日本語".into(),
                url: "https://literal-host/a/b/-/merge_requests/1234".into(),
                author: "Jonas".into(),
                project: "a/b".into(),
                ..Default::default()
            },
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, opened: &mut bool| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                *opened |= inbox_row(
                    ui,
                    &item,
                    &[Reason::ReviewRequested, Reason::NewComments(1234)],
                    &t,
                )
                .clicked();
                crate::a11y::end_frame(ui.ctx());
                assert_eq!(item.forge.noun(), "merge request");
                assert_eq!(
                    item.pr.url,
                    "https://literal-host/a/b/-/merge_requests/1234"
                );
            },
            false,
        );
        h.run();
        let label = "GitLab Merge Request 1234: Fix 日本語, von Jonas, a/b. Review angefordert, 1.234 neue Kommentare";
        h.get_by_role_and_label(egui::accesskit::Role::Button, label);
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label(label).is_focused());
        h.key_press(egui::Key::Enter);
        h.run();
        assert!(*h.state());
    }

    #[test]
    fn protocol_errors_are_grouped_raw_and_only_display_is_translated() {
        crate::i18n::set_language("de");
        let raw = "no Forgejo login known for codeberg.org: add one with `tea login add`";
        let problem = Problem {
            forge: Forge::Forgejo,
            host: "codeberg.org".into(),
            at: "Forgejo · codeberg.org".into(),
            why: raw.into(),
        };
        assert!(forge_inbox::is_no_login(&problem.why));
        assert!(forge_inbox::gives_up(&problem.why));
        let shown = grouped(&[&problem]);
        assert!(shown[0].contains("Für codeberg.org ist keine Forgejo-Anmeldung bekannt"));
        assert_eq!(problem.why, raw);
        assert!(forge_inbox::is_no_login(&problem.why));
        assert!(forge_inbox::gives_up(&problem.why));
        assert_eq!(
            forge_inbox::display_error("HTTP 502 literal"),
            "HTTP 502 literal"
        );
    }
}
