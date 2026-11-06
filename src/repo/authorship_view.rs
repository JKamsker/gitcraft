//! The AI authorship lens in the tab: what the commits record of their authorship, read in the
//! background after each read of the repository (when its refs or notes moved); the robot on
//! graph rows, the _Agent-written_ filter of the search bar, the stripes of blame and diffs, and
//! a branch's figures.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, Painter, Rect, Ui, pos2, vec2};

use super::{Action, RepoTab};
use crate::authorship::{self, Attribution, Who};
use crate::git::Commit;
use crate::theme::Tokens;

/// What the tab knows of who wrote its commits.
#[derive(Default)]
pub(crate) struct Lens {
    /// By commit hash; commits that record nothing aren't in it.
    pub(crate) map: Arc<HashMap<String, Attribution>>,
    /// The refs and notes it was read for: read again only when they move.
    read_for: Option<String>,
    loading: bool,
    again: bool,
    /// The search bar shows only agent-written commits.
    pub(crate) filter: bool,
}

/// What the attribution of a commit not in the map is, from its own message as the graph has
/// it (the trailers of commits past those read).
fn from_message(commit: &Commit) -> Option<Attribution> {
    // The graph keeps the message on one line: each `Co-authored-by:` taken up to its `>`.
    let lower = commit.body.to_lowercase();
    let mut message = String::new();
    for (at, _) in lower.match_indices("co-authored-by:") {
        let rest = &commit.body[at..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        message.push_str(&rest[..end]);
        message.push('\n');
    }
    authorship::attribute(&commit.author, &commit.email, &message, None, None)
}

impl Lens {
    /// What is known of `commit`.
    pub(crate) fn of(&self, commit: &Commit) -> Option<Attribution> {
        self.map
            .get(&commit.hash)
            .cloned()
            .or_else(|| from_message(commit))
    }
}

/// The colour of a stripe: agent, mixed, person; none when not known.
pub(crate) fn color(t: &Tokens, who: Who) -> Option<Color32> {
    match who {
        Who::Agent => Some(t.accent),
        Who::Mixed => Some(t.warning),
        Who::Human => Some(t.success),
        Who::Unknown => None,
    }
}

/// A 3 px stripe at the left of `rect` in the colour of `who`.
pub(crate) fn stripe(painter: &Painter, t: &Tokens, rect: Rect, who: Who) {
    if let Some(c) = color(t, who) {
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
            0.0,
            c,
        );
    }
}

/// The robot of a graph row at `left` (centred on `mid`), its hover saying what is known;
/// returns how much room it took.
pub(crate) fn robot(ui: &Ui, left: f32, mid: f32, a: &Attribution, id: egui::Id) -> f32 {
    if !matches!(a.who, Who::Agent | Who::Mixed) {
        return 0.0;
    }
    let t = Tokens::get(ui.ctx());
    let rect = Rect::from_center_size(pos2(left + 8.0, mid), vec2(16.0, 16.0));
    let color = if a.who == Who::Agent {
        t.accent
    } else {
        t.warning
    };
    crate::icons::paint(ui, rect, "bot", 13.0, color);
    ui.interact(rect, id, egui::Sense::hover())
        .on_hover_text(a.describe());
    18.0
}

impl RepoTab {
    /// Reads what the commits record, in the background, when the refs or notes moved since.
    pub(crate) fn load_authorship(&mut self, ctx: &egui::Context) {
        if self.authorship.loading {
            self.authorship.again = true;
            return;
        }
        self.authorship.loading = true;
        let read_for = self.authorship.read_for.clone();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                // Every ref and note, as a fingerprint: unchanged, nothing to read again.
                let refs = crate::git::run(
                    Some(&repo),
                    &["for-each-ref", "--format=%(objectname) %(refname)"],
                )?;
                if read_for.as_deref() == Some(refs.as_str()) {
                    return Ok(None);
                }
                authorship::load::all(&repo).map(|map| Some((refs, map)))
            },
            |tab, result, ctx| {
                tab.authorship.loading = false;
                match result {
                    Ok(Some((refs, map))) => {
                        tab.authorship.read_for = Some(refs);
                        tab.authorship.map = Arc::new(map);
                        if tab.authorship.filter {
                            tab.filter_agent_commits(ctx);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tab.status = Some((e, true)),
                }
                if std::mem::take(&mut tab.authorship.again) {
                    tab.load_authorship(ctx);
                }
            },
        );
    }

