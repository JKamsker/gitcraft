//! Notebooks through the diff the view loads, on real repositories, and the view drawn.

use std::path::{Path, PathBuf};

use super::compare::Status;
use super::parse::tests::{PNG, notebook};
use super::view::summary;
use super::{NotebookData, notebook_bar};
use crate::git::{self, Area, DiffOptions, StatusFile};
use crate::repo::DiffKey;
use crate::repo::diff::load;

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-nb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn commit(dir: &Path, name: &str, text: &str, message: &str) -> Result<String, String> {
    std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    git::run(Some(dir), &["add", "-A"])?;
    git::run(Some(dir), &["commit", "-qm", message])?;
    Ok(git::head(dir))
}

fn image_output() -> String {
    format!(r#"[{{"output_type":"display_data","data":{{"image/png":"{PNG}"}},"metadata":{{}}}}]"#)
}

/// A notebook changed in a commit: one cell edited and re-run (new output), one only re-run
/// (folded as unchanged), one added, one removed; ids matched; outputs before and after.
#[test]
fn a_notebook_changed_in_a_commit() -> Result<(), String> {
    let dir = repo("commit")?;
    let text_out =
        |s: &str| format!(r#"[{{"output_type":"stream","name":"stdout","text":["{s}\n"]}}]"#);
    let name = "Analysis ü.ipynb";
    commit(
        &dir,
        name,
        &notebook(&[
            ("a", "markdown", "# Sales", None, ""),
            (
                "b",
                "code",
                "x = load()\nx.plot()",
                Some(1),
                &text_out("old"),
            ),
            ("c", "code", "print(1)", Some(2), &text_out("1")),
            ("d", "code", "drop_me()", Some(3), "[]"),
        ]),
        "one",
    )?;
    let hash = commit(
        &dir,
        name,
        &notebook(&[
            ("a", "markdown", "# Sales", None, ""),
            (
                "b",
                "code",
                "x = load(fresh=True)\nx.plot()",
                Some(4),
                &image_output(),
            ),
            ("c", "code", "print(1)", Some(5), &text_out("1")),
            ("e", "code", "summary()", Some(6), "[]"),
        ]),
        "two",
    )?;
    let key = DiffKey::Commit {
        hash,
        path: name.into(),
    };
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    let nb = data.notebook.ok_or("not read as a notebook")?;
    let statuses: Vec<Status> = nb.diff.cells.iter().map(|c| c.status).collect();
    assert_eq!(
        statuses,
        [
            Status::Unchanged,
            Status::Modified,
            Status::Unchanged,
            Status::Removed,
            Status::Added
        ]
    );
    let b = &nb.diff.cells[1];
    assert!(b.outputs_changed && b.count_changed);
    assert!(
        b.source
            .iter()
            .any(|(e, l)| *e == crate::repo::diff::line_diff::Edit::Add
                && l == "x = load(fresh=True)")
    );
    assert!(
        nb.diff.cells[2].count_changed,
        "re-run only: count changed, cell unchanged"
    );
    assert_eq!(summary(&nb), "1 cell changed, 1 added, 1 removed");
    assert!(nb.diff.metadata.is_none());
    assert_eq!(nb.strip, git::StripState::Off);
    assert!(!nb.compared_stripped);
    draw(&nb)?;
    Ok(())
}

/// Draws the bar and the cells in a headless frame: nothing may panic or lock up.
fn draw(nb: &NotebookData) -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut acts = Vec::new();
    let mut json = None;
    for _ in 0..2 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                json = Some(notebook_bar(ui, nb, "test.ipynb", &mut acts));
            });
        });
        out.textures_delta.clear();
    }
    if json == Some(false) && acts.is_empty() {
        Ok(())
    } else {
        Err(format!("drawn as {json:?}, asked {}", acts.len()))
    }
}

/// A new notebook in the working tree reads as all added; a broken one stays JSON.
#[test]
fn new_and_broken_notebooks() -> Result<(), String> {
    let dir = repo("work")?;
    std::fs::write(
        dir.join("new.ipynb"),
        notebook(&[("x", "code", "1+1", None, "[]")]),
    )
    .map_err(|e| e.to_string())?;
    let key = DiffKey::Work(StatusFile {
        path: "new.ipynb".into(),
        old_path: None,
        code: '?',
        area: Area::Untracked,
    });
    let nb = load(&dir, &key, DiffOptions::default(), true)?
        .notebook
        .ok_or("not a notebook")?;
    assert_eq!(nb.diff.cells.len(), 1);
    assert_eq!(nb.diff.cells[0].status, Status::Added);
    draw(&nb)?;

    let hash = commit(&dir, "broken.ipynb", "{ not json", "broken")?;
    let key = DiffKey::Commit {
        hash,
        path: "broken.ipynb".into(),
    };
    let data = load(&dir, &key, DiffOptions::default(), true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(data.notebook.is_none());
    assert!(!data.lines.is_empty(), "shown as its text instead");
    Ok(())
}

/// The strip switch through the tab: turned on, the diff says so and staging drops outputs.
#[test]
fn stripping_outputs_from_the_tab() -> Result<(), String> {
    let dir = repo("strip")?;
    if git::strip_command(&dir).is_err() {
        eprintln!("skipped: neither nbstripout nor Python is installed here");
        return Ok(());
    }
    commit(
        &dir,
        "a.ipynb",
        &notebook(&[("x", "code", "1+1", None, "[]")]),
        "base",
    )?;
    std::fs::write(
        dir.join("a.ipynb"),
        notebook(&[("x", "code", "1+1", Some(9), &image_output())]),
    )
    .map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(dir.clone(), &ctx);
    tab.apply(crate::repo::Action::StripNotebookOutputs(true), &ctx);
    for _ in 0..400 {
        tab.poll(&ctx);
        if tab.status.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    assert!(!status.1, "{}", status.0);
    assert!(git::strips_outputs(&dir));
    git::run(Some(&dir), &["add", "a.ipynb"])?;
    let staged = git::run(Some(&dir), &["show", ":0:a.ipynb"])?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!staged.contains("image/png"), "{staged}");
    Ok(())
}
