//! Bounded, resource-isolated SVG rendering. Parsing/rasterization runs in shared workers.
use egui::load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use resvg::usvg;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
const SOURCE_LIMIT: usize = 256 * 1024;
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
fn render(bytes: &[u8], hint: SizeHint) -> Result<egui::ColorImage, String> {
    if bytes.len() > SOURCE_LIMIT {
        return Err("SVG exceeds 256 KiB; use source view".into());
    }
    // Reject XML entities/DTD, compressed SVG, expansion primitives and expensive effect
    // graphs before usvg sees them. Limits bound parsing and render work, not just output.
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG must be uncompressed UTF-8")?;
    if text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err("SVG declarations are unsupported; use source view".into());
    }
    let doc = roxmltree::Document::parse(text).map_err(|_| "Invalid SVG")?;
    let mut nodes = 0;
    let mut complexity = 0usize;
    for n in doc.descendants().filter(|n| n.is_element()) {
        nodes += 1;
        complexity = complexity.saturating_add(
            1 + n
                .attributes()
                .filter(|a| matches!(a.name(), "d" | "points"))
                .map(|a| a.value().len() / 16)
                .sum::<usize>(),
        );
        if nodes > 4096 || n.ancestors().count() > 64 {
            return Err("SVG complexity exceeds preview limits; use source view".into());
        }
        if matches!(
            n.tag_name().name(),
            "use" | "filter" | "mask" | "pattern" | "foreignObject"
        ) {
            return Err(
                "SVG expansion/effect graphs are unsupported in previews; use source view".into(),
            );
        }
    }
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    // Only small, decoded and re-encoded raster images. Nested SVG/data URLs cannot
    // recursively expand; raster headers cannot request unbounded decoder allocations.
    let decoded_pixels = std::sync::atomic::AtomicU64::new(0);
    options.image_href_resolver.resolve_data = Box::new(move |mime, bytes, _| {
        use image::ImageEncoder;
        if bytes.len() > 128 * 1024 || !matches!(mime, "image/png" | "image/jpeg") {
            return None;
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes.as_ref()))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1024);
        limits.max_image_height = Some(1024);
        limits.max_alloc = Some(8 * 1024 * 1024);
        reader.limits(limits);
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes.as_ref()))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?;
        let pixels = u64::from(width) * u64::from(height);
        if pixels > PIXELS
            || decoded_pixels
                .fetch_add(pixels, std::sync::atomic::Ordering::Relaxed)
                .saturating_add(pixels)
                > 2 * PIXELS
        {
            return None;
        }
        let decoded = reader.decode().ok()?.to_rgba8();
        if u64::from(decoded.width()) * u64::from(decoded.height()) > PIXELS {
            return None;
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &decoded,
                decoded.width(),
                decoded.height(),
                image::ExtendedColorType::Rgba8,
            )
            .ok()?;
        Some(usvg::ImageKind::PNG(Arc::new(png)))
    });
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|_| "SVG parsing failed")?;
    let size = tree.size();
    let requested = match hint {
        SizeHint::Size { width, height, .. } => width.max(height) as f32,
        SizeHint::Height(h) | SizeHint::Width(h) => h as f32,
        _ => size.width().max(size.height()),
    }
    .clamp(1., 1024.);
    let mut scale = requested / size.width().max(size.height());
    // Bound overdraw/geometry work too: complex documents use a smaller thumbnail.
    let budget = (16 * PIXELS / complexity.max(1) as u64).clamp(1024, PIXELS) as f32;
    let pixels = size.width() * size.height() * scale * scale;
    if pixels > budget {
        scale *= (budget / pixels).sqrt();
    }
    let width = (size.width() * scale).ceil().max(1.) as u32;
    let height = (size.height() * scale).ceil().max(1.) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("SVG allocation failed")?;
    resvg::render(
        &tree,
        usvg::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        pixmap.data(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_absolute_relative_and_file_resources_are_never_resolved() -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("outside.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><path fill="magenta" d="M0 0h8v8H0z"/></svg>"#).map_err(|e| e.to_string())?;
        // Reading the benign outside resource would turn the otherwise transparent image magenta.
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = cwd.ancestors().last().ok_or("cwd root")?;
        let relative = format!(
            "{}{}",
            "../".repeat(
                cwd.components()
                    .filter(|c| matches!(c, std::path::Component::Normal(_)))
                    .count()
            ),
            path.strip_prefix(root)
                .map_err(|e| e.to_string())?
                .display()
        );
        assert!(std::path::Path::new(&relative).is_file());
        for href in [
            path.to_string_lossy().into_owned(),
            relative,
            format!("file://{}", path.display()),
        ] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><image href="{href}" width="8" height="8"/></svg>"#
            );
            let image = render(svg.as_bytes(), SizeHint::default())?;
            assert!(image.pixels.iter().all(|p| p.a() == 0));
        }
        Ok(())
    }
    #[test]
    fn complex_overdraw_has_a_smaller_bounded_thumbnail() -> Result<(), String> {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">{}</svg>"#,
            r#"<rect width="1000" height="1000"/>"#.repeat(4000)
        );
        let image = render(svg.as_bytes(), SizeHint::default())?;
        assert!(image.pixels.len() <= 5000);
        Ok(())
    }
    #[test]
    fn installed_loader_owns_svg_before_defaults_and_finishes_off_paint() -> Result<(), String> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        install(&ctx);
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("outside.svg");
        std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect fill="magenta" width="8" height="8"/></svg>"#).map_err(|e| e.to_string())?;
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><image href="{}" width="8" height="8"/></svg>"#,
            path.display()
        );
        ctx.include_bytes("bytes://repository/untrusted.svg", svg.into_bytes());
        assert!(matches!(
            ctx.try_load_image("bytes://repository/untrusted.svg", SizeHint::default()),
            Ok(ImagePoll::Pending { .. })
        ));
        let start = std::time::Instant::now();
        loop {
            match ctx
                .try_load_image("bytes://repository/untrusted.svg", SizeHint::default())
                .map_err(|e| e.to_string())?
            {
                ImagePoll::Ready { image } => {
                    assert!(image.pixels.iter().all(|p| p.a() == 0));
                    break;
                }
                ImagePoll::Pending { .. }
                    if start.elapsed() < std::time::Duration::from_secs(5) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => return Err("SVG worker did not finish".into()),
            }
        }
        ctx.forget_image("bytes://repository/untrusted.svg");
        Ok(())
    }
    #[test]
    fn inline_rasters_work_but_oversize_headers_and_complexity_are_bounded() -> Result<(), String> {
        use base64::Engine;
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255, 0, 255, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        let svg = |data: &[u8]| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><image href="data:image/png;base64,{}" width="8" height="8"/></svg>"#,
                base64::engine::general_purpose::STANDARD.encode(data)
            )
        };
        let image = render(svg(&png).as_bytes(), SizeHint::default())?;
        assert!(image.pixels.iter().any(|p| p.a() > 0));
        // A compressed solid raster remains small on disk, but its decoded dimensions are refused.
        png.clear();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &vec![0; 2048 * 2048 * 4],
                2048,
                2048,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        assert!(
            render(svg(&png).as_bytes(), SizeHint::default())?
                .pixels
                .iter()
                .all(|p| p.a() == 0)
        );
        assert!(render(&vec![b' '; SOURCE_LIMIT + 1], SizeHint::default()).is_err());
        assert!(
            render(
                br##"<svg xmlns="http://www.w3.org/2000/svg"><use href="#x"/></svg>"##,
                SizeHint::default()
            )
            .is_err()
        );
        Ok(())
    }
}
