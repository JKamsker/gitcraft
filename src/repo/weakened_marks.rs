//! The _weakens checks_ badges of a tab's file lists: the scans of the commits, comparisons and
//! working trees shown, read in the background one at a time (the one asked for last first)
//! and kept for when they are shown again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Ui;

use super::DiffKey;
use super::widgets::{FileRowResponse, RowExtra};
use crate::theme::Tokens;
use crate::weakened::{Area, FileFlags, Scope};

/// A scan's files that weaken the checks, by path, or why it couldn't be read.
pub(crate) type Found = Arc<Result<HashMap<String, FileFlags>, String>>;

/// Most scans kept; the ones asked for longest ago go first past it.
const KEPT: usize = 48;
/// Most scans waiting; the oldest asked for is dropped past it (it is asked for again when
/// shown again).
const WAITING: usize = 8;

/// A scan kept: what it found for the read numbered `generation` (`None`: not yet), and the
/// last thing found before, shown meanwhile.
#[derive(Default)]
struct Entry {
    generation: u64,
    found: Option<Found>,
    previous: Option<Found>,
}

#[derive(Default)]
struct Store {
    scans: HashMap<Scope, Entry>,
    /// Which scans were asked for, longest ago first.
    order: Vec<Scope>,
    /// Scans to make: (scope, generation), asked for last at the end.
    waiting: Vec<(Scope, u64)>,
    /// A worker thread is reading.
    working: bool,
    /// What it reads now.
    reading: Option<(Scope, u64)>,
}

/// Shared with the background worker, which fills it in.
#[derive(Clone, Default)]
pub(crate) struct WeakenedMarks {
    store: Arc<Mutex<Store>>,
}

impl WeakenedMarks {
    /// The scan of `scope` in `repo` as of the read numbered `generation` (a newer one reads it
    /// again, the last result shown meanwhile): `None` until something is read, the reading
    /// asked for on the first ask, the window repainted when it is in.
    pub(crate) fn get(
        &self,
        ctx: &egui::Context,
        repo: &Path,
        scope: Scope,
        generation: u64,
    ) -> Option<Found> {
        let Ok(mut store) = self.store.lock() else {
            return None;
        };
        let store = &mut *store;
        let entry = store.scans.entry(scope.clone()).or_default();
        if entry.found.is_some() && entry.generation == generation {
            let found = entry.found.clone();
            touch(&mut store.order, &scope);
            return found;
        }
        if entry.generation != generation || entry.found.is_some() {
            entry.previous = entry.found.take().or(entry.previous.take());
            entry.generation = generation;
        }
        let shown = entry.previous.clone();
        touch(&mut store.order, &scope);
        while store.order.len() > KEPT {
            let old = store.order.remove(0);
            store.scans.remove(&old);
        }
        let asked = (scope, generation);
        let queued = store.waiting.contains(&asked) || store.reading.as_ref() == Some(&asked);
        let (scope, generation) = asked;
        if !queued {
            store.waiting.retain(|w| w.0 != scope);
            store.waiting.push((scope, generation));
            if store.waiting.len() > WAITING {
                store.waiting.remove(0);
            }
        }
        if !store.working {
            store.working = true;
            self.start(ctx.clone(), repo.to_owned());
        }
        shown
    }

    /// The worker: reads what is waiting, the last asked for first, until nothing is.
    fn start(&self, ctx: egui::Context, repo: PathBuf) {
        let store = self.store.clone();
        let spawned = std::thread::Builder::new()
            .name("weakened-checks".into())
            .spawn(move || {
                while let Some((scope, generation)) = next(&store) {
                    let found = crate::weakened::scan(&repo, &scope).map(|files| {
                        files
                            .into_iter()
                            .map(|f| (f.path.clone(), f))
                            .collect::<HashMap<_, _>>()
                    });
                    keep(&store, &scope, generation, found);
                    ctx.request_repaint();
                }
            });
        if let Err(e) = spawned
            && let Ok(mut store) = self.store.lock()
        {
            store.working = false;
            let failed = Arc::new(Err(crate::i18n::message(
                "repo-core-3747",
                &[("e", format!("{}", e).into())],
            )));
            for (scope, generation) in std::mem::take(&mut store.waiting) {
                if let Some(entry) = store.scans.get_mut(&scope)
                    && entry.generation == generation
                {
                    entry.found = Some(failed.clone());
                }
            }
        }
    }
}

/// Moves `scope` to the end of `order` (asked for last).
fn touch(order: &mut Vec<Scope>, scope: &Scope) {
    order.retain(|s| s != scope);
    order.push(scope.clone());
}

/// The scan to make next, or `None` (and the worker stops) when nothing waits.
fn next(store: &Mutex<Store>) -> Option<(Scope, u64)> {
    let mut store = store.lock().ok()?;
    let next = store.waiting.pop();
    store.reading = next.clone();
    if next.is_none() {
        store.working = false;
    }
    next
}

