//! Keep image previews bounded across files and tabs. egui otherwise keeps each image's
//! textures for the context's whole lifetime, even after its diff or pull request is closed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::load::{DefaultTextureLoader, SizeHint, TextureLoadResult, TextureLoader, TexturePoll};
use egui::{Context, TextureOptions};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGES: usize = 512;

fn key() -> egui::Id {
    egui::Id::new("gitcraft-image-cache")
}

pub fn install(ctx: &Context) {
    // Once uploaded, a decoded RGBA copy serves no purpose in this GPU-rendered app.
    ctx.options_mut(|o| o.reduce_texture_memory = true);
    let loader = Arc::new(Tracked::default());
    ctx.add_texture_loader(loader.clone());
    ctx.data_mut(|d| d.insert_temp(key(), loader));
}

/// Before drawing, outside the loader locks: evict old images from every cache together.
/// Images used in the previous pass stay, even when the visible images exceed the budget.
pub fn maintain(ctx: &Context) {
    maintain_with_limits(ctx, MAX_BYTES, MAX_IMAGES);
}

fn maintain_with_limits(ctx: &Context, max_bytes: usize, max_images: usize) {
    let Some(loader) = ctx.data(|d| d.get_temp::<Arc<Tracked>>(key())) else {
        return;
    };
    let evicted = {
        let textures = ctx.tex_manager();
        let textures = textures.read();
        let mut state = loader
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Count every size and filtering variant of a URI, including resized SVG icons.
        for entry in state.images.values_mut() {
            entry.bytes = 0;
        }
        for (_, meta) in textures.allocated() {
            if let Some(entry) = state.images.get_mut(&meta.name) {
                entry.bytes += meta.bytes_used();
            }
        }
        state.evict(max_bytes, max_images)
    };
    for uri in evicted {
        ctx.forget_image(&uri);
    }
}

#[derive(Default)]
struct Tracked {
    inner: DefaultTextureLoader,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    generation: u64,
    images: HashMap<String, Entry>,
}

struct Entry {
    used: u64,
    bytes: usize,
    ready: bool,
}

impl State {
    fn evict(&mut self, max_bytes: usize, max_images: usize) -> Vec<String> {
        self.generation += 1;
        let mut bytes: usize = self.images.values().map(|e| e.bytes).sum();
        let abandoned = self
            .images
            .values()
            .any(|e| !e.ready && e.used + 1 < self.generation);
        if bytes <= max_bytes && self.images.len() <= max_images && !abandoned {
            return Vec::new();
        }
        let mut old: Vec<_> = self
            .images
            .iter()
            .filter(|(_, e)| e.used + 1 < self.generation)
            .map(|(uri, e)| (e.used, uri.clone()))
            .collect();
        old.sort_unstable();
        let mut evicted = Vec::new();
        for (_, uri) in old {
            if bytes <= max_bytes
                && self.images.len() <= max_images
                && self.images.get(&uri).is_some_and(|e| e.ready)
            {
                continue;
            }
            if let Some(entry) = self.images.remove(&uri) {
                bytes = bytes.saturating_sub(entry.bytes);
                evicted.push(uri);
            }
        }
        evicted
    }
}