    /// The search bar's results become the agent-written commits of the history loaded.
    pub(crate) fn filter_agent_commits(&mut self, ctx: &egui::Context) {
        let found: Vec<String> = self
            .commits
            .iter()
            .filter(|c| {
                self.authorship
                    .of(c)
                    .is_some_and(|a| matches!(a.who, Who::Agent | Who::Mixed))
            })
            .map(|c| c.hash.clone())
            .collect();
        self.search.query.clear();
        self.search.edited = None;
        self.search.running = false;
        self.search.searched = AGENT_FILTER.to_owned();
        self.search.matches = found.iter().cloned().collect();
        self.search.results = found;
        self.search.current = 0;
        if let Some(first) = self.search.results.first().cloned() {
            self.goto(&first, ctx);
        }
    }

    /// Turns the _Agent-written_ filter on or off.
    pub(crate) fn toggle_agent_filter(&mut self, on: bool, ctx: &egui::Context) {
        self.authorship.filter = on;
        if on {
            self.filter_agent_commits(ctx);
        } else if self.search.searched == AGENT_FILTER {
            self.search.searched.clear();
            self.search.results.clear();
            self.search.matches.clear();
        }
    }

    /// Works out a branch's figures in the background and says them.
    pub(crate) fn branch_authorship(&mut self, branch: &str, ctx: &egui::Context) {
        let full = super::branch_visibility::local_ref(branch);
        // The main worktree's commit, unless that is the branch itself: then the default one.
        let main = self
            .worktrees
            .iter()
            .find(|w| w.main && w.branch.as_deref() != Some(branch))
            .map(|w| w.head.clone())
            .filter(|h| !h.is_empty());
        let known = self.authorship.map.clone();
        let repo = self.path.clone();
        let name = branch.to_owned();
        self.status = Some((format!("Working out who wrote {branch}…"), false));
        self.then(
            ctx,
            move || {
                let base = main
                    .or_else(|| default_branch(&repo, &full))
                    .ok_or_else(|| format!("There is no main branch to set {name} against"))?;
                authorship::branch::figures(&repo, &full, &base, &known).map(|f| f.headline(&name))
            },
            |tab, said, _| {
                tab.status = Some(match said {
                    Ok(line) => (line, false),
                    Err(e) => (e, true),
                });
            },
        );
    }
}

/// The remote's default branch, or `main` / `master`, other than `branch` itself.
fn default_branch(repo: &std::path::Path, branch: &str) -> Option<String> {
    [
        "refs/remotes/origin/HEAD",
        "refs/heads/main",
        "refs/heads/master",
    ]
    .iter()
    .filter(|r| **r != branch)
    .find(|r| crate::git::run(Some(repo), &["rev-parse", "--verify", "--quiet", r]).is_ok())
    .map(|r| (*r).to_owned())
}

/// What the search bar shows it searched when the filter is on.
pub(crate) const AGENT_FILTER: &str = "agent-written commits";

