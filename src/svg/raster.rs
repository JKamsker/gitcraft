//! Bound SVG complexity before parsing and rasterize with external resources disabled.
use super::*;

pub(super) fn render(bytes: &[u8], hint: SizeHint) -> Result<egui::ColorImage, String> {
    render_with_parser(bytes, hint, |bytes, options| {
        usvg::Tree::from_data(bytes, options).map_err(|_| "SVG parsing failed".into())
    })
}

// The parser boundary is injectable so refusals can be proved without ever expanding
// hostile paint graphs. Active parsing/rasterization is bounded work, not preemptible.
pub(super) fn render_with_parser(
    bytes: &[u8],
    hint: SizeHint,
    parse: impl FnOnce(&[u8], &usvg::Options<'_>) -> Result<usvg::Tree, String>,
) -> Result<egui::ColorImage, String> {
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
            // Markers and clip paths expand recursively during parsing or painting,
            // including references from inline styles or escaped CSS properties.
            // Refuse definitions irrespective of reference syntax before CSS/usvg.
            "use" | "marker" | "clipPath" | "filter" | "mask" | "pattern" | "foreignObject"
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
    let tree = parse(bytes, &options)?;
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
