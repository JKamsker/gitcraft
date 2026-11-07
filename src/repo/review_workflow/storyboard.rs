//! Author-arranged, revision-pinned review cards; immutable originals remain available.
use super::store;
use crate::{
    git,
    pull_request::{PrDetails, PrFiles},
    repo::{Action, DiffKey, RepoTab, file_view::FileTab},
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
}
#[derive(Clone)]
pub(crate) enum StoryAct {
    Load,
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
            command,
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
impl RepoTab {
    pub(in crate::repo) fn story_act(&mut self, act: StoryAct, ctx: &egui::Context) {
        if let StoryAct::OpenCode { commit, path, line } = act {
            let key = DiffKey::Commit { hash: commit, path };
            self.review_document.open = false;
            self.open_diff(key.clone(), ctx);
            let repo = self.path.clone();
            self.then_or(
                ctx,
                false,
                Err,
                move || {
                    let data =
                        crate::repo::diff::load(&repo, &key, git::DiffOptions::default(), false)?;
                    Ok((
                        key,
                        data.lines
                            .iter()
                            .position(|l| l.new == u32::try_from(line).ok())
                            .unwrap_or(0),
                    ))
                },
                |tab, result, _| {
                    if let Ok((key, index)) = result
                        && let Some(v) = tab.diff.as_mut().filter(|v| v.key == key)
                    {
                        v.jump_line = Some(index);
                    }
                },
            );
            return;
        }
        let Some(v) = &mut self.pr_view else { return };
        let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
            return;
        };
        let (d, f, path, request) = (
            d.clone(),
            f.clone(),
            self.path.clone(),
            v.storyboard.request + 1,
        );
        v.storyboard.request = request;
        let mutation = !matches!(act, StoryAct::Load);
        if mutation
            && !d
                .extras
                .viewer
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(&d.author))
        {
            self.status = Some((
                "Only the pull request author arranges this storyboard".into(),
                true,
            ));
            return;
        }
        if mutation {
            v.storyboard.rows = None;
        }
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
                if let Some(v) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.storyboard.request == request)
                {
                    v.storyboard.rows = Some(result);
                }
            },
        );
    }
}
pub(in crate::repo) fn show(
    ui: &mut egui::Ui,
    state: &mut Storyboard,
    d: &PrDetails,
    f: &PrFiles,
    acts: &mut Vec<Action>,
) {
    egui::CollapsingHeader::new("Review storyboard").show(ui, |ui| {
        if ui.button("Load storyboard").clicked() {
            acts.push(Action::Story(StoryAct::Load));
        }
        let own = d
            .extras
            .viewer
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(&d.author));
        if let Some(rows) = &state.rows {
            match rows {
                Err(e) => {
                    ui.label(e);
                }
                Ok(rows) => {
                    for (i, row) in rows.iter().enumerate() {
                        ui.group(|ui| {
                            ui.strong(format!("{}. {}", i + 1, row.card.phase));
                            ui.label(&row.card.text);
                            ui.weak(format!(
                                "Pinned {}..{}",
                                crate::repo::short(&row.card.base),
                                crate::repo::short(&row.card.head)
                            ));
                            if let Some(stale) = &row.stale {
                                ui.colored_label(egui::Color32::YELLOW, format!("Stale: {stale}"));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if let Some((commit, path, line)) = &row.target
                                    && ui
                                        .button(if *line > 0 {
                                            "Open pinned hunk"
                                        } else {
                                            "Open file"
                                        })
                                        .clicked()
                                {
                                    open(acts, commit, path, *line);
                                }
                                match &row.card.anchor {
                                    Anchor::Code {
                                        commit, path, line, ..
                                    } => {
                                        if ui.button("Open original revision").clicked() {
                                            open(acts, commit, path, *line);
                                        }
                                    }
                                    Anchor::Test {
                                        tree,
                                        command,
                                        verdict,
                                        ..
                                    } => {
                                        ui.monospace(format!(
                                            "Observed {verdict} · tree {tree} · {command}"
                                        ));
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
                                    if ui.small_button("Remove").clicked() {
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
                .selected_text(["Old failure", "Invariant", "Fix", "Result"][form.phase])
                .show_ui(ui, |ui| {
                    for (i, name) in ["Old failure", "Invariant", "Fix", "Result"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut form.phase, i, *name);
                    }
                });
            ui.add(egui::TextEdit::multiline(&mut form.text).hint_text("Explain this step"));
            ui.checkbox(&mut form.test, "Pin a test-lane result at this revision");
            ui.add(
                egui::TextEdit::singleline(&mut form.commit)
                    .hint_text("Commit (empty = current PR head)"),
            );
            if !form.test {
                ui.add(
                    egui::TextEdit::singleline(&mut form.path)
                        .hint_text("Repository-relative path"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut form.line)
                        .hint_text("Line (empty = entire file)"),
                );
            }
            if ui.button("Add revision-pinned card").clicked() {
                acts.push(Action::Story(StoryAct::Add {
                    form: form.clone(),
                    head: f.to.clone(),
                }));
            }
        }
        ui.weak("Stored in this clone. Cards never execute commands or change the pull request.");
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
}