/// Keeps what the scan of `scope` found, unless a newer read of it was asked for meanwhile or
/// it was dropped.
fn keep(
    store: &Mutex<Store>,
    scope: &Scope,
    generation: u64,
    found: Result<HashMap<String, FileFlags>, String>,
) {
    if let Ok(mut store) = store.lock()
        && let Some(entry) = store.scans.get_mut(scope)
        && entry.generation == generation
    {
        entry.found = Some(Arc::new(found));
        entry.previous = None;
    }
}

/// The scan a file list opening `key` belongs to, and the read it is of: a commit never
/// changes; a comparison and the working tree change with each read of the repository
/// (`read`).
pub(crate) fn scope_of(key: &DiffKey, read: u64) -> (Scope, u64) {
    match key {
        DiffKey::Remerge { hash, .. } | DiffKey::Commit { hash, .. } => {
            (Scope::Commit(hash.clone()), 0)
        }
        DiffKey::Range { from, to, .. } => (
            Scope::Range {
                from: from.clone(),
                to: to.clone(),
            },
            read,
        ),
        DiffKey::Work(file) => (Scope::Work(area_of(file.area)), read),
    }
}

/// The scan a working-tree file is in.
pub(crate) fn area_of(area: crate::git::Area) -> Area {
    match area {
        crate::git::Area::Staged => Area::Staged,
        crate::git::Area::Untracked => Area::Untracked,
        _ => Area::Unstaged,
    }
}

/// `extra` with the badge when `path` weakens the checks in `found`.
pub(crate) fn extra<'a>(
    found: Option<&'a Found>,
    path: &str,
    extra: &'a RowExtra<'a>,
) -> (RowExtra<'a>, Option<&'a FileFlags>) {
    let flags = found
        .and_then(|f| f.as_ref().as_ref().ok())
        .and_then(|files| files.get(path));
    match flags {
        Some(flags) => (
            RowExtra::Badged(extra, crate::weakened::display::badge()),
            Some(flags),
        ),
        None => (*extra, None),
    }
}

/// The badge's hover text: what was found, line by line.
pub(crate) fn hover(ui: &Ui, row: &FileRowResponse, flags: Option<&FileFlags>) {
    if let (Some(rect), Some(flags)) = (row.badge, flags) {
        ui.interact(rect, row.row.id.with("weakened"), egui::Sense::hover())
            .on_hover_text(flags.display_summary());
    }
}

