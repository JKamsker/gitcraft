//! The thumbnails of a picture's history: read and scaled down in the background, two at a
//! time, as the strip asks for them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::ColorImage;

/// How large a thumbnail is, at most.
pub const THUMB: u32 = 96;
/// How many thumbnails are read at once.
const WORKERS: usize = 2;

/// A thumbnail being read, read, or that couldn't be.
#[derive(Clone)]
pub enum Thumb {
    Loading,
    Ready(Arc<ColorImage>),
    Failed,
}

#[derive(Default)]
struct Shared {
    thumbs: HashMap<String, Thumb>,
    /// (commit, the picture's name there) waiting to be read.
    queue: Vec<(String, String)>,
    running: usize,
}

/// The thumbnails of one picture's history, by commit; shared with the threads reading them.
#[derive(Clone, Default)]
pub struct Thumbs {
    shared: Arc<Mutex<Shared>>,
}

impl Thumbs {
    /// The thumbnail of `path` at `hash`, read in the background the first time it is asked
    /// for.
    pub fn get(
        &self,
        ctx: &egui::Context,
        repo: &std::path::Path,
        path: &str,
        hash: &str,
    ) -> Thumb {
        let mut s = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(t) = s.thumbs.get(hash) {
            return t.clone();
        }
        s.thumbs.insert(hash.to_owned(), Thumb::Loading);
        s.queue.push((hash.to_owned(), path.to_owned()));
        if s.running < WORKERS {
            s.running += 1;
            let (shared, ctx, repo) = (self.shared.clone(), ctx.clone(), repo.to_path_buf());
            std::thread::spawn(move || work(&shared, &ctx, &repo));
        }
        Thumb::Loading
    }
}

/// Reads the thumbnails queued, the newest asked for first, until none is left.
fn work(shared: &Mutex<Shared>, ctx: &egui::Context, repo: &std::path::Path) {
    loop {
        let next = {
            let mut s = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match s.queue.pop() {
                Some(next) => next,
                None => {
                    s.running -= 1;
                    return;
                }
            }
        };
        let (hash, path) = next;
        let thumb = std::panic::catch_unwind(|| read(repo, &hash, &path))
            .ok()
            .flatten()
            .map_or(Thumb::Failed, |i| Thumb::Ready(Arc::new(i)));
        shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .thumbs
            .insert(hash, thumb);
        ctx.request_repaint();
    }
}

/// `path` as it is at `hash`, scaled down to fit [`THUMB`].
pub fn read(repo: &std::path::Path, hash: &str, path: &str) -> Option<ColorImage> {
    let bytes = crate::git::file_at(repo, Some(hash), path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    // Scaled down only: a small picture stays as it is.
    let img = if img.width().max(img.height()) > THUMB {
        img.thumbnail(THUMB, THUMB)
    } else {
        img
    }
    .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}
