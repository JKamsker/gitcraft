//! The start page's pull-request inbox: across every repository opened (recent ones of every
//! profile, the open tabs) and every forge, the pull requests whose review is asked of you, and
//! yours with failing checks or comments you haven't seen. A click opens one in its
//! repository's tab (in the browser for a forge gitcraft has no page for).
//!
//! It asks in two steps, both in the background: which forge each repository is on (git's
//! remotes), then each forge repository once, however many clones of it there are.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{Align2, Rect, RichText, Sense, Ui, pos2, vec2};

use super::App;
use crate::across::{self, runner};
use crate::forge::inbox::{self as forge_inbox, InboxPr, Reason, Seen};
use crate::forge::{self, Forge, Hosting};
use crate::git;
use crate::icons;
use crate::theme::{self, Tokens};

/// How often the inbox asks again by itself while the start page shows.
const ASK_EVERY: Duration = Duration::from_secs(300);

/// How long a forge may take to answer (its tool or API, for one repository).
const ASK_FOR: Duration = Duration::from_secs(90);

/// How long a pull request opened from the inbox waits for its tab to know its forge.
const OPEN_WAIT: Duration = Duration::from_secs(45);

/// How many forges are asked at once, and of one host.
const ASKS: runner::Limits = runner::Limits {
    total: 4,
    per_group: 2,
};

/// A repository and the forge it is on.
type Hosted = (PathBuf, Hosting);

/// A batch of jobs running: what they are for, by index, and their results.
type Running<T, R> = Option<(Vec<T>, Receiver<runner::Done<R>>)>;

/// One pull request in the list: the repository it was found through, its forge, and it.
#[derive(Clone)]
struct Item {
    repo: PathBuf,
    forge: Forge,
    pr: InboxPr,
}

#[derive(Default)]
pub(super) struct Inbox {
    /// Step one running: which forge each repository is on.
    finding: Running<PathBuf, Option<Hosting>>,
    found: Vec<Hosted>,
    /// Step two running: each forge repository asked.
    asking: Running<Hosted, Vec<InboxPr>>,
    /// What the last round found, and what this one has so far.
    items: Vec<Item>,
    next: Vec<Item>,
    /// Forges that couldn't be asked: (where, why).
    errors: Vec<(String, String)>,
    next_errors: Vec<(String, String)>,
    /// When the last round started.
    asked_at: Option<Instant>,
    /// A pull request to open once its tab knows its forge.
    opening: Option<(PathBuf, u64, String, Instant)>,
}

impl Inbox {
    fn busy(&self) -> bool {
        self.finding.is_some() || self.asking.is_some()
    }

