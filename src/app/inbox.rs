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

mod round;
use round::*;
mod row;
mod view;
use row::*;

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

#[cfg(test)]
#[path = "inbox/accessibility_tests.rs"]
mod accessibility_tests;
#[cfg(test)]
#[path = "inbox/localization_tests.rs"]
mod localization_tests;
#[cfg(test)]
#[path = "inbox/tests.rs"]
mod tests;
