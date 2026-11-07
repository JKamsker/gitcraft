//! Change-wide moved blocks. All inputs are raw, bounded Git patches; no converters or keys.
use super::*;
use crate::git::{Area, StatusFile};
use crate::repo::diff::{self, DiffData, DiffLine, Kind, moved, moved_view};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) struct CodeMoves {
    pub label: String,
    pub data: DiffData,
    pub locations: Vec<[Location; 2]>,
    pub notes: Vec<String>,
}
pub(super) struct Location {
    pub key: DiffKey,
    pub line: u32,
}
impl Location {
    pub fn label(&self) -> String {
        format!(
            "{}:{}",
            files::display_path(self.key.path().as_bytes()),
            self.line
        )
    }
}
struct Patch {
    path: String,
    code: char,
    text: String,
}

pub(super) fn read(
    repo: &Path,
    source: u8,
    from: &str,
    to: &str,
    active: &dyn Fn() -> bool,
) -> Result<CodeMoves, String> {
    let started = Instant::now();
    let live = || active() && started.elapsed() < Duration::from_secs(40);
    let mut args: Vec<String> = vec!["--literal-pathspecs".into()];
    let (label, pins) = match source {
        0 => {
            args.extend(["diff".into(), "--cached".into()]);
            // An unborn HEAD is a valid empty before-side.
            let head = files::resolve(repo, "HEAD", &live).ok();
            (
                "Staged changes · HEAD → index".into(),
                head.into_iter().collect::<Vec<_>>(),
            )
        }
        1 => {
            args.extend(
                ["show", "--format=", "--root", "--diff-merges=first-parent"].map(str::to_owned),
            );
            let oid = files::resolve(repo, to, &live)?;
            (format!("Commit {} · first parent", &oid[..8]), vec![oid])
        }
        2 => {
            args.push("diff".into());
            let a = files::resolve(repo, from, &live)?;
            let b = files::resolve(repo, to, &live)?;
            (format!("{} → {}", &a[..8], &b[..8]), vec![a, b])
        }
        3 => {
            args.push("diff".into());
            (
                "Working copy · index → working copy (tracked files)".into(),
                Vec::new(),
            )
        }
        4 => {
            args.push("diff".into());
            let oid = files::resolve(repo, from, &live)?;
            (
                format!("{} → working copy (tracked files)", &oid[..8]),
                vec![oid],
            )
        }
        _ => return Err("Unknown moved-code source".into()),
    };
    args.extend(
        [
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-relative",
            "--diff-algorithm=histogram",
        ]
        .map(str::to_owned),
    );
    args.extend(pins.iter().cloned());
    let run = |extra: &[&str], path: Option<&str>| {
        let mut argv: Vec<_> = args.iter().map(String::as_str).collect();
        // Options precede revision operands. All revisions are pinned object IDs.
        argv.splice(2..2, extra.iter().copied());
        argv.push("--");
        if let Some(path) = path {
            argv.push(path);
        }
        files::raw_output(repo, &argv, &live)
    };
    let names = run(&["--name-status", "-z"], None)?;
    let (paths, mut notes) = paths(&names)?;
    let mut patches = Vec::new();
    let mut bytes = 0usize;
    for (path, code) in paths {
        if !live() {
            return Err("Moved-code analysis cancelled or exceeded 40 seconds".into());
        }
        let raw = run(&["--patch", "--unified=3"], Some(&path))?;
        bytes = bytes.saturating_add(raw.len());
        if bytes > 6 * 1024 * 1024 {
            return Err("Changed patches exceed 6 MiB. Compare a smaller range.".into());
        }
        let Ok(text) = String::from_utf8(raw) else {
            notes.push(format!(
                "{}: non-UTF8 patch skipped",
                files::display_path(path.as_bytes())
            ));
            continue;
        };
        if text.contains("ENC[AES256_GCM") {
            notes.push(format!(
                "{}: encrypted content excluded",
                files::display_path(path.as_bytes())
            ));
            continue;
        }
        patches.push(Patch { path, code, text });
    }
    // Reads of mutable index/worktree inputs must form a coherent snapshot. A raw diff
    // manifest alone cannot detect another edit to an already modified working file.
    if matches!(source, 0 | 3 | 4) {
        if run(&["--name-status", "-z"], None)? != names {
            return Err("Changed paths changed during analysis. Refresh.".into());
        }
        for p in &patches {
            if run(&["--patch", "--unified=3"], Some(&p.path))? != p.text.as_bytes() {
                return Err("File content changed during analysis. Refresh.".into());
            }
        }
        if source == 0
            && files::resolve(repo, "HEAD", &live)
                .ok()
                .into_iter()
                .collect::<Vec<_>>()
                != pins
        {
            return Err("HEAD changed during analysis. Refresh.".into());
        }
    }
    if !live() {
        return Err("Moved-code analysis cancelled or exceeded 40 seconds".into());
    }
    assemble(label, patches, notes, source, &pins)
}
fn paths(bytes: &[u8]) -> Result<(Vec<(String, char)>, Vec<String>), String> {
    let mut fields = bytes.split(|b| *b == 0).filter(|f| !f.is_empty());
    let mut paths = Vec::new();
    let mut notes = Vec::new();
    while let Some(code) = fields.next() {
        let path = fields.next().ok_or("Incomplete changed-file list")?;
        let code = *code.first().ok_or("Missing change status")? as char;
        if !matches!(code, 'A' | 'M' | 'D' | 'T' | 'U') {
            return Err("Unexpected changed-file status".into());
        }
        if code == 'U' {
            return Err("Resolve conflicts before analyzing moved code".into());
        }
        match std::str::from_utf8(path) {
            Ok(p) => paths.push((p.to_owned(), code)),
            Err(_) => notes.push(format!(
                "{}: non-UTF8 path skipped",
                files::display_path(path)
            )),
        }
        if paths.len() + notes.len() > 300 {
            return Err("More than 300 changed files. Compare a smaller range.".into());
        }
    }
    Ok((paths, notes))
}
fn assemble(
    label: String,
    patches: Vec<Patch>,
    notes: Vec<String>,
    source: u8,
    pins: &[String],
) -> Result<CodeMoves, String> {
    let mut lines = Vec::new();
    let mut owners = Vec::new();
    for (i, p) in patches.iter().enumerate() {
        // A boundary prevents candidates/edited gaps joining unrelated file ends.
        lines.push(DiffLine {
            kind: Kind::Hunk,
            old: None,
            new: None,
            text: p.path.clone(),
            raw: String::new(),
        });
        owners.push(i);
        for l in diff::parse(&p.text) {
            lines.push(l);
            owners.push(i);
        }
        if lines.len() >= 50_000 {
            return Err("More than 50,000 patch lines. Compare a smaller range.".into());
        }
    }
    let detected = moved::detect(&lines);
    let blocks: Vec<_> = detected
        .blocks
        .into_iter()
        .filter(|b| owners[b.del.start] != owners[b.add.start])
        .collect();
    let key = |p: &Patch| match source {
        0 | 3 => DiffKey::Work(StatusFile {
            path: p.path.clone(),
            old_path: None,
            code: p.code,
            area: if source == 0 {
                Area::Staged
            } else {
                Area::Unstaged
            },
        }),
        1 => DiffKey::Commit {
            hash: pins[0].clone(),
            path: p.path.clone(),
        },
        _ => DiffKey::Range {
            from: pins[0].clone(),
            to: pins.get(1).cloned(),
            path: p.path.clone(),
        },
    };
    let locations = blocks
        .iter()
        .map(|b| {
            [
                Location {
                    key: key(&patches[owners[b.del.start]]),
                    line: lines[b.del.start].old.unwrap_or(1),
                },
                Location {
                    key: key(&patches[owners[b.add.start]]),
                    line: lines[b.add.start].new.unwrap_or(1),
                },
            ]
        })
        .collect();
    let mut of_line = vec![None; lines.len()];
    for (i, b) in blocks.iter().enumerate() {
        for at in b.del.clone().chain(b.add.clone()) {
            of_line[at] = Some((i, detected.of_line[at].is_some_and(|(_, same)| same)));
        }
    }
    let mut data = DiffData::default();
    data.lines = lines;
    data.moves = moved::Moves { blocks, of_line };
    Ok(CodeMoves {
        label,
        data,
        locations,
        notes,
    })
}

