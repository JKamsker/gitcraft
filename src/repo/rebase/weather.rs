//! Replays trees only: no index, checkout, hooks, ref movement or user's merge drivers.
//! Git's temporary objects are unreachable and cleaned up by its normal garbage collection.

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

#[derive(Clone)]
struct Applied {
    tree: String,
    forecast: Forecast,
}

fn replay(
    repo: &Path,
    onto: &str,
    plan: &[(String, Step)],
    cache: &mut Cache,
    mergiraf: bool,
    requested: impl Fn() -> bool,
) -> Vec<Forecast> {
    let began = Instant::now();
    replay_with_budget(repo, onto, plan, cache, mergiraf, requested, || {
        began.elapsed() < Duration::from_secs(90)
    })
}

fn replay_with_budget(
    repo: &Path,
    onto: &str,
    plan: &[(String, Step)],
    cache: &mut Cache,
    mergiraf: bool,
    requested: impl Fn() -> bool,
    within_budget: impl Fn() -> bool,
) -> Vec<Forecast> {
    if !requested() {
        return Vec::new();
    }
    let active = || requested() && within_budget();
    let setup = || -> Result<(String, ForecastGit), String> {
        let context = ForecastGit::new(repo, &active)?;
        let tree = if onto.is_empty() {
            context.run(&["mktree"], &[0], &active)?
        } else {
            context.run(
                &[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &format!("{onto}^{{tree}}"),
                ],
                &[0],
                &active,
            )?
        };
        Ok((tree.trim().to_owned(), context))
    };
    let (tree, context) = match setup() {
        Ok(value) => value,
        Err(e) => {
            if !requested() {
                return Vec::new();
            }
            if !within_budget() {
                return finish_budget(plan, Vec::new(), &requested, &within_budget);
            }
            return unfinished(plan, &e);
        }
    };
    replay_rows(
        plan,
        cache,
        tree,
        &requested,
        &within_budget,
        |tree, commit, active| apply(&context, tree, commit, mergiraf, active),
    )
}

/// Keep completed rows, but never publish a short current-generation answer when the
/// overall budget runs out. Cancellation belongs to a stale editor/generation instead.
fn finish_budget(
    plan: &[(String, Step)],
    mut rows: Vec<Forecast>,
    requested: &dyn Fn() -> bool,
    within_budget: &dyn Fn() -> bool,
) -> Vec<Forecast> {
    if requested() && !within_budget() {
        rows.extend(unfinished(
            &plan[rows.len()..],
            crate::i18n::text("repo-core-3255"),
        ));
    }
    rows
}

fn unfinished(plan: &[(String, Step)], reason: &str) -> Vec<Forecast> {
    plan.iter()
        .map(|(_, step)| {
            if *step == Step::Drop {
                Forecast::Dropped
            } else {
                Forecast::Unknown(reason.to_owned())
            }
        })
        .collect()
}

fn replay_rows(
    plan: &[(String, Step)],
    cache: &mut Cache,
    mut tree: String,
    requested: &dyn Fn() -> bool,
    within_budget: &dyn Fn() -> bool,
    mut apply_step: impl FnMut(&str, &str, &dyn Fn() -> bool) -> Result<Applied, String>,
) -> Vec<Forecast> {
    let active = || requested() && within_budget();
    let mut rows = Vec::with_capacity(plan.len());
    let mut blocked: Option<String> = None;
    for (commit, step) in plan {
        if !active() {
            break;
        }
        if *step == Step::Drop {
            rows.push(Forecast::Dropped);
            continue;
        }
        if let Some(reason) = &blocked {
            rows.push(Forecast::Unknown(reason.clone()));
            continue;
        }
        let key = (tree.clone(), commit.clone());
        let applied = match cache.get(&key) {
            Some(value) => Ok(value.clone()),
            None => apply_step(&tree, commit, &active).inspect(|value| {
                // Bound a long editing session without affecting correctness.
                if cache.len() >= 2048 {
                    cache.clear();
                }
                cache.insert(key, value.clone());
            }),
        };
        if !active() {
            break;
        }
        match applied {
            Ok(applied) => {
                if matches!(
                    applied.forecast,
                    Forecast::Conflict(_) | Forecast::Unknown(_)
                ) {
                    blocked = Some(crate::i18n::text("repo-core-3256").to_owned());
                }
                tree = applied.tree;
                rows.push(applied.forecast);
            }
            Err(e) => {
                let reason =
                    crate::i18n::message("repo-core-3257", &[("e", format!("{}", e).into())]);
                blocked = Some(reason.clone());
                rows.push(Forecast::Unknown(reason));
            }
        }
    }
    finish_budget(plan, rows, requested, within_budget)
}

