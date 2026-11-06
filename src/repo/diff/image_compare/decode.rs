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

/// `bytes` decoded (PNG, JPEG, GIF, BMP, WebP), at full size, or why not.
pub fn decode(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    image::load_from_memory(bytes).map_err(|e| format!("Could not read the picture: {e}"))
}

/// The two pictures ready to compare: scaled down by one factor when the larger is past
/// [`MAX_SIDE`], so they still line up pixel for pixel.
pub fn ready(
    old: Option<image::DynamicImage>,
    new: Option<image::DynamicImage>,
) -> (Option<Decoded>, Option<Decoded>) {
    let largest = [&old, &new]
        .into_iter()
        .flatten()
        .map(|i| i.width().max(i.height()))
        .max()
        .unwrap_or(0);
    let factor = if largest > MAX_SIDE {
        f64::from(MAX_SIDE) / f64::from(largest)
    } else {
        1.0
    };
    let make = |img: image::DynamicImage| {
        let (width, height) = (img.width(), img.height());
        let scaled = factor < 1.0;
        let img = if scaled {
            let w = ((f64::from(width) * factor).round() as u32).max(1);
            let h = ((f64::from(height) * factor).round() as u32).max(1);
            img.resize_exact(w, h, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        Decoded {
            image: Arc::new(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())),
            width,
            height,
            scaled,
        }
    };
    (old.map(make), new.map(make))
}

/// How far apart two pixels may be and still count as the same (a re-saved JPEG moves some by
/// a level or two).
const SAME_WITHIN: u8 = 2;

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
            if d > SAME_WITHIN {
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
    fn pictures_decoded_and_scaled_alike() {
        let (d, none) = ready(decode(&png(3, 2, [10, 20, 30, 255])).ok(), None);
        assert!(none.is_none());
        assert_eq!(
            d.as_ref().map(|d| (d.width, d.height, d.scaled)),
            Some((3, 2, false))
        );
        assert_eq!(d.map(|d| d.image.size), Some([3, 2]));
        // One past the limit, one not: both scaled by the same factor.
        let big = decode(&png(MAX_SIDE * 2, 10, [0, 0, 0, 0])).ok();
        let small = decode(&png(MAX_SIDE, 10, [0, 0, 0, 0])).ok();
        let (a, b) = ready(big, small);
        let a = a.map(|d| (d.width, d.scaled, d.image.size[0] as u32));
        assert_eq!(a, Some((MAX_SIDE * 2, true, MAX_SIDE)));
        assert_eq!(b.map(|d| d.image.size[0] as u32), Some(MAX_SIDE / 2));
        assert!(decode(b"not a picture").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn differences_counted_and_shown() {
        let a = ColorImage::new([2, 2], vec![Color32::BLACK; 4]);
        let mut b = a.clone();
        b.pixels[3] = Color32::from_rgb(9, 0, 0);
        b.pixels[2] = Color32::from_rgb(2, 0, 0);
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