pub(super) fn view(
    ui: &mut egui::Ui,
    state: &mut State,
    data: &CodeMoves,
    t: &crate::theme::Tokens,
    acts: &mut Vec<Action>,
) {
    ui.label(&data.label);
    ui.label(format!(
        "{} blocks moved across files · shared M-markers pair both locations",
        data.locations.len()
    ));
    for note in &data.notes {
        ui.colored_label(t.text_dim, note);
    }
    if data.locations.is_empty() {
        ui.label("No cross-file moved blocks found (minimum 20 letters/digits). Same-file moves remain in the normal patch.");
    }
    egui::ScrollArea::vertical()
        .id_salt("code-move-list")
        .max_height(190.)
        .show(ui, |ui| {
            for (i, loc) in data.locations.iter().enumerate() {
                let color = block_color(t, i);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("M{}", i + 1)).color(color));
                    if ui
                        .selectable_label(
                            state.code_move == Some(i),
                            format!("{} → {}", loc[0].label(), loc[1].label()),
                        )
                        .clicked()
                    {
                        state.code_move = Some(i);
                    }
                    ui.colored_label(
                        t.text_dim,
                        if data.data.moves.blocks[i].exact {
                            "unchanged / re-indented"
                        } else {
                            "edited during move"
                        },
                    );
                });
            }
        });
    let Some(i) = state.code_move.filter(|i| *i < data.locations.len()) else {
        return;
    };
    let loc = &data.locations[i];
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(block_color(t, i), format!("M{} · Moved to…", i + 1));
        for (side, l) in loc.iter().enumerate() {
            if ui
                .button(format!(
                    "Open {} {}",
                    if side == 0 { "source" } else { "destination" },
                    l.label()
                ))
                .clicked()
            {
                acts.push(Action::OpenDiff(l.key.clone()));
                acts.push(Action::DiffGoToLine {
                    old: (side == 0).then_some(l.line),
                    new: (side == 1).then_some(l.line),
                });
            }
        }
    });
    if let Some(opened) = moved_view::Opened::new(&data.data, i) {
        let mut tint = *t;
        tint.accent = block_color(t, i);
        moved_view::content(ui, &tint, &data.data, &opened);
    }
}
fn block_color(t: &crate::theme::Tokens, i: usize) -> egui::Color32 {
    [t.accent, t.success, t.warning][i % 3].gamma_multiply(0.7)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn patch(path: &str, body: &str) -> Patch {
        Patch {
            path: path.into(),
            code: 'M',
            text: format!("--- a/{path}\n+++ b/{path}\n@@ -1,5 +1,5 @@\n{body}"),
        }
    }
    #[test]
    fn finds_an_edited_reindented_cross_file_block_with_shared_markers() {
        let a = patch(
            "old/module.rs",
            "-fn important_validation_function(value: i32) {\n-    validate_input(value);\n-    let amount = 1;\n-    record_the_validation_result(value);\n-}\n",
        );
        let b = patch(
            "new/module.rs",
            "+    fn important_validation_function(value: i32) {\n+        validate_input(value);\n+        let amount = 2;\n+        record_the_validation_result(value);\n+    }\n",
        );
        let data = assemble(
            "test".into(),
            vec![a, b],
            vec![],
            2,
            &["a".repeat(40), "b".repeat(40)],
        )
        .unwrap();
        assert_eq!(data.locations.len(), 1);
        let b = &data.data.moves.blocks[0];
        assert!(!b.exact);
        assert_eq!(data.locations[0][0].key.path(), "old/module.rs");
        assert_eq!(data.locations[0][1].key.path(), "new/module.rs");
        assert_eq!(data.data.moves.at(b.del.start), Some((0, true)));
        assert_eq!(data.data.moves.at(b.add.start), Some((0, true)));
        assert_eq!(
            moved_view::Opened::new(&data.data, 0)
                .unwrap()
                .edited_lines(),
            1
        );
    }
    #[test]
    fn same_file_moves_are_left_to_the_existing_diff() {
        let a = patch(
            "one.rs",
            "-fn important_validation_function() {}\n context\n+fn important_validation_function() {}\n",
        );
        assert!(
            assemble("".into(), vec![a], vec![], 0, &[])
                .unwrap()
                .locations
                .is_empty()
        );
    }
    #[test]
    fn paths_preserve_literal_metacharacters_and_refuse_conflicts() {
        assert_eq!(paths(b"M\0a\n:(glob)*\0").unwrap().0[0].0, "a\n:(glob)*");
        assert!(paths(b"U\0a\0").is_err());
    }
    #[test]
    fn reads_staged_commit_range_and_working_snapshots() {
        use crate::git::file_comparison::tests::Repo;
        let temp = Repo::new().unwrap();
        let p = &temp.0;
        let git = |_: &Path, args: &[&str]| temp.git(args).unwrap();
        let content = "fn important_validation_function(value: i32) {\n    validate_input(value);\n    record_the_validation_result(value);\n}\n";
        std::fs::write(p.join("a.rs"), content).unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-qm", "base"]);
        std::fs::write(p.join("a.rs"), "").unwrap();
        std::fs::write(p.join("b.rs"), content).unwrap();
        git(p, &["add", "."]);
        let data = read(p, 0, "", "", &|| true).unwrap();
        assert_eq!(data.locations.len(), 1);
        git(p, &["commit", "-qm", "move"]);
        assert_eq!(read(p, 1, "", "HEAD", &|| true).unwrap().locations.len(), 1);
        assert_eq!(
            read(p, 2, "HEAD~1", "HEAD", &|| true)
                .unwrap()
                .locations
                .len(),
            1
        );
        std::fs::write(p.join("a.rs"), content).unwrap();
        std::fs::write(p.join("b.rs"), "").unwrap();
        assert_eq!(read(p, 3, "", "", &|| true).unwrap().locations.len(), 1);
        assert!(read(p, 3, "", "", &|| false).is_err());
    }
}