fn apply(
    context: &ForecastGit,
    tree: &str,
    commit: &str,
    mergiraf: bool,
    active: &dyn Fn() -> bool,
) -> Result<Applied, String> {
    // Resolve before commands without --end-of-options; only object IDs follow.
    let commit = context.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{commit}}"),
        ],
        &[0],
        active,
    )?;
    let commit = commit.trim();
    let ancestry = context.run(
        &["rev-list", "--parents", "-n", "1", commit, "--"],
        &[0],
        active,
    )?;
    let parent = match ancestry.split_whitespace().nth(1) {
        Some(parent) => parent.to_owned(),
        None => {
            let empty = context.run(&["mktree"], &[0], active)?;
            context.run(
                &[
                    "-c",
                    "user.name=gitcraft forecast",
                    "-c",
                    "user.email=forecast@gitcraft.invalid",
                    "commit-tree",
                    "--no-gpg-sign",
                    empty.trim(),
                    "-m",
                    "empty forecast base",
                ],
                &[0],
                active,
            )?
        }
    };
    let parent = parent.trim();
    let temporary = context.run(
        &[
            "-c",
            "user.name=gitcraft forecast",
            "-c",
            "user.email=forecast@gitcraft.invalid",
            "commit-tree",
            "--no-gpg-sign",
            tree,
            "-m",
            "gitcraft rebase forecast",
        ],
        &[0],
        active,
    )?;
    // Global option: unsupported Git fails explicitly rather than silently ignoring an env
    // variable. All attribute lookups use the simulated parent, even in a dirty checkout.
    let attributes = format!("--attr-source={tree}");
    let base = format!("--merge-base={parent}");
    let output = context.run(
        &[
            "-c",
            "merge.default=text",
            "-c",
            "merge.conflictStyle=diff3",
            "-c",
            "merge.renormalize=false",
            &attributes,
            "merge-tree",
            "--write-tree",
            "--name-only",
            "-z",
            &base,
            temporary.trim(),
            commit,
        ],
        &[0, 1],
        active,
    )?;
    let (result_tree, files) = parse_merge(&output)?;
    let forecast = if files.is_empty() {
        if result_tree == tree
            && !context
                .run(
                    &[
                        "diff-tree",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--name-only",
                        "-r",
                        parent,
                        commit,
                    ],
                    &[0],
                    active,
                )?
                .is_empty()
        {
            Forecast::Empty
        } else {
            Forecast::Clean
        }
    } else {
        let mut conflicts = Vec::new();
        for path in files {
            let checked = if !mergiraf {
                Err(crate::i18n::text("repo-core-3273").to_owned())
            } else {
                context
                    .bytes(
                        &["show", "--end-of-options", &format!("{result_tree}:{path}")],
                        &[0],
                        active,
                    )
                    .and_then(|bytes| {
                        String::from_utf8(bytes)
                            .map_err(|_| crate::i18n::text("repo-core-3275").to_owned())
                    })
                    .and_then(|text| {
                        if !text.lines().any(|line| line.starts_with("<<<<<<< ")) {
                            return Err(crate::i18n::text("repo-core-3276").to_owned());
                        }
                        crate::mergiraf::solve_text(&path, &text, active)
                    })
            };
            if !active() {
                return Err(crate::i18n::text("repo-core-3277").into());
            }
            let result = match checked {
                Ok(text) if !text.lines().any(|line| line.starts_with("<<<<<<< ")) => {
                    crate::i18n::text("repo-core-3278").to_owned()
                }
                Ok(_) => crate::i18n::text("repo-core-3279").to_owned(),
                Err(e) => crate::i18n::message("repo-core-3280", &[("e", format!("{}", e).into())]),
            };
            conflicts.push((path, result));
        }
        Forecast::Conflict(conflicts)
    };
    Ok(Applied {
        tree: result_tree,
        forecast,
    })
}

