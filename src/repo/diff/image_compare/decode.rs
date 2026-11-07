//! Pictures decoded for comparing: their pixels (scaled down past a size), and the difference
//! between two of them.

use std::sync::Arc;

use egui::{Color32, ColorImage};
use image::{GenericImageView as _, ImageDecoder as _};

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
pub fn decode(bytes: &[u8]) -> Result<Picture, String> {
    decode_picture(bytes).map_err(|e| format!("Could not read the picture: {e}"))
}

/// RGBA8 images can decode into the final, correctly aligned pixel allocation. Other colour
/// types retain their native representation until resized, as they may use less memory.
pub struct Picture(Source);

enum Source {
    Rgba(Pixels),
    Other(image::DynamicImage),
}

struct Pixels {
    size: (u32, u32),
    // Unmultiplied until `into_display`, so resampling has exactly the old colour behaviour.
    rgba: Vec<Color32>,
}

fn decode_picture(bytes: &[u8]) -> image::ImageResult<Picture> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    // Keep ImageReader::decode's allocation accounting, including its output buffer.
    let mut limits = image::Limits::default();
    limits.reserve(decoder.total_bytes())?;
    decoder.set_limits(limits)?;
    let source = if decoder.color_type() == image::ColorType::Rgba8 {
        let size = decoder.dimensions();
        let mut rgba = vec![Color32::TRANSPARENT; (decoder.total_bytes() / 4) as usize];
        decoder.read_image(bytemuck::cast_slice_mut(&mut rgba))?;
        Source::Rgba(Pixels { size, rgba })
    } else {
        Source::Other(image::DynamicImage::from_decoder(decoder)?)
    };
    Ok(Picture(source))
}

impl Picture {
    fn dimensions(&self) -> (u32, u32) {
        match &self.0 {
            Source::Rgba(pixels) => pixels.size,
            Source::Other(img) => img.dimensions(),
        }
    }

    fn into_display(self, factor: f64) -> ColorImage {
        let (width, height) = self.dimensions();
        let scaled = factor < 1.0;
        let w = ((f64::from(width) * factor).round() as u32).max(1);
        let h = ((f64::from(height) * factor).round() as u32).max(1);
        let rgba = match self.0 {
            Source::Rgba(mut pixels) if !scaled => {
                // Premultiply in place; no second full-sized buffer beside the decode.
                for pixel in &mut pixels.rgba {
                    let [r, g, b, a] = pixel.to_array();
                    *pixel = Color32::from_rgba_unmultiplied(r, g, b, a);
                }
                return ColorImage::new([width as usize, height as usize], pixels.rgba);
            }
            Source::Rgba(pixels) => {
                image::imageops::resize(&pixels, w, h, image::imageops::FilterType::Triangle)
            }
            Source::Other(img) => {
                let img = if scaled {
                    let resized = img.resize_exact(w, h, image::imageops::FilterType::Triangle);
                    drop(img);
                    resized
                } else {
                    img
                };
                img.into_rgba8()
            }
        };
        let size = [rgba.width() as usize, rgba.height() as usize];
        ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
    }
}

impl image::GenericImageView for Pixels {
    type Pixel = image::Rgba<u8>;

    fn dimensions(&self) -> (u32, u32) {
        self.size
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        image::Rgba(self.rgba[y as usize * self.size.0 as usize + x as usize].to_array())
    }
}

/// The two pictures ready to compare: scaled down by one factor when the larger is past
/// [`MAX_SIDE`], so they still line up pixel for pixel.
pub fn ready(old: Option<Picture>, new: Option<Picture>) -> (Option<Decoded>, Option<Decoded>) {
    let largest = [&old, &new]
        .into_iter()
        .flatten()
        .map(|i| {
            let (w, h) = i.dimensions();
            w.max(h)
        })
        .max()
        .unwrap_or(0);
    let factor = if largest > MAX_SIDE {
        f64::from(MAX_SIDE) / f64::from(largest)
    } else {
        1.0
    };
    let make = |img: Picture| {
        let (width, height) = img.dimensions();
        let scaled = factor < 1.0;
        Decoded {
            image: Arc::new(img.into_display(factor)),
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

    fn encode(img: &image::DynamicImage, format: image::ImageFormat) -> Result<Vec<u8>, String> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, format)
            .map_err(|e| e.to_string())?;
        Ok(bytes.into_inner())
    }

    fn expected(bytes: &[u8], factor: f64) -> Result<ColorImage, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let img = if factor < 1.0 {
            let w = ((f64::from(img.width()) * factor).round() as u32).max(1);
            let h = ((f64::from(img.height()) * factor).round() as u32).max(1);
            img.resize_exact(w, h, image::imageops::FilterType::Triangle)
        } else {
            img
        }
        .into_rgba8();
        Ok(ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        ))
    }

    #[test]
    fn rgba_decode_reuses_its_allocation_and_preserves_every_alpha() -> Result<(), String> {
        let img = image::RgbaImage::from_fn(8, 256, |x, y| {
            image::Rgba([(x * 31) as u8, y as u8, 210, y as u8])
        });
        let bytes = encode(&img.into(), image::ImageFormat::Png)?;
        let picture = decode(&bytes)?;
        let Source::Rgba(pixels) = &picture.0 else {
            return Err("RGBA8 didn't use the direct decode".to_owned());
        };
        let allocation = pixels.rgba.as_ptr();
        let actual = picture.into_display(1.0);
        assert_eq!(actual.pixels.as_ptr(), allocation);
        let expected = expected(&bytes, 1.0)?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn resampling_preserves_the_original_unmultiplied_colours() -> Result<(), String> {
        let img = image::RgbaImage::from_fn(137, 83, |x, y| {
            image::Rgba([(x * 17) as u8, (y * 31) as u8, 90, (x * y) as u8])
        });
        let bytes = encode(&img.into(), image::ImageFormat::Png)?;
        for factor in [0.5, 0.031] {
            assert_eq!(
                decode(&bytes)?.into_display(factor),
                expected(&bytes, factor)?
            );
        }
        Ok(())
    }

    #[test]
    fn other_formats_and_colour_depths_keep_the_same_pixels() -> Result<(), String> {
        let rgba = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(19, 11, |x, y| {
            image::Rgba([(x * 31) as u8, (y * 17) as u8, 90, (x * y) as u8])
        }));
        let rgba16 = image::ImageBuffer::from_fn(19, 11, |x, y| {
            image::Rgba([
                (x * 3171) as u16,
                (y * 1729) as u16,
                913,
                (x * y * 71) as u16,
            ])
        });
        let variants = [
            rgba.clone(),
            rgba.to_rgb8().into(),
            rgba.to_luma8().into(),
            rgba.to_luma_alpha8().into(),
            image::DynamicImage::ImageRgba16(rgba16),
        ];
        for img in variants {
            let bytes = encode(&img, image::ImageFormat::Png)?;
            for factor in [1.0, 0.5] {
                assert_eq!(
                    decode(&bytes)?.into_display(factor),
                    expected(&bytes, factor)?
                );
            }
        }
        for format in [
            image::ImageFormat::Bmp,
            image::ImageFormat::WebP,
            image::ImageFormat::Gif,
            image::ImageFormat::Jpeg,
        ] {
            let img = if format == image::ImageFormat::Jpeg {
                rgba.to_rgb8().into()
            } else {
                rgba.clone()
            };
            let bytes = encode(&img, format)?;
            for factor in [1.0, 0.5] {
                assert_eq!(
                    decode(&bytes)?.into_display(factor),
                    expected(&bytes, factor)?
                );
            }
        }
        Ok(())
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
