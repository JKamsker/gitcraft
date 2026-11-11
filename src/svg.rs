//! Bounded, resource-isolated SVG rendering. Parsing/rasterization runs in shared workers.
use egui::load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use resvg::usvg;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
const SOURCE_LIMIT: usize = 256 * 1024;
/// A built-in icon at most this large is drawn where it is asked for (see `load`).
const ICON_LIMIT: usize = 8 * 1024;
const PIXELS: u64 = 1024 * 1024;
type ResultImage = Result<Arc<egui::ColorImage>, String>;
struct Job {
    result: Mutex<Option<ResultImage>>,
    scope: crate::tasks::Scope,
}
#[derive(Default)]
struct Loader(Mutex<HashMap<(String, SizeHint), Arc<Job>>>);
pub(crate) fn install(ctx: &egui::Context) {
    // Last installed gets first refusal, including errors: unsafe defaults never see SVGs.
    ctx.add_image_loader(Arc::new(Loader::default()));
}
impl ImageLoader for Loader {
    fn id(&self) -> &str {
        "gitcraft-resource-isolated-svg"
    }
    fn load(&self, ctx: &egui::Context, uri: &str, hint: SizeHint) -> ImageLoadResult {
        if !egui::load::has_extension(uri, "svg") {
            return Err(LoadError::NotSupported);
        }
        let mut jobs = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (uri.to_owned(), hint);
        if let Some(job) = jobs.get(&key) {
            return match job
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
            {
                Some(Ok(image)) => Ok(ImagePoll::Ready {
                    image: image.clone(),
                }),
                Some(Err(e)) => Err(LoadError::Loading(e.clone())),
                None => Ok(ImagePoll::Pending { size: None }),
            };
        }
        let bytes = match ctx.try_load_bytes(uri)? {
            BytesPoll::Pending { .. } => return Ok(ImagePoll::Pending { size: None }),
            BytesPoll::Ready { bytes, .. } => bytes,
        };
        if bytes.len() > SOURCE_LIMIT {
            return Err(LoadError::Loading(
                "SVG exceeds 256 KiB; use source view".into(),
            ));
        }
        // The app's own icons: a few hundred bytes each, drawn in a fraction of a millisecond,
        // so right away rather than after the queue: no icon is missing from the first frame.
        if uri.starts_with("bytes://icons/") && bytes.len() <= ICON_LIMIT {
            let result = render(&bytes, hint).map(Arc::new);
            let job = Arc::new(Job {
                result: Mutex::new(Some(result.clone())),
                scope: crate::tasks::Scope::default(),
            });
            jobs.insert(key, job);
            return result
                .map(|image| ImagePoll::Ready { image })
                .map_err(LoadError::Loading);
        }
        let (mut pending, mut cached_bytes) = (0, 0);
        for job in jobs.values() {
            match job
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
            {
                None => pending += 1,
                Some(Ok(image)) => cached_bytes += image.pixels.len() * 4,
                _ => (),
            }
        }
        if pending >= 8 {
            return Ok(ImagePoll::Pending { size: None });
        }
        if jobs.len() >= 512 || cached_bytes >= 32 * 1024 * 1024 {
            return Err(LoadError::Loading("SVG preview cache is full".into()));
        }
        let job = Arc::new(Job {
            result: Mutex::new(None),
            scope: crate::tasks::Scope::default(),
        });
        let weak = Arc::downgrade(&job);
        let ctx = ctx.clone();
        let work = move || {
            let result = render(&bytes, hint).map(Arc::new);
            if let Some(job) = weak.upgrade() {
                *job.result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
                ctx.request_repaint();
            }
        };
        crate::tasks::pool()
            .map_err(LoadError::Loading)?
            .submit(
                job.scope.0.clone(),
                crate::tasks::Priority::Background,
                None,
                Box::new(work),
            )
            .map_err(|e| LoadError::Loading(e.into()))?;
        jobs.insert(key, job);
        Ok(ImagePoll::Pending { size: None })
    }
    fn forget(&self, uri: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|(u, _), _| u != uri);
    }
    fn forget_all(&self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    fn has_pending(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .any(|j| {
                j.result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_none()
            })
    }
    fn byte_size(&self) -> usize {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|j| {
                j.result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map_or(0, |i| i.pixels.len() * 4)
            })
            .sum()
    }
}
mod raster;
use raster::*;

#[cfg(test)]
mod tests;
