//! Bounded inspection of known paths only: no scanning, fetching, or connecting.
use crate::forge::Hosting;
use crate::git::Remote;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

type Catalog = Vec<(PathBuf, Vec<Remote>)>;
struct Job<T> {
    rx: Receiver<T>,
    _scope: crate::tasks::Scope,
}
#[derive(Default)]
pub(super) struct Local {
    known: Vec<PathBuf>,
    started: bool,
    catalog: Catalog,
    reading: Option<Job<Catalog>>,
    opening: Option<Job<Result<PathBuf, String>>>,
    pub error: Option<String>,
}
fn submit<T: Send + 'static>(
    ctx: &egui::Context,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<Job<T>, String> {
    let scope = crate::tasks::Scope::default();
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    crate::tasks::pool()?
        .submit(
            scope.0.clone(),
            crate::tasks::Priority::Interactive,
            None,
            Box::new(move || {
                let _ = tx.send(work());
                repaint.request_repaint();
            }),
        )
        .map_err(str::to_owned)?;
    Ok(Job { rx, _scope: scope })
}
impl Local {
    pub fn new(known: Vec<PathBuf>) -> Self {
        Self {
            known,
            ..Default::default()
        }
    }
    pub fn retire_open(&mut self) {
        self.opening = None;
        self.error = None;
    }
    pub fn tick(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if !self.started {
            self.started = true;
            let known = std::mem::take(&mut self.known);
            match submit(ctx, move || inspect(known)) {
                Ok(job) => self.reading = Some(job),
                Err(_) => self.error = Some(crate::i18n::text("github-browser-queue").into()),
            }
        }
        if let Some(job) = &self.reading {
            match job.rx.try_recv() {
                Ok(catalog) => {
                    self.catalog = catalog;
                    self.reading = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.reading = None;
                    self.error = Some(crate::i18n::text("github-browser-local-stale").into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.reading.is_some() || self.opening.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let job = self.opening.as_ref()?;
        match job.rx.try_recv() {
            Ok(result) => {
                self.opening = None;
                match result {
                    Ok(root) => return Some(root),
                    Err(error) => self.error = Some(error),
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.opening = None;
                self.error = Some(crate::i18n::text("github-browser-local-stale").into());
            }
            Err(TryRecvError::Empty) => {}
        }
        None
    }
    pub fn matches(&self, hosting: &Hosting) -> Vec<PathBuf> {
        matches(&self.catalog, hosting)
    }
    pub fn open(&mut self, path: PathBuf, hosting: Hosting, ctx: &egui::Context) {
        self.error = None;
        match submit(ctx, move || revalidate(&path, &hosting)) {
            Ok(job) => self.opening = Some(job),
            Err(_) => self.error = Some(crate::i18n::text("github-browser-queue").into()),
        }
    }
}
fn inspect(paths: Vec<PathBuf>) -> Catalog {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| !crate::remote::is_remote(path) && seen.insert(path.clone()))
        .take(128)
        .take_while(|_| {
            std::time::Instant::now() < deadline && !crate::tasks::cancellation().is_cancelled()
        })
        .filter_map(|path| {
            crate::git::remotes(&path)
                .ok()
                .map(|remotes| (path, remotes))
        })
        .collect()
}
fn matches(catalog: &Catalog, hosting: &Hosting) -> Vec<PathBuf> {
    catalog
        .iter()
        .filter(|(_, remotes)| {
            remotes
                .iter()
                .any(|remote| hosting.matches_git_remote(&remote.url))
        })
        .map(|(path, _)| path.clone())
        .collect()
}
fn revalidate(path: &std::path::Path, hosting: &Hosting) -> Result<PathBuf, String> {
    let stale = || crate::i18n::text("github-browser-local-stale").to_owned();
    let root = crate::git::toplevel(path).map_err(|_| stale())?;
    if root.canonicalize().ok() != path.canonicalize().ok() || !path.is_dir() {
        return Err(stale());
    }
    let remotes = crate::git::remotes(&root).map_err(|_| stale())?;
    if !remotes
        .iter()
        .any(|remote| hosting.matches_git_remote(&remote.url))
    {
        return Err(stale());
    }
    Ok(root)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hosting() -> Hosting {
        Hosting {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".into(),
            path: "owner/repo".into(),
            web: "https://github.com".into(),
        }
    }
    #[test]
    fn local_matches_https_ssh_and_non_origin_remotes() {
        let catalog = vec![
            (
                PathBuf::from("one"),
                vec![Remote {
                    name: "upstream".into(),
                    url: "git@github.com:owner/repo.git".into(),
                }],
            ),
            (
                PathBuf::from("two"),
                vec![Remote {
                    name: "other".into(),
                    url: "https://github.com/owner/repo".into(),
                }],
            ),
            (
                PathBuf::from("unrelated"),
                vec![Remote {
                    name: "origin".into(),
                    url: "https://github.com/owner/other".into(),
                }],
            ),
        ];
        assert_eq!(
            matches(&catalog, &hosting()),
            vec![PathBuf::from("one"), PathBuf::from("two")]
        );
    }
    #[test]
    fn local_rejects_stale_path() {
        assert!(
            revalidate(
                std::path::Path::new("/nonexistent/gitcraft-231-local-test"),
                &hosting()
            )
            .is_err()
        );
    }
}
