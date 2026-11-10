//! Theme discovery snapshots, cache invalidation, and background refresh.
use super::*;

/// The themes read last, and what they were read from.
pub(super) struct Cache {
    pub(super) checked: Instant,
    /// The themes folder's files as they were read (see [`files::fingerprint`]).
    pub(super) folder: Option<u64>,
    pub(super) themes: Arc<Vec<ThemeInfo>>,
    pub(super) errors: Vec<(String, String)>,
}

pub(super) static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// A look at the themes folder and VS Code is under way on another thread.
pub(super) static LOOKING: AtomicBool = AtomicBool::new(false);

/// How often the themes folder and VS Code are looked at again.
pub(super) const RECHECK: Duration = Duration::from_secs(2);

/// Every theme there is to choose. Read once on the first call; then looked at again on
/// another thread when older than a few seconds, so the window never waits for the disk.
pub fn themes() -> Arc<Vec<ThemeInfo>> {
    let stale = match CACHE.lock() {
        Ok(cache) => match cache.as_ref() {
            Some(c) => {
                let themes = c.themes.clone();
                if c.checked.elapsed() >= RECHECK && !LOOKING.swap(true, Ordering::AcqRel) {
                    std::thread::spawn(|| {
                        look_again();
                        LOOKING.store(false, Ordering::Release);
                    });
                }
                return themes;
            }
            None => true,
        },
        Err(_) => return Arc::new(builtin_infos()),
    };
    if stale {
        refresh();
    }
    CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|c| c.themes.clone()))
        .unwrap_or_else(|| Arc::new(builtin_infos()))
}

/// What was read last: the folder's fingerprint, the themes and the files that didn't read.
pub(super) type Snapshot = (Option<u64>, Arc<Vec<ThemeInfo>>, Vec<(String, String)>);

pub(super) fn snapshot() -> Option<Snapshot> {
    let cache = CACHE.lock().ok()?;
    cache
        .as_ref()
        .map(|c| (c.folder, c.themes.clone(), c.errors.clone()))
}

/// Reads the themes again now: after a theme file was added, so it can be chosen at once.
pub fn refresh() {
    let before = snapshot();
    let next = read_themes(before, true);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(next);
    }
}

/// [`refresh`] if anything changed; true when it did. On a thread of its own.
pub(super) fn look_again() -> bool {
    let before = snapshot();
    let old = before.as_ref().map(|(_, themes, _)| themes.clone());
    let next = read_themes(before, false);
    let changed = old.is_none_or(|old| *old != *next.themes);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(next);
    }
    changed
}

/// The themes: the catalogue's, the themes folder's (read again only when its files changed,
/// unless `force`), and VS Code's. A file that no longer reads keeps the theme it was last
/// read as, so a typo while editing it doesn't change the window: it is listed as an error.
pub(super) fn read_themes(before: Option<Snapshot>, force: bool) -> Cache {
    let dir = files::dir();
    let folder = dir.as_deref().and_then(files::fingerprint);
    let mut list = builtin_infos();
    let (own, errors) = match &before {
        Some((was, themes, errors)) if *was == folder && !force => (
            themes
                .iter()
                .filter(|t| t.group == Group::Yours)
                .cloned()
                .collect(),
            errors.clone(),
        ),
        _ => {
            let (read, errors) = dir.as_deref().map(files::scan).unwrap_or_default();
            let mut own: Vec<ThemeInfo> = read
                .into_iter()
                .map(|f| ThemeInfo {
                    id: ThemeId::file(&f.file),
                    name: f.name,
                    credit: f.credit,
                    group: Group::Yours,
                    palette: f.palette,
                })
                .collect();
            if let Some((_, themes, _)) = &before {
                for (file, _) in &errors {
                    let id = ThemeId::file(file);
                    if let Some(last) = themes.iter().find(|t| t.id == id) {
                        own.push(last.clone());
                    }
                }
                own.sort_by(|a, b| a.id.0.cmp(&b.id.0));
            }
            (own, errors)
        }
    };
    list.extend(own);
    if let Some(e) = crate::vscode_theme::current().filter(|e| !e.workbench.is_empty()) {
        list.push(ThemeInfo {
            id: ThemeId::new(ThemeId::VSCODE),
            name: format!("Follow VS Code ({})", e.name),
            credit: "VS Code's current colour theme".to_owned(),
            group: Group::Editor,
            palette: files::from_vscode(&e.workbench, Some(e.dark)),
        });
    }
    Cache {
        checked: Instant::now(),
        folder,
        themes: Arc::new(list),
        errors,
    }
}

/// The files in the themes folder that could not be read as themes, and why.
pub fn theme_errors() -> Vec<(String, String)> {
    let _ = themes();
    CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|c| c.errors.clone()))
        .unwrap_or_default()
}

pub(super) fn builtin_infos() -> Vec<ThemeInfo> {
    catalog::BUILTIN
        .iter()
        .map(|b| {
            let palette = b.palette();
            ThemeInfo {
                id: ThemeId::new(b.id),
                name: b.name.to_owned(),
                credit: b.credit.to_owned(),
                group: if b.credit == "gitcraft"
                    || b.id.chars().next().is_some_and(char::is_uppercase)
                {
                    Group::Gitcraft
                } else if palette.dark {
                    Group::Dark
                } else {
                    Group::Light
                },
                palette,
            }
        })
        .collect()
}

/// The theme `id`, if there is one.
pub fn find(id: &ThemeId) -> Option<ThemeInfo> {
    themes().iter().find(|t| t.id == *id).cloned()
}

/// The theme `id`, or gitcraft's dark one when there is none such any more (a theme file
/// removed, VS Code uninstalled).
pub fn resolve(id: &ThemeId) -> ThemeInfo {
    find(id).unwrap_or_else(|| {
        let mut fallback = builtin_infos().swap_remove(0);
        fallback.id = id.clone();
        fallback
    })
}