impl TextureLoader for Tracked {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Tracked)
    }

    fn load(
        &self,
        ctx: &Context,
        uri: &str,
        options: TextureOptions,
        size_hint: SizeHint,
    ) -> TextureLoadResult {
        let result = self.inner.load(ctx, uri, options, size_hint);
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let used = state.generation;
            let entry = state.images.entry(uri.to_owned()).or_insert(Entry {
                used,
                bytes: 0,
                ready: false,
            });
            entry.used = used;
            entry.ready = matches!(result, Ok(TexturePoll::Ready { .. }));
        }
        result
    }

    fn forget(&self, uri: &str) {
        self.inner.forget(uri);
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .images
            .remove(uri);
    }

    fn forget_all(&self) {
        self.inner.forget_all();
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .images
            .clear();
    }

    fn end_pass(&self, pass_index: u64) {
        self.inner.end_pass(pass_index);
    }

    fn byte_size(&self) -> usize {
        self.inner.byte_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ffffff"/></svg>"##;

    fn context() -> Context {
        let ctx = Context::default();
        egui_extras::install_image_loaders(&ctx);
        install(&ctx);
        ctx
    }

    fn load(ctx: &Context, uri: &str, size: u32) -> Result<egui::TextureId, String> {
        ctx.include_bytes(uri.to_owned(), SVG);
        match ctx
            .try_load_texture(
                uri,
                TextureOptions::LINEAR,
                SizeHint::Size {
                    width: size,
                    height: size,
                    maintain_aspect_ratio: true,
                },
            )
            .map_err(|e| e.to_string())?
        {
            TexturePoll::Ready { texture } => Ok(texture.id),
            TexturePoll::Pending { .. } => Err("SVG wasn't loaded synchronously".to_owned()),
        }
    }

    #[test]
    fn old_images_release_their_textures_and_can_be_loaded_again() -> Result<(), String> {
        let ctx = context();
        let a = load(&ctx, "bytes://a.svg", 16)?;
        maintain_with_limits(&ctx, 1024, MAX_IMAGES);
        let b = load(&ctx, "bytes://b.svg", 16)?;
        maintain_with_limits(&ctx, 1024, MAX_IMAGES);
        assert!(ctx.tex_manager().read().meta(a).is_none());
        assert!(ctx.tex_manager().read().meta(b).is_some());
        assert_ne!(load(&ctx, "bytes://a.svg", 16)?, a);
        Ok(())
    }

    #[test]
    fn visible_images_stay_even_when_they_exceed_the_budget() -> Result<(), String> {
        let ctx = context();
        let a = load(&ctx, "bytes://a.svg", 16)?;
        for _ in 0..4 {
            maintain_with_limits(&ctx, 1, 0);
            assert_eq!(load(&ctx, "bytes://a.svg", 16)?, a);
        }
        Ok(())
    }

    #[test]
    fn every_texture_size_counts_toward_the_budget() -> Result<(), String> {
        let ctx = context();
        let small = load(&ctx, "bytes://a.svg", 16)?;
        let large = load(&ctx, "bytes://a.svg", 32)?;
        maintain_with_limits(&ctx, 4096, MAX_IMAGES);
        maintain_with_limits(&ctx, 4096, MAX_IMAGES);
        assert!(ctx.tex_manager().read().meta(small).is_none());
        assert!(ctx.tex_manager().read().meta(large).is_none());
        Ok(())
    }

    #[test]
    fn image_count_is_bounded_and_uploaded_pixels_are_released() -> Result<(), String> {
        let ctx = context();
        let a = load(&ctx, "bytes://a.svg", 16)?;
        assert!(ctx.try_load_bytes("bytes://a.svg").is_err());
        assert_eq!(
            ctx.loaders()
                .image
                .lock()
                .iter()
                .map(|l| l.byte_size())
                .sum::<usize>(),
            0
        );
        maintain_with_limits(&ctx, usize::MAX, 1);
        let b = load(&ctx, "bytes://b.svg", 16)?;
        maintain_with_limits(&ctx, usize::MAX, 1);
        assert!(ctx.tex_manager().read().meta(a).is_none());
        assert!(ctx.tex_manager().read().meta(b).is_some());
        Ok(())
    }

    #[test]
    fn browsing_previews_stays_within_the_memory_budget() -> Result<(), String> {
        let browse = |bounded| -> Result<(usize, usize), String> {
            let ctx = Context::default();
            egui_extras::install_image_loaders(&ctx);
            if bounded {
                install(&ctx);
            }
            for i in 0..48 {
                let mut result = Ok(());
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    if bounded {
                        maintain(ui.ctx());
                    }
                    result = load(ui.ctx(), &format!("bytes://preview/{i}.svg"), 512).map(|_| ());
                })
                .drop_without_applying_deltas();
                result?;
            }
            ctx.run_ui(egui::RawInput::default(), |ui| {
                if bounded {
                    maintain(ui.ctx());
                }
            })
            .drop_without_applying_deltas();
            let textures = ctx
                .tex_manager()
                .read()
                .allocated()
                .filter(|(_, meta)| meta.name.starts_with("bytes://preview/"))
                .map(|(_, meta)| meta.bytes_used())
                .sum();
            let decoded = ctx
                .loaders()
                .image
                .lock()
                .iter()
                .map(|l| l.byte_size())
                .sum();
            Ok((textures, decoded))
        };
        let before = browse(false)?;
        let after = browse(true)?;
        assert_eq!(before, (48 * 1024 * 1024, 48 * 1024 * 1024));
        assert!(after.0 <= MAX_BYTES);
        assert_eq!(after.1, 0);
        println!("48 previews: texture/decoded bytes before={before:?}, after={after:?}");
        Ok(())
    }
}