/// Under a file list: why its files couldn't be looked through, if they couldn't.
pub(crate) fn error_note(ui: &mut Ui, found: Option<&Found>) {
    if let Some(Err(e)) = found.map(|f| f.as_ref()) {
        let t = Tokens::get(ui.ctx());
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(crate::weakened::display::diagnostic(e))
                    .size(11.0)
                    .color(t.text_faint),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{WeakenedMarks, scope_of};
    use crate::repo::DiffKey;
    use crate::weakened::repo_tests::{commit, repo, write};

    /// The badges of a commit's file list and of its diff's hunks, read the way the tab
    /// reads them: in the background, kept once read.
    #[test]
    fn file_rows_and_hunks_get_their_badge() -> Result<(), String> {
        let dir = repo("marks")?;
        write(&dir, "src/a b.rs", "fn a() {}\n\n#[test]\nfn t() {}\n")?;
        commit(&dir, "one")?;
        write(
            &dir,
            "src/a b.rs",
            "fn a() {}\n\n#[test]\n#[ignore]\nfn t() {}\n",
        )?;
        let hash = commit(&dir, "two")?;
        let key = DiffKey::Commit {
            hash: hash.clone(),
            path: String::new(),
        };
        let marks = WeakenedMarks::default();
        let ctx = egui::Context::default();
        let (scope, generation) = scope_of(&key, 0);
        let started = Instant::now();
        let found = loop {
            if let Some(found) = marks.get(&ctx, &dir, scope.clone(), generation) {
                break found;
            }
            if started.elapsed() > Duration::from_secs(20) {
                return Err("the scan never came back".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let files = found.as_ref().as_ref().map_err(Clone::clone)?;
        assert!(files["src/a b.rs"].summary().contains("line 4: #[ignore]"));

        let diff_key = DiffKey::Commit {
            hash,
            path: "src/a b.rs".into(),
        };
        let data = crate::repo::diff::load(&dir, &diff_key, Default::default(), true)?;
        assert_eq!(data.weakened.len(), 1);
        let (line, tip) = data.weakened.iter().next().ok_or("a hunk badge")?;
        assert_eq!(data.lines[*line].kind, crate::repo::diff::Kind::Hunk);
        assert!(
            tip.starts_with("Skips, disables or focuses tests:"),
            "{tip}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// A newer read of the working tree shows the last badges until it is in, and asking
    /// every frame starts one scan, not one per frame.
    #[test]
    fn a_refresh_keeps_the_badges_and_reads_once() -> Result<(), String> {
        use crate::weakened::{Area, Scope};
        let dir = repo("refresh")?;
        write(&dir, "tests/a_test.go", "func TestA(t *testing.T) {\n}\n")?;
        commit(&dir, "one")?;
        write(
            &dir,
            "tests/a_test.go",
            "func TestA(t *testing.T) {\n\tt.Skip()\n}\n",
        )?;
        let marks = WeakenedMarks::default();
        let ctx = egui::Context::default();
        let scope = Scope::Work(Area::Unstaged);
        let wait = |generation| -> Result<_, String> {
            let started = Instant::now();
            loop {
                if let Some(found) = marks.get(&ctx, &dir, scope.clone(), generation)
                    && marks
                        .store
                        .lock()
                        .map(|s| s.scans[&scope].found.is_some())
                        .unwrap_or(false)
                {
                    return Ok(found);
                }
                if started.elapsed() > Duration::from_secs(20) {
                    return Err("the scan never came back".to_owned());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let first = wait(1)?;
        assert!(first.as_ref().as_ref().is_ok_and(|f| f.len() == 1));
        // Read again: the last badges meanwhile, never nothing.
        let meanwhile = marks.get(&ctx, &dir, scope.clone(), 2);
        assert!(meanwhile.is_some());
        let waiting = marks
            .store
            .lock()
            .map(|s| s.waiting.len() + usize::from(s.reading.is_some()));
        assert!(waiting.is_ok_and(|n| n <= 1));
        wait(2)?;
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}

#[cfg(test)]
mod core_guard_paint_tests {
    use super::*;
    use crate::repo::push_preflight::core_guard_paint_tests::{frame, text};

    #[test]
    fn core_guard_paint_file_badge_hover_and_errors_keep_supplied_scan_raw() -> Result<(), String> {
        let ui_errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let ui_errors_0 = ui_errors.clone();

        let path = "tests/opaque12345.rs";
        let flags = FileFlags {
            path: path.into(),
            flags: vec![crate::weakened::Flag {
                reason: crate::weakened::Reason::SkipsTests,
                hunk: Some(0),
                line: Some(12345),
                removed: false,
                evidence: "line 12345: #[ignore] // opaque 12345".into(),
            }],
        };
        let raw = flags.summary();
        let found: Found = Arc::new(Ok(HashMap::from([(path.into(), flags.clone())])));
        let failed: Found = Arc::new(Err("Could not read the changes to look for weakened checks: provider: opaque12345\nEXPECT raw".into()));
        let ctx = egui::Context::default();
        let mut at = egui::Pos2::ZERO;
        for (locale, label, number, error) in [
            (
                "en-US",
                "weakens checks",
                "12,345",
                "Could not read the changes",
            ),
            ("de", "schwächt Prüfungen ab", "12.345", "Die Änderungen"),
        ] {
            crate::i18n::set_language(locale);
            let mut draw = |ui: &mut Ui| {
                let ui_result = (|| -> Result<(), String> {
                    let original = RowExtra::Stats(Some(1), Some(0));
                    let (extra, shown) = extra(Some(&found), path, &original);
                    let row =
                        super::super::widgets::file_row(ui, 'M', path, None, None, false, extra);
                    at = row.badge.ok_or("badge")?.center();
                    hover(ui, &row, shown);
                    error_note(ui, Some(&failed));

                    Ok(())
                })();
                if let Err(error) = ui_result {
                    ui_errors_0.borrow_mut().push(error);
                }
            };
            let _ = frame(&ctx, vec![], &mut draw);
            // Release the closure's mutable borrow before supplying pointer coordinates.
            let pointer = at;
            let mut draw = |ui: &mut Ui| {
                let original = RowExtra::Stats(Some(1), Some(0));
                let (extra, shown) = extra(Some(&found), path, &original);
                let row = super::super::widgets::file_row(ui, 'M', path, None, None, false, extra);
                hover(ui, &row, shown);
                error_note(ui, Some(&failed));
            };
            let events = vec![egui::Event::PointerMoved(pointer)];
            let _ = frame(&ctx, events.clone(), &mut draw);
            let _ = frame(&ctx, events.clone(), &mut draw);
            let shown = text(&frame(&ctx, events, &mut draw));
            for expected in [
                label,
                number,
                "#[ignore] // opaque 12345",
                "provider: opaque12345",
                "EXPECT raw",
                error,
            ] {
                assert!(shown.contains(expected), "missing {expected}: {shown}");
            }
            assert_eq!(
                found
                    .as_ref()
                    .as_ref()
                    .map_err(|error| format!("{error:?}"))?[path],
                flags
            );
            assert_eq!(flags.summary(), raw);
            assert_eq!(
                failed.as_ref().as_ref().err().ok_or("expected an error")?,
                "Could not read the changes to look for weakened checks: provider: opaque12345\nEXPECT raw"
            );
        }
        crate::i18n::set_language("en-US");

        if let Some(error) = ui_errors.borrow().first() {
            return Err(error.clone());
        }

        Ok(())
    }
}