/// The filter's toggle in the search bar.
pub(crate) fn filter_toggle(ui: &mut Ui, on: bool, acts: &mut Vec<Action>) {
    if ui
        .selectable_label(on, "Agent-written")
        .on_hover_text(
            "Only the commits an agent wrote or co-wrote, as they record it (git-ai notes, \
             Agent Trace, an agent's account or Co-authored-by trailer)",
        )
        .clicked()
    {
        acts.push(Action::AgentFilter(!on));
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::authorship::Who;
    use crate::git;
    use crate::repo::{Action, DiffKey, RepoTab};

    fn until(
        tab: &mut RepoTab,
        ctx: &egui::Context,
        done: impl Fn(&RepoTab) -> bool,
    ) -> Result<(), String> {
        let started = Instant::now();
        while !done(tab) {
            tab.poll(ctx);
            if started.elapsed() > Duration::from_secs(30) {
                return Err("timed out".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    #[test]
    fn the_lens_through_the_tab() -> Result<(), String> {
        let dir = crate::authorship::tests::repo("tab")?;
        let run = |args: &[&str]| git::run(Some(&dir), args);
        run(&["checkout", "-q", "-b", "agent/x"])?;
        let agent = crate::authorship::tests::commit(&dir, "a.rs", "one\ntwo\nthree\n", "work")?;
        let note = "a.rs\n  0123456789abcdef 2-3\n---\n{\"schema_version\":\"authorship/3.0.0\",\"base_commit_sha\":\"x\",\"prompts\":{\"0123456789abcdef\":{\"agent_id\":{\"tool\":\"claude\"}}}}";
        run(&["notes", "--ref=ai", "add", "-m", note, &agent])?;
        let trailer = crate::authorship::tests::commit(
            &dir,
            "b.rs",
            "b\n",
            "b\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
        )?;
        let plain = crate::authorship::tests::commit(&dir, "c.rs", "c\n", "c")?;

        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.commits = git::log(&dir, None, 0, 100)?
            .into_iter()
            .map(std::sync::Arc::new)
            .collect();
        tab.load_authorship(&ctx);
        until(&mut tab, &ctx, |t| !t.authorship.loading)?;
        let of = |hash: &str| {
            tab.commits
                .iter()
                .find(|c| c.hash == hash)
                .and_then(|c| tab.authorship.of(c))
                .map(|a| a.who)
        };
        assert_eq!(of(&agent), Some(Who::Agent));
        assert_eq!(of(&trailer), Some(Who::Mixed));
        assert_eq!(of(&plain), None);
        // Nothing moved: read again, nothing to do.
        tab.load_authorship(&ctx);
        until(&mut tab, &ctx, |t| !t.authorship.loading)?;
        assert_eq!(tab.authorship.map.len(), 2);

        // The filter: the agent-written commits as the search's results.
        tab.apply(Action::AgentFilter(true), &ctx);
        let mut found = tab.search.results.clone();
        found.sort();
        let mut want = vec![agent.clone(), trailer.clone()];
        want.sort();
        assert_eq!(found, want);
        tab.apply(Action::AgentFilter(false), &ctx);
        assert!(tab.search.results.is_empty());

        // The diff's stripe: the two lines added are the agent's (line 1 was there).
        let key = DiffKey::Commit {
            hash: agent.clone(),
            path: "a.rs".into(),
        };
        let data = crate::repo::diff::load(&dir, &key, Default::default(), true)?;
        let added: Vec<Option<Who>> = data
            .lines
            .iter()
            .zip(&data.authors)
            .filter(|(l, _)| l.kind == crate::repo::diff::Kind::Add)
            .map(|(_, w)| *w)
            .collect();
        assert_eq!(added, vec![Some(Who::Agent), Some(Who::Agent)]);
        // Blame knows each line's number in its commit.
        let blame = git::blame(&dir, None, "a.rs")?;
        assert_eq!(
            blame.iter().map(|l| l.orig).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(blame[1].file, "a.rs");

        // The branch's figures, said in the status line.
        tab.apply(Action::BranchAuthorship("agent/x".into()), &ctx);
        until(&mut tab, &ctx, |t| {
            t.status.as_ref().is_some_and(|(s, _)| !s.ends_with('…'))
        })?;
        let said = tab
            .status
            .as_ref()
            .map(|(s, _)| s.clone())
            .unwrap_or_default();
        assert!(said.starts_with("agent/x: "), "{said}");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