    /// Starts a round over `repos`, unless one runs.
    fn ask(&mut self, repos: Vec<PathBuf>, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.asked_at = Some(Instant::now());
        self.next.clear();
        self.next_errors.clear();
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
        self.finding = Some((repos, rx));
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some((repos, rx)) = &self.finding {
            loop {
                match rx.try_recv() {
                    Ok((i, Ok(Some(hosting)))) => {
                        if let Some(repo) = repos.get(i) {
                            self.found.push((repo.clone(), hosting));
                        }
                    }
                    // Not on a forge, or not readable now: nothing to ask.
                    Ok(_) => {}
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.finding = None;
                        self.start_asking(ctx);
                        break;
                    }
                }
            }
        }
        if let Some((asked, rx)) = &self.asking {
            loop {
                match rx.try_recv() {
                    Ok((i, result)) => {
                        let Some((repo, hosting)) = asked.get(i) else {
                            continue;
                        };
                        match result {
                            Ok(prs) => self.next.extend(prs.into_iter().map(|pr| Item {
                                repo: repo.clone(),
                                forge: hosting.forge,
                                pr,
                            })),
                            Err(e) => self.next_errors.push((where_is(hosting), e)),
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.asking = None;
                        self.items = dedupe(std::mem::take(&mut self.next));
                        self.errors = std::mem::take(&mut self.next_errors);
                        break;
                    }
                }
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Asks each forge repository once, through the first clone of it found.
    fn start_asking(&mut self, ctx: &egui::Context) {
        let todo = unique_hostings(std::mem::take(&mut self.found));
        if todo.is_empty() {
            self.items.clear();
            self.errors.clear();
            return;
        }
        let wake = ctx.clone();
        let rx = runner::run_bounded(
            todo.clone(),
            ASKS,
            ASK_FOR,
            |(_, h): &Hosted| h.host.clone(),
            |(repo, hosting): Hosted| hosting.forge.provider().inbox(&repo, &hosting),
            move || wake.request_repaint(),
        );
        self.asking = Some((todo, rx));
    }
}

/// Each forge repository once (the first clone of it met).
fn unique_hostings(found: Vec<Hosted>) -> Vec<Hosted> {
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|(_, h)| seen.insert((h.forge.key(), h.host.clone(), h.path.clone())))
        .collect()
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

/// "GitHub · owner/repo", for an error.
fn where_is(hosting: &Hosting) -> String {
    format!("{} · {}", hosting.forge.name(), hosting.path)
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
        self.inbox.poll(&ctx);
        if self
            .inbox
            .asked_at
            .is_none_or(|at| at.elapsed() > ASK_EVERY)
        {
            let repos = self.inbox_repos(false);
            self.inbox.ask(repos, &ctx);
        }
        let mut refresh = false;
        let mut open = None;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PULL REQUESTS FOR YOU")
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                if self.inbox.busy() {
                    ui.add(crate::spinner::Spinner::new().size(11.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = "Ask again (connects to other machines)";
                    refresh = icons::small_button(ui, "refresh-cw", tip).clicked();
                });
            });
            let list = waiting(&self.inbox.items, &self.inbox_seen);
            if list.is_empty() && !self.inbox.busy() && self.inbox.asked_at.is_some() {
                ui.label(
                    RichText::new(
                        "Nothing waits for you: no review asked of you, and yours have no \
                         failing checks or comments you haven't seen.",
                    )
                    .color(t.text_faint)
                    .small(),
                );
            }
            ui.spacing_mut().item_spacing.y = 2.0;
            for (item, reasons) in list {
                if inbox_row(ui, item, &reasons, &t).clicked() {
                    open = Some(item.clone());
                }
            }
            if !self.inbox.errors.is_empty() {
                errors_line(ui, &self.inbox.errors, &t);
            }
        });
        if refresh {
            let repos = self.inbox_repos(true);
            self.inbox.ask(repos, &ctx);
        }
        if let Some(item) = open {
            self.open_from_inbox(item, &ctx);
        }
    }

    /// Opens `item` in its repository's tab (once the tab knows its forge), or in the browser
    /// for a forge gitcraft shows no pull requests of; what it said so far is seen.
    fn open_from_inbox(&mut self, item: Item, ctx: &egui::Context) {
        let seen = forge_inbox::seen_now(&item.pr, git::now());
        self.inbox_seen.insert(item.pr.url.clone(), seen);
        if !item.forge.has_pages() {
            ctx.open_url(egui::OpenUrl::new_tab(&item.pr.url));
            return;
        }
        self.open_repo(&item.repo, ctx);
        self.inbox.opening = Some((item.repo, item.pr.number, item.pr.url, Instant::now()));
    }

    /// Opens the pull request asked from the inbox once its tab is there and knows its forge;
    /// in the browser if that doesn't come.
    pub(super) fn poll_inbox_opening(&mut self, ctx: &egui::Context) {
        let Some((repo, number, url, since)) = &self.inbox.opening else {
            return;
        };
        let tab = self.tabs.iter_mut().find_map(|t| match t {
            super::Tab::Repo(r) if &r.path == repo => Some(r),
            _ => None,
        });
        match tab {
            Some(tab) if tab.shows_pull_requests() => {
                tab.queue(crate::repo::Action::OpenPr(*number));
                self.inbox.opening = None;
                ctx.request_repaint();
            }
            _ if since.elapsed() > OPEN_WAIT => {
                ctx.open_url(egui::OpenUrl::new_tab(url));
                self.inbox.opening = None;
            }
            _ => ctx.request_repaint_after(Duration::from_millis(250)),
        }
    }
}

