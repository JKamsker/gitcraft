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
    /// Prepared once in the analysis worker; paints only borrow these immutable rows.
    opened: Vec<Option<moved_view::Opened>>,
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
                crate::i18n::text("repo-workflows-file_comparison-read-0378d2b4").into(),
                head.into_iter().collect::<Vec<_>>(),
            )
        }
        1 => {
            args.extend(
                ["show", "--format=", "--root", "--diff-merges=first-parent"].map(str::to_owned),
            );
            let oid = files::resolve(repo, to, &live)?;
            (
                crate::i18n::message(
                    "repo-workflows-file_comparison-read-5e3dd301",
                    &[("oid-8-1", (&oid[..8]).into())],
                ),
                vec![oid],
            )
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
                crate::i18n::text("repo-workflows-file_comparison-read-fcc615d7").into(),
                Vec::new(),
            )
        }
        4 => {
            args.push("diff".into());
            let oid = files::resolve(repo, from, &live)?;
            (
                crate::i18n::message(
                    "repo-workflows-file_comparison-read-bf031806",
                    &[("oid-8-1", (&oid[..8]).into())],
                ),
                vec![oid],
            )
        }
        _ => return Err(crate::i18n::text("repo-workflows-file_comparison-read-3fbe0e2d").into()),
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
            return Err(crate::i18n::text("repo-workflows-file_comparison-read-a6767f12").into());
        }
        let Ok(text) = String::from_utf8(raw) else {
            notes.push(crate::i18n::message(
                "repo-workflows-file_comparison-read-f42f63e5",
                &[(
                    "files-display-path-path-as-bytes-1",
                    (files::display_path(path.as_bytes())).clone().into(),
                )],
            ));
            continue;
        };
        if text.contains("ENC[AES256_GCM") {
            notes.push(crate::i18n::message(
                "repo-workflows-file_comparison-read-6289d4ba",
                &[(
                    "files-display-path-path-as-bytes-1",
                    (files::display_path(path.as_bytes())).clone().into(),
                )],
            ));
            continue;
        }
        patches.push(Patch { path, code, text });
    }
    // Reads of mutable index/worktree inputs must form a coherent snapshot. A raw diff
    // manifest alone cannot detect another edit to an already modified working file.
    if matches!(source, 0 | 3 | 4) {
        if run(&["--name-status", "-z"], None)? != names {
            return Err(crate::i18n::text("repo-workflows-file_comparison-read-fc054b54").into());
        }
        for p in &patches {
            if run(&["--patch", "--unified=3"], Some(&p.path))? != p.text.as_bytes() {
                return Err(
                    crate::i18n::text("repo-workflows-file_comparison-read-6adc8019").into(),
                );
            }
        }
        if source == 0
            && files::resolve(repo, "HEAD", &live)
                .ok()
                .into_iter()
                .collect::<Vec<_>>()
                != pins
        {
            return Err(crate::i18n::text("repo-workflows-file_comparison-read-259fd376").into());
        }
    }
    if !live() {
        return Err("Moved-code analysis cancelled or exceeded 40 seconds".into());
    }
    assemble(label, patches, notes, source, &pins, &live)
}
type ChangedPaths = (Vec<(String, char)>, Vec<String>);
fn paths(bytes: &[u8]) -> Result<ChangedPaths, String> {
    let mut fields = bytes.split(|b| *b == 0).filter(|f| !f.is_empty());
    let mut paths = Vec::new();
    let mut notes = Vec::new();
    while let Some(code) = fields.next() {
        let path = fields.next().ok_or(crate::i18n::text(
            "repo-workflows-file_comparison-paths-e3561990",
        ))?;
        let code = *code.first().ok_or(crate::i18n::text(
            "repo-workflows-file_comparison-paths-58fb2d6a",
        ))? as char;
        if !matches!(code, 'A' | 'M' | 'D' | 'T' | 'U') {
            return Err(crate::i18n::text("repo-workflows-file_comparison-paths-ae3de607").into());
        }
        if code == 'U' {
            return Err(crate::i18n::text("repo-workflows-file_comparison-paths-28803ea3").into());
        }
        match std::str::from_utf8(path) {
            Ok(p) => paths.push((p.to_owned(), code)),
            Err(_) => notes.push(crate::i18n::message(
                "repo-workflows-file_comparison-paths-59a504f9",
                &[(
                    "files-display-path-path-1",
                    (files::display_path(path)).clone().into(),
                )],
            )),
        }
        if paths.len() + notes.len() > 300 {
            return Err(crate::i18n::text("repo-workflows-file_comparison-paths-d4097e54").into());
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
    active: &dyn Fn() -> bool,
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
            return Err(
                crate::i18n::text("repo-workflows-file_comparison-assemble-1641af0b").into(),
            );
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
    let mut opened = Vec::with_capacity(data.moves.blocks.len());
    for i in 0..data.moves.blocks.len() {
        if !active() {
            return Err("Moved-code comparison cancelled".into());
        }
        opened.push(moved_view::Opened::new(&data, i));
    }
    if !active() {
        return Err("Moved-code comparison cancelled".into());
    }
    Ok(CodeMoves {
        opened,
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
    ui.label(crate::i18n::message(
        "repo-workflows-file_comparison-view-746eb9db",
        &[("data-locations-len-1", (data.locations.len()).into())],
    ));
    for note in &data.notes {
        ui.colored_label(t.text_dim, note);
    }
    if data.locations.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-file_comparison-view-a47d33a8",
        ));
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
                            crate::i18n::text("repo-workflows-file_comparison-view-b8b66b62")
                        } else {
                            crate::i18n::text("repo-workflows-file_comparison-view-baa2bc90")
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
        ui.colored_label(
            block_color(t, i),
            crate::i18n::message(
                "repo-workflows-file_comparison-view-8bb733fc",
                &[("i-1-1", (i + 1).into())],
            ),
        );
        for (side, l) in loc.iter().enumerate() {
            if ui
                .button(crate::i18n::message(
                    "repo-workflows-file_comparison-view-4699e093",
                    &[
                        (
                            "if-side-0-source-else-destinatio-1",
                            (if side == 0 { "source" } else { "destination" }).into(),
                        ),
                        ("l-label-2", (l.label()).clone().into()),
                    ],
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
    if let Some(opened) = data.opened.get(i).and_then(Option::as_ref) {
        let mut tint = *t;
        tint.accent = block_color(t, i);
        moved_view::content(ui, &tint, &data.data, opened);
    }
}
fn block_color(t: &crate::theme::Tokens, i: usize) -> egui::Color32 {
    [t.accent, t.success, t.warning][i % 3].gamma_multiply(0.7)
}

#[cfg(test)]
#[path = "code_moves_tests.rs"]
mod tests;
