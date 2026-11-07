//! Measure image-comparison peak memory in a process with no window or GPU initialization.
//!
//! cargo run --release --example image_memory -- generate /tmp/gitcraft-memory.png
//! /usr/bin/time -l target/release/examples/image_memory before /tmp/gitcraft-memory.png
//! /usr/bin/time -l target/release/examples/image_memory after /tmp/gitcraft-memory.png

#[allow(dead_code)]
#[path = "../src/repo/diff/image_compare/decode.rs"]
mod decode;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or("expected generate, before or after")?;
    let path = args.next().ok_or("expected a PNG path")?;
    if mode == "generate" {
        image::RgbaImage::from_pixel(4096, 4096, image::Rgba([40, 90, 160, 128])).save(path)?;
        return Ok(());
    }
    let bytes = std::fs::read(path)?;
    match mode.as_str() {
        "before" => {
            let old = image::load_from_memory(&bytes)?;
            let new = image::load_from_memory(&bytes)?;
            // The previous comparison path: conversion allocates beside the decoded pixels.
            let make = |img: image::DynamicImage| {
                let rgba = img.into_rgba8();
                egui::ColorImage::from_rgba_unmultiplied(
                    [rgba.width() as usize, rgba.height() as usize],
                    rgba.as_raw(),
                )
            };
            let pair = (make(old), make(new));
            std::hint::black_box(&pair);
            println!("Compared {} pixels per side", pair.0.pixels.len());
        }
        "after" => {
            let old = decode::decode(&bytes)?;
            let new = decode::decode(&bytes)?;
            let pair = decode::ready(Some(old), Some(new));
            std::hint::black_box(&pair);
            println!(
                "Compared {} pixels per side",
                pair.0.ok_or("no image")?.image.pixels.len()
            );
        }
        _ => return Err("expected generate, before or after".into()),
    }
    Ok(())
}
