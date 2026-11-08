//! Author-arranged, revision-pinned review cards; immutable originals remain available.
use super::{sharing, store};
use crate::{
    git,
    pull_request::{PrDetails, PrFiles},
    repo::{Action, RepoTab, file_view::FileTab},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize, Default)]
struct Board {
    cards: Vec<Card>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Card {
    phase: String,
    text: String,
    base: String,
    head: String,
    anchor: Anchor,
}
#[derive(Clone, Serialize, Deserialize)]
enum Anchor {
    Code {
        commit: String,
        path: String,
        line: u64,
        blob: String,
    },
    Test {
        commit: String,
        tree: String,
        command: String,
        verdict: String,
    },
}
#[derive(Clone)]
struct Shown {
    card: Card,
    target: Option<(String, String, u64)>,
    stale: Option<String>,
}
#[derive(Clone, Default)]
pub(crate) struct CardForm {
    phase: usize,
    text: String,
    path: String,
    commit: String,
    line: String,
    test: bool,
}
#[derive(Default)]
pub(crate) struct Storyboard {
    request: u64,
    rows: Option<Result<Vec<Shown>, String>>,
    form: CardForm,
    remote: String,
    imported: bool,
    cached: Option<Vec<Card>>,
}
impl Storyboard {
    pub(in crate::repo) fn invalidate(&mut self) {
        self.request += 1;
        self.rows = None;
    }
    fn accept(&mut self, request: u64, result: Result<Vec<Shown>, String>) {
        if request != self.request {
            return;
        }
        if let Ok(rows) = &result {
            self.cached = Some(rows.iter().map(|r| r.card.clone()).collect());
        }
        self.rows = Some(result);
    }
}
#[derive(Clone)]
pub(crate) enum StoryAct {
    Load,
    Fetch {
        remote: String,
        head: String,
    },
    Publish {
        remote: String,
        head: String,
    },
    OpenCode {
        commit: String,
        path: String,
        line: u64,
    },
    Add {
        form: CardForm,
        head: String,
    },
    Move {
        index: usize,
        up: bool,
        head: String,
    },
    Remove {
        index: usize,
        head: String,
    },
}
fn key(d: &PrDetails) -> String {
    format!("{}#{}", d.url, d.number)
}
fn oid(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map(|s| s.trim().into())
}
fn file_blob(repo: &Path, commit: &str, path: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}:{path}"),
        ],
    )
    .map(|s| s.trim().into())
}
fn card(repo: &Path, d: &PrDetails, f: &PrFiles, form: CardForm) -> Result<Card, String> {
    if form.text.trim().is_empty() {
        return Err("Give the card its explanation".into());
    }
    let commit = oid(
        repo,
        if form.commit.trim().is_empty() {
            &f.to
        } else {
            form.commit.trim()
        },
    )?;
    let anchor = if form.test {
        let tree = git::run(Some(repo), &["rev-parse", &format!("{commit}^{{tree}}")])?
            .trim()
            .to_owned();
        let verdict = git::read_results(repo, git::DEFAULT_TEST)?
            .get(&tree)
            .copied()
            .ok_or("No test-lane result for this tree; run Test commits first")?;
        let command =
            git::test_command(repo, git::DEFAULT_TEST)?.ok_or("No test command recorded")?;
        Anchor::Test {
            commit,
            tree,
            command: format!(
                "Configured when pinned (execution command not recorded by test lane): {command}"
            ),
            verdict: verdict.word().into(),
        }
    } else {
        let path = form.path.trim().to_owned();
        if path.is_empty() {
            return Err("Choose a repository-relative path".into());
        }
        let blob = file_blob(repo, &commit, &path)?;
        let line = if form.line.trim().is_empty() {
            0
        } else {
            form.line
                .parse()
                .map_err(|_| "Line must be a positive number")?
        };
        if line > 0 {
            let text = git::run(Some(repo), &["cat-file", "blob", &blob])?;
            if line > text.lines().count() as u64 {
                return Err("Line is outside this revision's file".into());
            }
        }
        Anchor::Code {
            commit,
            path,
            line,
            blob,
        }
    };
    Ok(Card {
        phase: ["Old failure", "Invariant", "Fix", "Result"]
            .get(form.phase)
            .unwrap_or(&"Fix")
            .to_string(),
        text: form.text,
        base: f.from.clone(),
        head: d.head_oid.clone(),
        anchor,
    })
}
fn show_card(repo: &Path, card: Card, f: &PrFiles) -> Result<Shown, String> {
    let mut shown = Shown {
        card: card.clone(),
        target: None,
        stale: None,
    };
    match &card.anchor {
        Anchor::Test { tree, .. } => {
            let now = git::run(Some(repo), &["rev-parse", &format!("{}^{{tree}}", f.to)])?;
            if now.trim() != tree {
                shown.stale = Some("Current tree differs from the pinned test result".into());
            }
        }
        Anchor::Code {
            commit,
            path,
            line,
            blob,
        } => {
            let mut target = commit.clone();
            if card.head != f.to {
                let pairs = git::range_diff(
                    repo,
                    &git::Series::new(&card.base, &card.head),
                    &git::Series::new(&f.from, &f.to),
                )?;
                if commit == &card.head {
                    target = f.to.clone();
                } else if commit == &card.base {
                    target = f.from.clone();
                } else if let Some(new) = pairs
                    .pairs
                    .iter()
                    .find(|p| p.old.as_ref().is_some_and(|c| &c.hash == commit))
                    .and_then(|p| p.new.as_ref())
                {
                    target = new.hash.clone();
                } else {
                    shown.stale =
                        Some("The pinned commit has no replacement in this series".into());
                }
            }
            let renames = git::diff_files(repo, commit, Some(&target))?;
            let path = renames
                .iter()
                .find(|f| f.old_path.as_deref() == Some(path))
                .map_or(path.as_str(), |f| f.path.as_str())
                .to_owned();
            match file_blob(repo, &target, &path) {
                Ok(now) if &now == blob => shown.target = Some((target, path, *line)),
                Ok(_) => {
                    shown.stale = Some(
                        "The card's file changed; open the pinned original to review it".into(),
                    )
                }
                Err(_) => {
                    shown.stale = Some("The card's file was removed or is unavailable".into())
                }
            }
        }
    }
    Ok(shown)
}
fn read(repo: &Path, d: &PrDetails, f: &PrFiles) -> Result<Vec<Shown>, String> {
    store::load::<Board>(repo, "storyboard", &key(d))?
        .unwrap_or_default()
        .cards
        .into_iter()
        .map(|c| show_card(repo, c, f))
        .collect()
}
fn preserve(repo: &Path, key: &str, board: &Board) -> Result<(), String> {
    // JSON references alone do not retain commit objects through gc.
    for (i, c) in board.cards.iter().enumerate() {
        for (role, rev) in [
            ("base", &c.base),
            ("head", &c.head),
            (
                "anchor",
                match &c.anchor {
                    Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
                },
            ),
        ] {
            git::run(
                Some(repo),
                &[
                    "update-ref",
                    &format!(
                        "refs/gitcraft/storyboard-anchors/{:x}/{i}/{role}",
                        md5::compute(key)
                    ),
                    rev,
                ],
            )?;
        }
    }
    store::save(repo, "storyboard", key, board)
}
fn shared_ref(key: &str) -> String {
    format!("refs/gitcraft/storyboards/{:x}", md5::compute(key))
}
fn export(repo: &Path, key: &str, board: &Board, previous: Option<&str>) -> Result<String, String> {
    if board.cards.len() > 256 {
        return Err("A shared storyboard supports at most 256 cards".into());
    }
    let json = serde_json::to_string(board).map_err(|e| e.to_string())?;
    let blob = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &json)?;
    let tree = git::run_input(
        Some(repo),
        &["mktree"],
        &format!("100644 blob {}\tstoryboard.json\n", blob.trim()),
    )?;
    let mut parents = std::collections::BTreeSet::new();
    if let Some(previous) = previous {
        parents.insert(oid(repo, previous)?);
    }
    for card in &board.cards {
        for rev in [
            &card.base,
            &card.head,
            match &card.anchor {
                Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
            },
        ] {
            parents.insert(oid(repo, rev)?);
        }
    }
    let mut args = vec![
        "-c".into(),
        "commit.gpgSign=false".into(),
        "commit-tree".into(),
        tree.trim().to_owned(),
        "-m".into(),
        format!("gitcraft review storyboard {key}"),
    ];
    for parent in parents {
        args.extend(["-p".into(), parent]);
    }
    git::run(
        Some(repo),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .map(|s| s.trim().into())
}
fn imported(repo: &Path, commit: &str) -> Result<Board, String> {
    let json = git::run(Some(repo), &["show", &format!("{commit}:storyboard.json")])?;
    if json.len() > 2 * 1024 * 1024 {
        return Err("Shared storyboard exceeds the 2MiB metadata budget".into());
    }
    let board: Board =
        serde_json::from_str(&json).map_err(|e| format!("Invalid shared storyboard: {e}"))?;
    if board.cards.len() > 256 {
        return Err("Shared storyboard has more than 256 cards".into());
    }
    // Verify pinned sources are present commits. Fetching the artifact retains them through
    // its parent graph, including source commits removed by a later force push.
    for card in &board.cards {
        for rev in [
            &card.base,
            &card.head,
            match &card.anchor {
                Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
            },
        ] {
            if oid(repo, rev)? != *rev {
                return Err("Shared storyboard revision is not an immutable commit ID".into());
            }
        }
    }
    for card in &board.cards {
        if let Anchor::Code {
            commit, path, blob, ..
        } = &card.anchor
            && file_blob(repo, commit, path)? != *blob
        {
            return Err("Shared storyboard blob does not match its pinned source".into());
        }
    }
    Ok(board)
}
fn cached_ref(key: &str, target: &sharing::Target) -> String {
    format!(
        "refs/gitcraft/storyboard-remotes/{}/{:x}",
        target.cache_key(),
        md5::compute(key)
    )
}
fn publish_board(
    repo: &Path,
    key: &str,
    board: &Board,
    target: &sharing::Target,
    creds: Option<&crate::git::Credentials>,
) -> Result<(), String> {
    target.guard(repo)?;
    let shared = shared_ref(key);
    let cached = cached_ref(key, target);
    // A remote can have different fetch/push URLs. The lease must describe the push server.
    let remote_tip = target.run(
        repo,
        &["ls-remote", "--refs", target.push_url(), &shared],
        creds,
    )?;
    let previous = remote_tip.split_whitespace().next().map(str::to_owned);
    if previous.is_some() {
        target.guard(repo)?;
        target.fetch(
            repo,
            true,
            &[format!("+{shared}:{cached}")],
            &cached,
            false,
            creds,
        )?;
        if git::run(Some(repo), &["rev-parse", &cached])?.trim()
            != previous.as_deref().unwrap_or("")
        {
            return Err(
                "Shared storyboard changed while preparing publication; fetch and retry".into(),
            );
        }
    }
    let commit = export(repo, key, board, previous.as_deref())?;
    target.guard(repo)?;
    target.run(
        repo,
        &[
            "push",
            &format!(
                "--force-with-lease={shared}:{}",
                previous.as_deref().unwrap_or("")
            ),
            "--",
            target.push_url(),
            &format!("{commit}:{shared}"),
        ],
        creds,
    )?;
    let old_cache = target
        .fetched(&cached)?
        .unwrap_or_else(|| "0".repeat(commit.len()));
    git::run(Some(repo), &["update-ref", &cached, &commit, &old_cache])?;
    Ok(())
}
fn fetch_board(
    repo: &Path,
    key: &str,
    target: &sharing::Target,
    creds: Option<&crate::git::Credentials>,
) -> Result<Board, String> {
    target.guard(repo)?;
    let cached = cached_ref(key, target);
    target.fetch(
        repo,
        false,
        &[format!("+{}:{cached}", shared_ref(key))],
        &cached,
        false,
        creds,
    )?;
    let commit = target
        .fetched(&cached)?
        .ok_or("Shared storyboard was not fetched")?;
    imported(repo, &commit)
}
fn current_files(request: u64, files_request: u64, head: &str, files_head: &str) -> bool {
    request == files_request && head == files_head
}
impl RepoTab {
    pub(in crate::repo) fn refresh_storyboard(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.pr_view else {
            return;
        };
        let (Some(Ok(details)), Some(Ok(files))) = (&view.details, &view.files) else {
            return;
        };
        if !current_files(
            self.pr_requests,
            view.files_request,
            &details.head_oid,
            &files.to,
        ) {
            return;
        }
        let Some(cards) = view.storyboard.cached.clone() else {
            return;
        };
        view.storyboard.invalidate();
        let request = view.storyboard.request;
        let pr_request = self.pr_requests;
        let (repo, files) = (self.path.clone(), files.clone());
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                cards
                    .into_iter()
                    .map(|c| show_card(&repo, c, &files))
                    .collect()
            },
            move |tab, result, _| {
                if tab.pr_requests != pr_request {
                    return;
                }
                if let Some(view) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.storyboard.request == request)
                {
                    view.storyboard.accept(request, result);
                }
            },
        );
    }
    pub(in crate::repo) fn story_act(&mut self, act: StoryAct, ctx: &egui::Context) {
        if let StoryAct::OpenCode { commit, path, line } = act {
            self.review_document.open = false;
            self.dossier.close();
            self.symbols.close();
            self.file_comparison = None;
            // A line card describes the full pinned file, including unchanged/out-of-hunk
            // lines. Its logical line stays on this view until accepted file data is painted.
            self.open_file_view(path, Some(commit), FileTab::File, ctx);
            if let Some(view) = &mut self.file_view {
                view.goto_line
                    .set(usize::try_from(line).ok().filter(|n| *n > 0));
                view.marked_line = usize::try_from(line).ok().filter(|n| *n > 0);
            }
            return;
        }
        if let StoryAct::Fetch { remote, head } | StoryAct::Publish { remote, head } = &act {
            let publish = matches!(act, StoryAct::Publish { .. });
            let (Some(v), Some(observed)) = (
                &mut self.pr_view,
                self.remotes.iter().find(|r| &r.name == remote).cloned(),
            ) else {
                self.status = Some(crate::repo::Notice::plain((
                    "Choose a configured storyboard sharing remote".into(),
                    true,
                )));
                return;
            };
            let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
                return;
            };
            if !current_files(self.pr_requests, v.files_request, &d.head_oid, &f.to)
                || &d.head_oid != head
            {
                self.status = Some(crate::repo::Notice::plain((
                    "Pull request changed; refresh the storyboard".into(),
                    true,
                )));
                return;
            }
            if publish
                && !d
                    .extras
                    .viewer
                    .as_deref()
                    .is_some_and(|viewer| viewer.eq_ignore_ascii_case(&d.author))
            {
                self.status = Some(crate::repo::Notice::plain((
                    "Only the pull request author publishes its storyboard".into(),
                    true,
                )));
                return;
            }
            let (d, f, repo, creds) = (
                d.clone(),
                f.clone(),
                self.path.clone(),
                self.credentials.clone(),
            );
            v.storyboard.request += 1;
            let request = v.storyboard.request;
            v.storyboard.rows = None;
            let number = d.number;
            let pr_request = self.pr_requests;
            self.then_or(
                ctx,
                true,
                Err,
                move || {
                    let target = sharing::Target::capture(&repo, &observed)?;
                    target.guard(&repo)?;
                    if publish {
                        let board = store::load::<Board>(&repo, "storyboard", &key(&d))?
                            .unwrap_or_default();
                        publish_board(&repo, &key(&d), &board, &target, creds.as_ref())?;
                        Ok((read(&repo, &d, &f)?, false))
                    } else {
                        let rows = fetch_board(&repo, &key(&d), &target, creds.as_ref())?
                            .cards
                            .into_iter()
                            .map(|c| show_card(&repo, c, &f))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok((rows, true))
                    }
                },
                move |tab, result, _| {
                    if tab.pr_requests != pr_request {
                        return;
                    }
                    if let Some(v) = tab
                        .pr_view
                        .as_mut()
                        .filter(|v| v.number == number && v.storyboard.request == request)
                    {
                        match result {
                            Ok((rows, imported)) => {
                                v.storyboard.imported = imported;
                                v.storyboard.accept(request, Ok(rows));
                            }
                            Err(e) => v.storyboard.rows = Some(Err(e)),
                        }
                    }
                },
            );
            return;
        }
        let Some(v) = &mut self.pr_view else { return };
        let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
            return;
        };
        if !current_files(self.pr_requests, v.files_request, &d.head_oid, &f.to) {
            self.status = Some(crate::repo::Notice::plain((
                "Pull request files are refreshing; wait before arranging its storyboard".into(),
                true,
            )));
            return;
        }
        let (d, f, path, request) = (
            d.clone(),
            f.clone(),
            self.path.clone(),
            v.storyboard.request + 1,
        );
        v.storyboard.request = request;
        let mutation = !matches!(act, StoryAct::Load);
        if mutation && v.storyboard.imported {
            self.status = Some(crate::repo::Notice::plain((
                "Load your local storyboard before editing".into(),
                true,
            )));
            return;
        }
        v.storyboard.imported = false;
        if mutation
            && !d
                .extras
                .viewer
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(&d.author))
        {
            self.status = Some(crate::repo::Notice::plain((
                "Only the pull request author arranges this storyboard".into(),
                true,
            )));
            return;
        }
        if mutation {
            v.storyboard.rows = None;
        }
        let pr_request = self.pr_requests;
        self.then_or(
            ctx,
            mutation,
            Err,
            move || {
                if mutation {
                    let mut board =
                        store::load::<Board>(&path, "storyboard", &key(&d))?.unwrap_or_default();
                    match act {
                        StoryAct::Add { form, head } if head == f.to => {
                            board.cards.push(card(&path, &d, &f, form)?)
                        }
                        StoryAct::Remove { index, head }
                            if head == f.to && index < board.cards.len() =>
                        {
                            board.cards.remove(index);
                        }
                        StoryAct::Move { index, up, head }
                            if head == f.to && index < board.cards.len() =>
                        {
                            let other = if up {
                                index.checked_sub(1)
                            } else {
                                index.checked_add(1)
                            };
                            if let Some(other) = other.filter(|n| *n < board.cards.len()) {
                                board.cards.swap(index, other);
                            }
                        }
                        _ => return Err("The storyboard selection is stale; reload it".into()),
                    }
                    preserve(&path, &key(&d), &board)?;
                }
                read(&path, &d, &f)
            },
            move |tab, result, _| {
                if tab.pr_requests != pr_request {
                    return;
                }
                if let Some(v) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.storyboard.request == request)
                {
                    v.storyboard.accept(request, result);
                }
            },
        );
    }
}
fn display_phase(phase: &str) -> &str {
    match phase {
        "Old failure" => crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"),
        "Invariant" => crate::i18n::text("repo-workflows-review-label-invariant-59d137"),
        "Fix" => crate::i18n::text("repo-workflows-review-label-fix-21f159"),
        "Result" => crate::i18n::text("repo-workflows-review-label-result-6e7d50"),
        _ => phase,
    }
}
fn display_verdict(verdict: &str) -> &str {
    match verdict {
        "good" => crate::i18n::text("repo-workflows-review-label-good-770e60"),
        "bad" => crate::i18n::text("repo-workflows-review-label-bad-2f05d4"),
        _ => verdict,
    }
}
fn display_test_command(command: &str) -> String {
    const CONFIGURED: &str =
        "Configured when pinned (execution command not recorded by test lane): ";
    match command.strip_prefix(CONFIGURED) {
        Some(command) => crate::i18n::message(
            "repo-workflows-review-raw-configured-when-pinned-execution-command-not-r-ddb1ee",
            &[("command", command.into())],
        ),
        None => command.to_owned(),
    }
}
pub(in crate::repo) fn show(
    ui: &mut egui::Ui,
    state: &mut Storyboard,
    d: &PrDetails,
    f: &PrFiles,
    remotes: &[git::Remote],
    acts: &mut Vec<Action>,
) {
    egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-review-storyboard-f917d1")).id_salt("repo-workflows-review-label-review-storyboard-f917d1").show(ui, |ui| {
        if ui.button(crate::i18n::text("repo-workflows-review-label-load-storyboard-df2f79")).clicked() {
            acts.push(Action::Story(StoryAct::Load));
        }
        egui::ComboBox::from_id_salt("story-sharing-remote").selected_text(if state.remote.is_empty(){crate::i18n::text("repo-workflows-review-label-choose-storyboard-remote-bd2c8d")}else{&state.remote}).show_ui(ui,|ui|{
            for remote in remotes {ui.selectable_value(&mut state.remote,remote.name.clone(),&remote.name);}
        });
        ui.weak(crate::i18n::text("repo-workflows-review-label-sharing-publishes-the-cards-and-their-pinned-s-4f1959"));
        if ui.button(crate::i18n::text("repo-workflows-review-label-fetch-shared-author-storyboard-ec4246")).clicked(){acts.push(Action::Story(StoryAct::Fetch{remote:state.remote.clone(),head:f.to.clone()}));}
        if state.imported {ui.label(crate::i18n::text("repo-workflows-review-label-showing-imported-storyboard-load-the-local-sto-cc81d0"));}
        let own = d
            .extras
            .viewer
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(&d.author));
        if own && ui.button(crate::i18n::text("repo-workflows-review-label-publish-my-local-storyboard-3ee6c4")).clicked(){acts.push(Action::Story(StoryAct::Publish{remote:state.remote.clone(),head:f.to.clone()}));}
        let own=own && !state.imported;
        if let Some(rows) = &state.rows {
            match rows {
                Err(e) => {
                    ui.label(crate::repo::messages::display_status(e));
                }
                Ok(rows) => {
                    for (i, row) in rows.iter().enumerate() {
                        ui.group(|ui| {
                            ui.strong(format!("{}. {}", i + 1, display_phase(&row.card.phase)));
                            ui.label(&row.card.text);
                            ui.weak(crate::i18n::message("repo-workflows-review-message-pinned-base-head-96c6e6", &[("base", (crate::repo::short(&row.card.base)).into()), ("head", (crate::repo::short(&row.card.head)).into())]));
                            if let Some(stale) = &row.stale {
                                ui.colored_label(egui::Color32::YELLOW, crate::i18n::message("repo-workflows-review-message-stale-detail-264fc0", &[("detail", (crate::repo::messages::display_status(stale)).into())]));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if let Some((commit, path, line)) = &row.target
                                    && ui
                                        .button(if *line > 0 {
                                            crate::i18n::text("repo-workflows-review-label-open-pinned-hunk-beb93d")
                                        } else {
                                            crate::i18n::text("repo-workflows-review-label-open-file-4190c0")
                                        })
                                        .clicked()
                                {
                                    open(acts, commit, path, *line);
                                }
                                match &row.card.anchor {
                                    Anchor::Code {
                                        commit, path, line, ..
                                    } => {
                                        if ui.button(crate::i18n::text("repo-workflows-review-label-open-original-revision-e14b40")).clicked() {
                                            open(acts, commit, path, *line);
                                        }
                                    }
                                    Anchor::Test {
                                        tree,
                                        command,
                                        verdict,
                                        ..
                                    } => {
                                        ui.monospace(crate::i18n::message("repo-workflows-review-message-observed-verdict-tree-tree-command-a654d0", &[("verdict", (display_verdict(verdict)).into()), ("tree", (tree.as_str()).into()), ("command", (display_test_command(command)).into())]));
                                    }
                                }
                                if own {
                                    for (up, label) in [(true, "↑"), (false, "↓")] {
                                        if ui.small_button(label).clicked() {
                                            acts.push(Action::Story(StoryAct::Move {
                                                index: i,
                                                up,
                                                head: f.to.clone(),
                                            }));
                                        }
                                    }
                                    if ui.small_button(crate::i18n::text("repo-workflows-review_workflow-show-c3812fc4")).clicked() {
                                        acts.push(Action::Story(StoryAct::Remove {
                                            index: i,
                                            head: f.to.clone(),
                                        }));
                                    }
                                }
                            });
                        });
                    }
                }
            }
        }
        if own {
            let form = &mut state.form;
            egui::ComboBox::from_id_salt("story-phase")
                .selected_text([crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"), crate::i18n::text("repo-workflows-review-label-invariant-59d137"), crate::i18n::text("repo-workflows-review-label-fix-21f159"), crate::i18n::text("repo-workflows-review-label-result-6e7d50")][form.phase])
                .show_ui(ui, |ui| {
                    for (i, name) in [crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"), crate::i18n::text("repo-workflows-review-label-invariant-59d137"), crate::i18n::text("repo-workflows-review-label-fix-21f159"), crate::i18n::text("repo-workflows-review-label-result-6e7d50")]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut form.phase, i, *name);
                    }
                });
            ui.add(egui::TextEdit::multiline(&mut form.text).hint_text(crate::i18n::text("repo-workflows-review-label-explain-this-step-7f09ed")));
            ui.checkbox(&mut form.test, crate::i18n::text("repo-workflows-review-label-pin-a-test-lane-result-at-this-revision-6f1f6f"));
            ui.add(
                egui::TextEdit::singleline(&mut form.commit)
                    .hint_text(crate::i18n::text("repo-workflows-review-label-commit-empty-current-pr-head-6e6c30")),
            );
            if !form.test {
                ui.add(
                    egui::TextEdit::singleline(&mut form.path)
                        .hint_text(crate::i18n::text("repo-workflows-review-label-repository-relative-path-3f3f0a")),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut form.line)
                        .hint_text(crate::i18n::text("repo-workflows-review-label-line-empty-entire-file-afe710")),
                );
            }
            if ui.button(crate::i18n::text("repo-workflows-review-label-add-revision-pinned-card-634c2d")).clicked() {
                acts.push(Action::Story(StoryAct::Add {
                    form: form.clone(),
                    head: f.to.clone(),
                }));
            }
        }
        ui.weak(crate::i18n::text("repo-workflows-review-label-stored-in-this-clone-cards-never-execute-comma-4cb802"));
    });
}
fn open(acts: &mut Vec<Action>, commit: &str, path: &str, line: u64) {
    if line > 0 {
        acts.push(Action::Story(StoryAct::OpenCode {
            commit: commit.into(),
            path: path.into(),
            line,
        }));
    } else {
        acts.push(Action::FileView {
            path: path.into(),
            rev: Some(commit.into()),
            tab: FileTab::File,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r2_pure_storyboard_requires_current_files_even_when_refreshed_head_is_unchanged() {
        assert!(current_files(2, 2, "head", "head"));
        assert!(!current_files(2, 1, "head", "head"));
        assert!(!current_files(2, 2, "new", "old"));
    }
    #[test]
    fn cards_follow_range_diff_pairs_and_flag_changed_content() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("storyboard")?;
        let base = r.tip("HEAD")?;
        let old = r.commit("a.rs", "original\n", "fix")?;
        let blob = file_blob(r.path(), &old, "a.rs")?;
        let c = Card {
            phase: "Fix".into(),
            text: "invariant".into(),
            base: base.clone(),
            head: old.clone(),
            anchor: Anchor::Code {
                commit: old,
                path: "a.rs".into(),
                line: 1,
                blob,
            },
        };
        r.git(&["commit", "--amend", "-m", "rewritten message"])?;
        let new = r.tip("HEAD")?;
        let mut f = PrFiles {
            from: base,
            to: new.clone(),
            files: vec![],
        };
        let shown = show_card(r.path(), c.clone(), &f)?;
        assert_eq!(shown.target.map(|t| t.0), Some(new));
        assert!(shown.stale.is_none());
        f.to = r.commit("a.rs", "different\n", "change")?;
        assert!(show_card(r.path(), c, &f)?.stale.is_some());
        Ok(())
    }
    #[test]
    fn sharing_roundtrip_retains_rewritten_sources_and_never_moves_a_branch() -> Result<(), String>
    {
        let author = git::stack::test_repo::TestRepo::new("story-author")?;
        let server = git::stack::test_repo::TestRepo::new("story-server")?;
        let reviewer = git::stack::test_repo::TestRepo::new("story-reader")?;
        let upstream = git::stack::test_repo::TestRepo::new("story-upstream")?;
        let base = author.tip("HEAD")?;
        let pinned = author.commit("old.rs", "old invariant\n", "original fix")?;
        let board = Board {
            cards: vec![Card {
                phase: "Fix".into(),
                text: "why".into(),
                base: base.clone(),
                head: pinned.clone(),
                anchor: Anchor::Code {
                    commit: pinned.clone(),
                    path: "old.rs".into(),
                    line: 1,
                    blob: file_blob(author.path(), &pinned, "old.rs")?,
                },
            }],
        };
        author.git(&["reset", "--hard", &base])?;
        let server_head = server.tip("HEAD")?;
        let reviewer_head = reviewer.tip("HEAD")?;
        author.git(&[
            "remote",
            "add",
            "team",
            upstream.path().to_str().ok_or("path")?,
        ])?;
        author.git(&[
            "remote",
            "set-url",
            "--push",
            "team",
            server.path().to_str().ok_or("path")?,
        ])?;
        reviewer.git(&[
            "remote",
            "add",
            "team",
            server.path().to_str().ok_or("path")?,
        ])?;
        let observed = git::remotes(author.path())?
            .into_iter()
            .find(|r| r.name == "team")
            .ok_or("remote")?;
        let target = sharing::Target::capture(author.path(), &observed)?;
        publish_board(author.path(), "example#1", &board, &target, None)
            .map_err(|e| format!("first publish: {e}"))?;
        let observed = git::remotes(reviewer.path())?
            .into_iter()
            .find(|r| r.name == "team")
            .ok_or("remote")?;
        let read_target = sharing::Target::capture(reviewer.path(), &observed)?;
        let imported = fetch_board(reviewer.path(), "example#1", &read_target, None)
            .map_err(|e| format!("first fetch: {e}"))?;
        assert_eq!(imported.cards.len(), 1);
        assert_eq!(oid(reviewer.path(), &pinned)?, pinned);
        assert_eq!(
            git::run(
                Some(reviewer.path()),
                &["show", &format!("{pinned}:old.rs")]
            )?
            .trim(),
            "old invariant"
        );
        let previous = oid(server.path(), &shared_ref("example#1"))?;
        // First publication updates the live cache; this target retains its private view.
        // Report both witnesses if publication cannot refresh that cache.
        let cached = cached_ref("example#1", &target);
        let live_cache = oid(author.path(), &cached)?;
        let captured_cache = target.fetched(&cached)?;
        publish_board(author.path(), "example#1", &Board::default(), &target, None)
            .map_err(|e| format!("second publish (live cache {live_cache}, private cache {captured_cache:?}): {e}"))?;
        let latest = oid(server.path(), &shared_ref("example#1"))?;
        assert!(git::is_ancestor(server.path(), &previous, &latest));
        assert!(
            fetch_board(reviewer.path(), "example#1", &read_target, None)
                .map_err(|e| format!("second fetch: {e}"))?
                .cards
                .is_empty()
        );
        // Reusing the same captured target must refresh the already populated cache,
        // including a no-change fetch. Its old-OID guard must still refuse a stale writer.
        assert_eq!(
            oid(reviewer.path(), &cached_ref("example#1", &read_target))?,
            latest
        );
        assert!(
            fetch_board(reviewer.path(), "example#1", &read_target, None)?
                .cards
                .is_empty()
        );
        let cached = cached_ref("example#1", &read_target);
        let transaction =
            format!("start\nupdate {cached} {previous} {previous}\nprepare\ncommit\n");
        assert!(
            git::run_input(
                Some(reviewer.path()),
                &["update-ref", "--stdin"],
                &transaction
            )
            .is_err()
        );
        assert_eq!(oid(reviewer.path(), &cached)?, latest);
        assert_eq!(author.tip("HEAD")?, base);
        assert_eq!(server.tip("HEAD")?, server_head);
        assert_eq!(reviewer.tip("HEAD")?, reviewer_head);
        Ok(())
    }
    #[test]
    fn r2_pure_storyboard_refresh_invalidates_rows_preserves_provenance_and_rejects_late_results() {
        let card = Card {
            phase: "Result".into(),
            text: "tested".into(),
            base: "A".into(),
            head: "B".into(),
            anchor: Anchor::Test {
                commit: "B".into(),
                tree: "treeB".into(),
                command: "test".into(),
                verdict: "good".into(),
            },
        };
        let mut state = Storyboard {
            imported: true,
            ..Default::default()
        };
        let shown = || {
            vec![Shown {
                card: card.clone(),
                target: None,
                stale: None,
            }]
        };
        state.accept(0, Ok(shown()));
        state.invalidate();
        assert!(state.rows.is_none());
        assert!(state.cached.is_some());
        assert!(state.imported);
        state.accept(0, Ok(shown()));
        assert!(state.rows.is_none());
        let request = state.request;
        state.accept(
            request,
            Ok(vec![Shown {
                card,
                target: None,
                stale: Some("Current tree differs".into()),
            }]),
        );
        assert!(
            state
                .rows
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .is_some_and(|rows| rows[0].stale.is_some())
        );
        assert!(state.imported);
    }
}
#[cfg(test)]
mod fluent_review_tests {
    use super::*;
    use crate::repo::review_workflow::presentation_tests::{click, paint};

    #[test]
    fn fluent_review_storyboard_actual_phase_adapter_keeps_persisted_metadata_and_stale_generation()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let details = PrDetails {
            author: "author".into(),
            head_oid: "head{opaque}".into(),
            ..Default::default()
        };
        let files = PrFiles {
            from: "base{opaque}".into(),
            to: details.head_oid.clone(),
            files: vec![],
        };
        let card = Card {
            phase: "Old failure".into(),
            text: "User story --argv {raw}".into(),
            base: files.from.clone(),
            head: files.to.clone(),
            anchor: Anchor::Test {
                commit: "opaque".into(),
                tree: "tree{raw}".into(),
                command: "cargo test --offline --test user_test".into(),
                verdict: "bad".into(),
            },
        };
        let before = serde_json::to_string(&card).map_err(|e| e.to_string())?;
        let mut state = Storyboard {
            request: 41,
            rows: Some(Ok(vec![Shown {
                card,
                target: None,
                stale: None,
            }])),
            imported: true,
            ..Default::default()
        };
        let mut acts = vec![];
        for (locale, heading, phase, verdict) in [
            ("en-US", "Review storyboard", "Old failure", "Observed bad"),
            (
                "de",
                "Review-Storyboard",
                "Früherer Fehler",
                "Beobachtet schlecht",
            ),
        ] {
            crate::i18n::set_language(locale);
            if locale == "en-US" {
                click(&ctx, heading, |ui| {
                    show(ui, &mut state, &details, &files, &[], &mut acts)
                })?;
            }
            let text = paint(&ctx, |ui| {
                show(ui, &mut state, &details, &files, &[], &mut acts)
            });
            assert!(text.contains(phase) && text.contains(verdict), "{text}");
            assert!(
                text.contains("cargo test --offline --test user_test")
                    && text.contains("User story --argv {raw}"),
                "{text}"
            );
        }
        assert_eq!(
            display_phase("Unknown user phase {raw}"),
            "Unknown user phase {raw}"
        );
        assert_eq!(display_verdict("custom --verdict"), "custom --verdict");
        let stored = &state
            .rows
            .as_ref()
            .ok_or("rows")?
            .as_ref()
            .map_err(|e| e.as_str())?[0]
            .card;
        assert_eq!(
            serde_json::to_string(stored).map_err(|e| e.to_string())?,
            before
        );
        assert_eq!(state.request, 41);
        assert!(acts.is_empty());
        state.accept(40, Err("stale response".into()));
        assert!(state.rows.as_ref().ok_or("rows")?.is_ok());
        Ok(())
    }
}
#[cfg(test)]
mod fluent_review_r1_tests {
    use super::*;
    use crate::repo::review_workflow::presentation_tests::{click, paint};

    #[test]
    fn fluent_review_r1_actual_storyboard_commands_are_opaque_even_when_operation_names_collide()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let details = PrDetails {
            author: "author".into(),
            head_oid: "head".into(),
            ..Default::default()
        };
        let files = PrFiles {
            from: "base".into(),
            to: "head".into(),
            files: vec![],
        };
        let mut state = Storyboard {
            rows: Some(Ok(vec![Shown {
                card: Card {
                    phase: "Result".into(),
                    text: "User story {raw}".into(),
                    base: "base".into(),
                    head: "head".into(),
                    anchor: Anchor::Test {
                        commit: "oid".into(),
                        tree: "tree{opaque}".into(),
                        command: "Stage".into(),
                        verdict: "bad".into(),
                    },
                },
                target: None,
                stale: None,
            }])),
            imported: true,
            ..Default::default()
        };
        let mut acts = vec![];
        crate::i18n::set_language("en-US");
        click(&ctx, "Review storyboard", |ui| {
            show(ui, &mut state, &details, &files, &[], &mut acts)
        })?;
        const PREFIX: &str =
            "Configured when pinned (execution command not recorded by test lane): ";
        let commands = [
            "Stage".to_owned(),
            "Undo Stage".to_owned(),
            "Preparing Stage".to_owned(),
            "cargo test --offline --test user_test".to_owned(),
            "Custom {raw} --argv=Stage".to_owned(),
            format!("{PREFIX}Stage"),
            format!("{PREFIX}Undo Stage --argv={{raw}}"),
            format!(" {PREFIX}Stage"),
            "Configured when pinned: Stage".to_owned(),
        ];
        for (locale, envelope) in [
            ("en-US", PREFIX),
            (
                "de",
                "Beim Fixieren konfiguriert (Ausführungsbefehl nicht von der Testspur aufgezeichnet): ",
            ),
        ] {
            crate::i18n::set_language(locale);
            for command in &commands {
                let rows = state
                    .rows
                    .as_mut()
                    .ok_or("rows")?
                    .as_mut()
                    .map_err(|e| e.to_string())?;
                let Anchor::Test {
                    command: recorded, ..
                } = &mut rows[0].card.anchor
                else {
                    return Err("anchor".into());
                };
                *recorded = command.clone();
                let before = serde_json::to_string(&rows[0].card).map_err(|e| e.to_string())?;
                let expected = command
                    .strip_prefix(PREFIX)
                    .map_or_else(|| command.clone(), |suffix| format!("{envelope}{suffix}"));
                let text = paint(&ctx, |ui| {
                    show(ui, &mut state, &details, &files, &[], &mut acts)
                });
                assert!(
                    text.contains(&format!("tree{{opaque}} · {expected}")),
                    "recorded command reinterpreted in {locale}: {text}"
                );
                assert!(
                    !text.contains("Vormerken"),
                    "operation translation reached command data: {text}"
                );
                let rows = state
                    .rows
                    .as_ref()
                    .ok_or("rows")?
                    .as_ref()
                    .map_err(|e| e.to_string())?;
                assert_eq!(
                    serde_json::to_string(&rows[0].card).map_err(|e| e.to_string())?,
                    before
                );
            }
        }
        assert!(acts.is_empty());
        Ok(())
    }
}