fn parse_merge(output: &str) -> Result<(String, Vec<String>), String> {
    let mut fields = output.split('\0');
    let tree = fields.next().unwrap_or_default();
    if !matches!(tree.len(), 40 | 64) || !tree.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(crate::i18n::text("repo-core-3281").to_owned());
    }
    let mut files = Vec::new();
    for path in fields.take_while(|path| !path.is_empty()) {
        if !files.iter().any(|p| p == path) {
            files.push(path.to_owned());
        }
    }
    Ok((tree.to_owned(), files))
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
                        format!(
                            "{}",
                            files
                                .iter()
                                .map(|(path, result)| format!("{path}\n  {result}"))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
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
mod tests {
    use super::*;
    use crate::git;
    use std::cell::Cell;
    use std::path::PathBuf;

    fn budget_plan() -> Plan {
        vec![
            ("first".into(), Step::Pick),
            ("second".into(), Step::Reword),
            ("dropped".into(), Step::Drop),
            ("last".into(), Step::Fixup),
        ]
    }

    fn assert_budget_exhausted(rows: &[Forecast], plan: &Plan, from: usize) {
        assert_eq!(rows.len(), plan.len());
        for (row, (_, step)) in rows[from..].iter().zip(&plan[from..]) {
            if *step == Step::Drop {
                assert_eq!(*row, Forecast::Dropped);
            } else {
                assert!(
                    matches!(row, Forecast::Unknown(reason) if reason.contains("time budget exceeded"))
                );
            }
        }
    }

    #[test]
    fn budget_expiring_between_rows_publishes_a_complete_terminal_answer() {
        let plan = budget_plan();
        let applied = Cell::new(false);
        let checked_after_apply = Cell::new(false);
        let budget = || !applied.get() || !checked_after_apply.replace(true);
        let rows = replay_rows(
            &plan,
            &mut Cache::new(),
            "base".into(),
            &|| true,
            &budget,
            |_, _, _| {
                applied.set(true);
                Ok(Applied {
                    tree: "next".into(),
                    forecast: Forecast::Clean,
                })
            },
        );
        assert_eq!(rows[0], Forecast::Clean);
        assert_budget_exhausted(&rows, &plan, 1);

        let mut weather = Weather {
            plan,
            running: true,
            ..Default::default()
        };
        weather.arrived(0, rows, Cache::new());
        assert!(!weather.running);
        assert!(weather.edited.is_none());
        // Every UI row lookup now finds a terminal status rather than badge(None)'s Checking.
        for i in 0..weather.plan.len() {
            assert!(weather.rows.get(i).is_some());
        }
    }

    #[test]
    fn budget_expiring_inside_a_step_finalizes_that_position_and_every_later_row() {
        let plan = budget_plan();
        let budget = Cell::new(true);
        let rows = replay_rows(
            &plan,
            &mut Cache::new(),
            "base".into(),
            &|| true,
            &|| budget.get(),
            |_, _, active| {
                assert!(active());
                budget.set(false);
                assert!(!active());
                Err("Forecast cancelled or timed out".into())
            },
        );
        assert_budget_exhausted(&rows, &plan, 0);
    }

    #[test]
    fn expired_setup_budget_is_terminal_but_generation_cancellation_can_abandon_rows() {
        let plan = budget_plan();
        let rows = replay_with_budget(
            Path::new("/nonexistent/budget-test"),
            "base",
            &plan,
            &mut Cache::new(),
            false,
            || true,
            || false,
        );
        assert_budget_exhausted(&rows, &plan, 0);

        let requested = Cell::new(true);
        let rows = replay_rows(
            &plan,
            &mut Cache::new(),
            "base".into(),
            &|| requested.get(),
            &|| true,
            |_, _, _| {
                requested.set(false);
                Err("Forecast cancelled".into())
            },
        );
        assert!(rows.is_empty());
    }

    struct Repository(PathBuf);

    impl Repository {
        fn new(name: &str) -> Result<Self, String> {
            let parent = std::env::temp_dir();
            let dir = super::super::private_dir(&parent).map_err(|e| e.to_string())?;
            let repo = Self(dir);
            for args in [
                &["init", "-q"][..],
                &["config", "user.name", "weather test"],
                &["config", "user.email", "weather@test.invalid"],
                &["config", "commit.gpgSign", "false"],
                &["config", "core.autocrlf", "false"],
            ] {
                repo.git(args)?;
            }
            repo.commit("file.txt", "base\n", name)?;
            Ok(repo)
        }

        fn git(&self, args: &[&str]) -> Result<String, String> {
            git::run(Some(&self.0), args).map(|s| s.trim_end().to_owned())
        }

        fn commit(&self, path: &str, text: &str, message: &str) -> Result<String, String> {
            std::fs::write(self.0.join(path), text).map_err(|e| e.to_string())?;
            self.git(&["add", "--", path])?;
            self.git(&["commit", "-qm", message])?;
            self.git(&["rev-parse", "HEAD"])
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn replay_accumulates_trees_and_reports_the_exact_conflict_position() -> Result<(), String> {
        let repo = Repository::new("base")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("file.txt", "one\n", "one")?;
        let b = repo.commit("file.txt", "two\n", "two")?;
        let mut cache = Cache::new();
        let original = vec![(a.clone(), Step::Pick), (b.clone(), Step::Pick)];
        let clean = replay(&repo.0, &base, &original, &mut cache, true, || true);
        assert_eq!(clean, [Forecast::Clean, Forecast::Clean]);
        let base_tree = repo.git(&["rev-parse", &format!("{base}^{{tree}}")])?;
        let first = cache
            .get(&(base_tree, a.clone()))
            .ok_or("first result not cached")?;
        let last = cache
            .get(&(first.tree.clone(), b.clone()))
            .ok_or("second result not cached")?;
        assert_eq!(last.tree, repo.git(&["rev-parse", "HEAD^{tree}"])?);
        let count = cache.len();
        assert_eq!(
            replay(&repo.0, &base, &original, &mut cache, true, || true),
            clean
        );
        assert_eq!(cache.len(), count);
        let reordered = vec![(b.clone(), Step::Pick), (a.clone(), Step::Reword)];
        let forecast = replay(&repo.0, &base, &reordered, &mut cache, true, || true);
        assert!(
            matches!(&forecast[0], Forecast::Conflict(files) if files.len() == 1 && files[0].0 == "file.txt")
        );
        assert!(matches!(&forecast[1], Forecast::Unknown(reason) if reason.contains("earlier")));
        let dropped = vec![(a, Step::Drop), (b, Step::Pick)];
        let forecast = replay(&repo.0, &base, &dropped, &mut cache, true, || true);
        assert_eq!(forecast[0], Forecast::Dropped);
        assert!(matches!(forecast[1], Forecast::Conflict(_)));
        Ok(())
    }

    #[test]
    fn attributes_come_from_each_simulated_parent_and_ignore_checkout_and_info()
    -> Result<(), String> {
        let repo = Repository::new("attributes")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("file.txt", "one\n", "one")?;
        std::fs::write(repo.0.join(".gitattributes"), "*.txt merge=union\n")
            .map_err(|e| e.to_string())?;
        repo.git(&["add", ".gitattributes"])?;
        let b = repo.commit("file.txt", "two\n", "two and union")?;
        let plan = [(b.clone(), Step::Pick), (a, Step::Pick)];
        let mut cache = Cache::new();
        let first = replay(&repo.0, &base, &plan, &mut cache, false, || true);
        assert!(matches!(first[0], Forecast::Conflict(_)), "{first:?}");
        assert!(matches!(first[1], Forecast::Unknown(_)));

        // Same immutable input, different dirty checkout/global/info attributes. Neither
        // a cold computation nor a cache hit may acquire the checkout's union semantics.
        std::fs::write(repo.0.join(".gitattributes"), "*.txt -merge\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(repo.0.join(".git/info/attributes"), "*.txt merge=union\n")
            .map_err(|e| e.to_string())?;
        let global = repo.0.join("global-attributes");
        std::fs::write(&global, "*.txt merge=union\n").map_err(|e| e.to_string())?;
        repo.git(&["config", "core.attributesFile", &global.to_string_lossy()])?;
        assert_eq!(
            replay(&repo.0, &base, &plan, &mut cache, false, || true),
            first
        );
        assert_eq!(
            replay(&repo.0, &base, &plan, &mut Cache::new(), false, || true),
            first
        );
        assert_eq!(
            std::fs::read_to_string(repo.0.join(".gitattributes")).map_err(|e| e.to_string())?,
            "*.txt -merge\n"
        );

        // Introducing union earlier in the todo changes the simulated parent tree. That
        // subsequent merge really should use union, despite a dirty binary checkout.
        std::fs::write(repo.0.join(".gitattributes"), "*.txt merge=union\n")
            .map_err(|e| e.to_string())?;
        repo.git(&["add", ".gitattributes"])?;
        // This commit must add attributes relative to a parent with none.
        repo.git(&["checkout", "-q", "--detach", &base])?;
        let union = repo.commit(".gitattributes", "*.txt merge=union\n", "union")?;
        std::fs::write(repo.0.join(".gitattributes"), "*.txt -merge\n")
            .map_err(|e| e.to_string())?;
        assert_eq!(
            replay(
                &repo.0,
                &base,
                &[(union, Step::Pick), (b, Step::Pick)],
                &mut Cache::new(),
                false,
                || true
            ),
            [Forecast::Clean, Forecast::Clean]
        );
        Ok(())
    }

    #[test]
    fn root_rebase_replays_from_an_empty_tree() -> Result<(), String> {
        let repo = Repository::new("root")?;
        let root = repo.git(&["rev-parse", "HEAD"])?;
        assert_eq!(
            replay(
                &repo.0,
                "",
                &[(root, Step::Pick)],
                &mut Cache::new(),
                false,
                || true
            ),
            [Forecast::Clean]
        );
        Ok(())
    }

    #[test]
    fn independent_reorders_squashes_and_fixups_have_the_same_final_tree() -> Result<(), String> {
        let repo = Repository::new("independent")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("a.txt", "a\n", "a")?;
        let b = repo.commit("b.txt", "b\n", "b")?;
        let expected = repo.git(&["rev-parse", "HEAD^{tree}"])?;
        for step in [Step::Pick, Step::Reword, Step::Squash, Step::Fixup] {
            let mut cache = Cache::new();
            let plan = vec![(b.clone(), Step::Pick), (a.clone(), step)];
            assert_eq!(
                replay(&repo.0, &base, &plan, &mut cache, true, || true),
                [Forecast::Clean, Forecast::Clean]
            );
            assert!(cache.values().any(|v| v.tree == expected));
        }
        Ok(())
    }

    #[test]
    fn commits_that_become_empty_are_cloudy_and_later_commits_are_still_checked()
    -> Result<(), String> {
        let repo = Repository::new("empty")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("file.txt", "one\n", "one")?;
        let b = repo.commit("file.txt", "base\n", "back")?;
        let c = repo.commit("file.txt", "one\n", "one again")?;
        let d = repo.commit("other.txt", "independent\n", "other")?;
        let rows = replay(
            &repo.0,
            &base,
            &[
                (a, Step::Pick),
                (b, Step::Drop),
                (c, Step::Pick),
                (d, Step::Pick),
            ],
            &mut Cache::new(),
            true,
            || true,
        );
        assert_eq!(
            rows,
            [
                Forecast::Clean,
                Forecast::Dropped,
                Forecast::Empty,
                Forecast::Clean
            ]
        );
        Ok(())
    }

    #[test]
    fn forecasting_preserves_dirty_files_index_refs_reflogs_and_stashes() -> Result<(), String> {
        let repo = Repository::new("unchanged")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("file.txt", "one\n", "one")?;
        std::fs::write(repo.0.join("file.txt"), "stashed\n").map_err(|e| e.to_string())?;
        repo.git(&["stash", "push", "-qm", "kept"])?;
        std::fs::write(repo.0.join("file.txt"), "staged\n").map_err(|e| e.to_string())?;
        repo.git(&["add", "file.txt"])?;
        std::fs::write(repo.0.join("file.txt"), "working\n").map_err(|e| e.to_string())?;
        let snapshot = || -> Result<_, String> {
            Ok((
                repo.git(&["show-ref"])?,
                repo.git(&["reflog", "show", "--all"])?,
                repo.git(&["diff", "--cached", "--binary"])?,
                repo.git(&["diff", "--binary"])?,
                repo.git(&["stash", "list"])?,
            ))
        };
        let before = snapshot()?;
        let rows = replay(
            &repo.0,
            &base,
            &[(a, Step::Pick)],
            &mut Cache::new(),
            true,
            || true,
        );
        assert_eq!(rows, [Forecast::Clean]);
        assert_eq!(snapshot()?, before);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("file.txt")).map_err(|e| e.to_string())?,
            "working\n"
        );
        assert!(!repo.0.join(".git/rebase-merge").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn configured_external_merge_drivers_and_hooks_are_not_executed() -> Result<(), String> {
        let repo = Repository::new("no drivers")?;
        repo.commit(
            ".gitattributes",
            "*.txt merge=untrusted\n",
            "driver attribute",
        )?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let a = repo.commit("file.txt", "one\n", "one")?;
        let b = repo.commit("file.txt", "two\n", "two")?;
        let marker = repo.0.join("driver-ran");
        let hook = repo.0.join("hook-ran");
        repo.git(&[
            "config",
            "merge.untrusted.driver",
            &format!(
                "touch {}",
                crate::remote::sh_quote(&marker.to_string_lossy())
            ),
        ])?;
        use std::os::unix::fs::PermissionsExt;
        let path = repo.0.join(".git/hooks/pre-commit");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\ntouch {}\n",
                crate::remote::sh_quote(&hook.to_string_lossy())
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let rows = replay(
            &repo.0,
            &base,
            &[(b, Step::Pick), (a, Step::Pick)],
            &mut Cache::new(),
            true,
            || true,
        );
        assert!(matches!(rows[0], Forecast::Conflict(_)), "{rows:?}");
        assert!(!marker.exists());
        assert!(!hook.exists());
        Ok(())
    }

    #[test]
    fn nul_paths_errors_and_cancellation_are_not_mistaken_for_clean_forecasts() -> Result<(), String>
    {
        let tree = "a".repeat(40);
        assert_eq!(
            parse_merge(&format!("{tree}\0a\nb\0q\\x\0\0messages\0"))?,
            (tree, vec!["a\nb".to_owned(), "q\\x".to_owned()])
        );
        assert!(parse_merge("usage: merge-tree").is_err());
        let repo = Repository::new("errors")?;
        let base = repo.git(&["rev-parse", "HEAD"])?;
        let plan = vec![("missing".to_owned(), Step::Pick)];
        assert!(matches!(
            replay(&repo.0, &base, &plan, &mut Cache::new(), true, || true)[0],
            Forecast::Unknown(_)
        ));
        assert!(replay(&repo.0, &base, &plan, &mut Cache::new(), true, || false).is_empty());
        Ok(())
    }

    #[test]
    fn stale_answers_are_dropped_and_refresh_discards_an_inflight_cache() {
        let ctx = egui::Context::default();
        let mut weather = Weather::default();
        let entry = Entry {
            commit: git::RebaseCommit {
                hash: "a".into(),
                ..Default::default()
            },
            step: Step::Pick,
            message: String::new(),
        };
        weather.observe(std::slice::from_ref(&entry), &ctx);
        let generation = weather.generation.load(Ordering::Relaxed);
        weather.observe(
            &[Entry {
                step: Step::Drop,
                ..entry
            }],
            &ctx,
        );
        weather.running = true;
        weather.arrived(generation, vec![Forecast::Clean], Cache::new());
        assert!(weather.running);
        assert!(weather.rows.is_empty());
        weather.refresh(&ctx);
        let mut cache = Cache::new();
        cache.insert(
            ("tree".into(), "commit".into()),
            Applied {
                tree: "other".into(),
                forecast: Forecast::Clean,
            },
        );
        weather.arrived(generation, vec![Forecast::Clean], cache);
        assert!(weather.cache.is_empty());
    }

    #[test]
    fn closing_the_editor_cancels_weather_even_while_a_todo_snapshot_exists() {
        let editor = super::super::RebaseEditor::new("base".into(), "base".into(), Vec::new());
        let lifetime = Arc::downgrade(&editor.weather.generation);
        let snapshot = editor.clone();
        drop(editor);
        assert!(lifetime.upgrade().is_none());
        assert_eq!(snapshot.onto, "base");
    }
}