/// One pull request: its number and title, its repository and author, and why it's here.
fn inbox_row(ui: &mut Ui, item: &Item, reasons: &[Reason], t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
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
    // The reasons on the right, as far as they go; the title keeps what is left.
    let font = egui::FontId::proportional(11.0);
    let mut x = rect.right() - 8.0;
    for reason in reasons.iter().rev() {
        let color = match reason {
            Reason::ReviewRequested => t.accent,
            Reason::ChecksFailing => t.danger,
            Reason::NewComments(_) => t.warning,
        };
        let galley = ui
            .painter()
            .layout_no_wrap(reason.label(), font.clone(), color);
        let w = galley.size().x;
        if x - w < rect.left() + rect.width() * 0.5 {
            break;
        }
        x -= w;
        ui.painter()
            .galley(pos2(x, y - galley.size().y / 2.0), galley, color);
        x -= 12.0;
    }
    let p = ui.painter_at(rect.with_max_x(x - 4.0));
    let draft = if item.pr.draft { "Draft · " } else { "" };
    p.text(
        pos2(rect.left() + 40.0, y - 8.0),
        Align2::LEFT_CENTER,
        format!("#{} {draft}{}", item.pr.number, item.pr.title),
        theme::medium(12.5),
        t.text,
    );
    p.text(
        pos2(rect.left() + 40.0, y + 8.0),
        Align2::LEFT_CENTER,
        format!(
            "{} · {} · {}",
            super::workspaces::repo_name(&item.repo),
            item.forge.name(),
            item.pr.author
        ),
        egui::FontId::proportional(11.0),
        t.text_faint,
    );
    let how = if item.forge.has_pages() {
        "Click to open it in its repository's tab"
    } else {
        "Click to open it in the browser"
    };
    resp.on_hover_text(format!("{}\n{how}", item.pr.url))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "Couldn't ask 2 forges", with each one's reason on hover.
fn errors_line(ui: &mut Ui, errors: &[(String, String)], t: &Tokens) {
    let text = match errors.len() {
        1 => "Couldn't ask 1 forge".to_owned(),
        n => format!("Couldn't ask {n} forges"),
    };
    let detail: Vec<String> = errors
        .iter()
        .map(|(at, why)| format!("{at}: {why}"))
        .collect();
    ui.label(RichText::new(text).color(t.text_faint).small())
        .on_hover_text(detail.join("\n\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosting(forge: Forge, path: &str) -> Hosting {
        Hosting {
            forge,
            host: "github.com".into(),
            path: path.into(),
            web: "https://github.com".into(),
        }
    }

    fn item(url: &str, updated: i64, mine: bool, requested: bool) -> Item {
        Item {
            repo: PathBuf::from("/r"),
            forge: Forge::GitHub,
            pr: InboxPr {
                url: url.into(),
                updated,
                mine,
                review_requested: requested,
                activity: forge_inbox::Activity::At(vec![100]),
                ..InboxPr::default()
            },
        }
    }

    #[test]
    fn each_forge_repository_is_asked_once() {
        let found = vec![
            (PathBuf::from("/a"), hosting(Forge::GitHub, "o/r")),
            (PathBuf::from("/a-clone"), hosting(Forge::GitHub, "o/r")),
            (PathBuf::from("/b"), hosting(Forge::GitHub, "o/other")),
        ];
        let todo = unique_hostings(found);
        let repos: Vec<_> = todo.iter().map(|(r, _)| r.clone()).collect();
        assert_eq!(repos, [PathBuf::from("/a"), PathBuf::from("/b")]);
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
        seen.insert("u2".to_owned(), Seen { at: 100, count: 1 });
        let list = waiting(&items, &seen);
        assert_eq!(list.len(), 1);
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
        inbox.ask(vec![dir.clone(), dir.join("missing")], &ctx);
        assert!(inbox.busy());
        // A second round isn't started over a running one.
        inbox.ask(vec![dir.clone()], &ctx);
        let until = Instant::now() + Duration::from_secs(30);
        while inbox.busy() {
            if Instant::now() > until {
                return Err("the round never ended".to_owned());
            }
            inbox.poll(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(inbox.items.is_empty() && inbox.errors.is_empty());
        Ok(())
    }

    #[test]
    fn errors_say_where() {
        assert_eq!(
            where_is(&hosting(Forge::GitLab, "g/sub/p")),
            "GitLab · g/sub/p"
        );
    }
}
