//! Pictures decoded for comparing: their pixels (scaled down past a size), and the difference
//! between two of them.

use std::sync::Arc;

use egui::{Color32, ColorImage};

/// The longest side shown at full detail; a larger picture is scaled down to it.
pub const MAX_SIDE: u32 = 4096;

/// A decoded picture.
pub struct Decoded {
    pub image: Arc<ColorImage>,
    /// Its own size, before any scaling.
    pub width: u32,
    pub height: u32,
    /// It was scaled down to be shown.
    pub scaled: bool,
}

/// `bytes` decoded (PNG, JPEG, GIF, BMP, WebP), or why not.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let img =
        image::load_from_memory(bytes).map_err(|e| format!("Could not read the picture: {e}"))?;
    let (width, height) = (img.width(), img.height());
    let scaled = width.max(height) > MAX_SIDE;
    let img = if scaled {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(Decoded {
        image: Arc::new(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())),
        width,
        height,
        scaled,
    })
}

/// How two pictures differ: a picture of the difference (each pixel how far the two are
/// apart, brightened so small changes show; transparent where they are the same), on a canvas
/// as large as both, top-left aligned, and the share of pixels that changed.
pub struct Difference {
    pub image: Arc<ColorImage>,
    pub changed: usize,
    pub total: usize,
}

pub fn difference(a: &ColorImage, b: &ColorImage) -> Difference {
    let w = a.size[0].max(b.size[0]);
    let h = a.size[1].max(b.size[1]);
    let at = |img: &ColorImage, x: usize, y: usize| {
        (x < img.size[0] && y < img.size[1]).then(|| img.pixels[y * img.size[0] + x])
    };
    let mut pixels = Vec::with_capacity(w * h);
    let mut changed = 0;
    for y in 0..h {
        for x in 0..w {
            let d = match (at(a, x, y), at(b, x, y)) {
                (Some(p), Some(q)) => distance(p, q),
                // Only on one side: as different as can be.
                _ => 255,
            };
            if d > 0 {
                changed += 1;
                // Brightened: a change of a few levels still shows.
                let v = (u32::from(d) * 4).min(255) as u8;
                pixels.push(Color32::from_rgba_unmultiplied(255, 40, 120, v.max(64)));
            } else {
                pixels.push(Color32::TRANSPARENT);
            }
        }
    }
    Difference {
        image: Arc::new(ColorImage::new([w, h], pixels)),
        changed,
        total: w * h,
    }
}

/// The largest difference of a channel (alpha included) between two pixels.
fn distance(p: Color32, q: Color32) -> u8 {
    let (p, q) = (p.to_srgba_unmultiplied(), q.to_srgba_unmultiplied());
    (0..4).map(|i| p[i].abs_diff(q[i])).max().unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG of `w` × `h` pixels of one colour.
    pub(crate) fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba(rgba));
        let mut out = std::io::Cursor::new(Vec::new());
        let _ = img.write_to(&mut out, image::ImageFormat::Png);
        out.into_inner()
    }

    #[test]
    fn pictures_decoded_and_scaled() {
        let d = decode(&png(3, 2, [10, 20, 30, 255])).ok();
        assert_eq!(
            d.as_ref().map(|d| (d.width, d.height, d.scaled)),
            Some((3, 2, false))
        );
        assert_eq!(d.map(|d| d.image.size), Some([3, 2]));
        let big = decode(&png(MAX_SIDE + 4, 10, [0, 0, 0, 0])).ok();
        assert_eq!(
            big.as_ref().map(|d| (d.width, d.scaled)),
            Some((MAX_SIDE + 4, true))
        );
        assert_eq!(big.map(|d| d.image.size[0] as u32), Some(MAX_SIDE));
        assert!(decode(b"not a picture").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn differences_counted_and_shown() {
        let a = ColorImage::new([2, 2], vec![Color32::BLACK; 4]);
        let mut b = a.clone();
        b.pixels[3] = Color32::from_rgb(1, 0, 0);
        let d = difference(&a, &b);
        assert_eq!((d.changed, d.total), (1, 4));
        assert_eq!(d.image.pixels[0], Color32::TRANSPARENT);
        assert_ne!(d.image.pixels[3], Color32::TRANSPARENT);
        // Of another size: the part only one has is all changed.
        let wide = ColorImage::new([3, 2], vec![Color32::BLACK; 6]);
        let d = difference(&a, &wide);
        assert_eq!((d.image.size, d.changed), ([3, 2], 2));
        assert_eq!(difference(&a, &a).changed, 0);
    }
}
