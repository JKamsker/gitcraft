//! CPU rendering of the same egui composition shown in the preview. Uses the bundled fonts, no system font discovery.
use super::Snapshot;
use egui::{Color32, FontId, Pos2, Rect, Stroke, pos2, vec2};
use std::collections::HashMap;
use std::io::Cursor;
const WIDTH: usize = 1280;
const PITCH: f32 = 27.0;
const BG: Color32 = Color32::from_rgb(21, 27, 39);
const TEXT: Color32 = Color32::from_rgb(239, 242, 249);
const DIM: Color32 = Color32::from_rgb(179, 192, 214);
const COLOURS: [Color32; 6] = [
    Color32::from_rgb(116, 185, 255),
    Color32::from_rgb(159, 222, 152),
    Color32::from_rgb(255, 191, 105),
    Color32::from_rgb(206, 164, 255),
    Color32::from_rgb(255, 147, 174),
    Color32::from_rgb(100, 218, 218),
];
pub(super) fn render(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
) -> Result<(egui::ColorImage, Vec<u8>), String> {
    if snapshot.commits.is_empty()
        || snapshot.commits.len() > crate::git::investigations::postcards::LIMIT
    {
        return Err("Invalid postcard size".into());
    }
    let graph = crate::graph::layout(&snapshot.commits);
    let contributors = snapshot.contributors.join(" · ");
    let footer_lines = contributors.chars().count().div_ceil(70).max(1);
    let height =
        (215.0 + snapshot.commits.len() as f32 * PITCH + footer_lines as f32 * 24.0) as usize;
    if height > 12000 {
        return Err("Postcard composition exceeds 12000 pixels; omit contributor names or choose a narrower range".into());
    }
    let size = vec2(WIDTH as f32, height as f32);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        },
        |ui| {
            let painter = ui.painter().clone();
            painter.rect_filled(Rect::from_min_size(Pos2::ZERO, size), 0.0, BG);
            painter.rect_filled(
                Rect::from_min_size(pos2(34.0, 28.0), vec2(5.0, 62.0)),
                2.0,
                COLOURS[0],
            );
            text(
                &painter,
                pos2(54.0, 28.0),
                &snapshot.name,
                32.0,
                TEXT,
                1170.0,
            );
            text(
                &painter,
                pos2(54.0, 75.0),
                &format!("{} · {} commits", snapshot.date, snapshot.commits.len()),
                17.0,
                DIM,
                1170.0,
            );
            text(
                &painter,
                pos2(40.0, 111.0),
                &snapshot.range,
                14.0,
                DIM,
                1200.0,
            );
            let graph_width = (graph.width.clamp(1, 24) as f32 * 13.0 + 20.0).min(335.0);
            for (i, commit) in snapshot.commits.iter().enumerate() {
                let y = 152.0 + i as f32 * PITCH;
                let row = &graph.rows[i];
                let x = |col: usize| 40.0 + col as f32 * 13.0;
                let clip = painter.with_clip_rect(Rect::from_min_size(
                    pos2(33.0, y - PITCH / 2.0),
                    vec2(graph_width, PITCH),
                ));
                for &lane in &row.through {
                    clip.line_segment(
                        [
                            pos2(x(lane), y - PITCH / 2.0),
                            pos2(x(lane), y + PITCH / 2.0),
                        ],
                        Stroke::new(1.8, COLOURS[lane % COLOURS.len()]),
                    );
                }
                for (&lane, dy) in row
                    .incoming
                    .iter()
                    .map(|l| (l, -PITCH / 2.0))
                    .chain(row.outgoing.iter().map(|l| (l, PITCH / 2.0)))
                {
                    clip.line_segment(
                        [pos2(x(row.col), y), pos2(x(lane), y + dy)],
                        Stroke::new(1.8, COLOURS[lane % COLOURS.len()]),
                    );
                }
                clip.circle_filled(
                    pos2(x(row.col), y),
                    if commit.parents.len() > 1 { 3.2 } else { 4.5 },
                    COLOURS[row.col % COLOURS.len()],
                );
                text(
                    &painter,
                    pos2(50.0 + graph_width, y - 10.0),
                    &snapshot.messages[i],
                    16.0,
                    TEXT,
                    1160.0 - graph_width,
                );
            }
            let footer = 165.0 + snapshot.commits.len() as f32 * PITCH;
            painter.line_segment(
                [pos2(40.0, footer), pos2(1240.0, footer)],
                Stroke::new(1.0, DIM.gamma_multiply(0.35)),
            );
            if !contributors.is_empty() {
                wrapped_text(
                    &painter,
                    pos2(40.0, footer + 14.0),
                    &format!("Contributors: {contributors}"),
                    15.0,
                    DIM,
                    1200.0,
                );
            }
            text(
                &painter,
                pos2(40.0, height as f32 - 24.0),
                if snapshot.anonymized {
                    "Gitcraft · names and commit messages anonymized · immutable range IDs retained"
                } else {
                    "Gitcraft · release history postcard"
                },
                11.0,
                DIM,
                1200.0,
            );
        },
    );
    if !active() {
        output.textures_delta.clear();
        return Err("Postcard rendering cancelled".into());
    }
    let mut textures: HashMap<egui::TextureId, std::sync::Arc<egui::ColorImage>> = HashMap::new();
    let changes = std::mem::take(&mut output.textures_delta.set);
    output.textures_delta.clear();
    for (id, deltas) in changes {
        for delta in deltas {
            let egui::ImageData::Color(image) = delta.image;
            if let Some([left, top]) = delta.pos {
                let texture = std::sync::Arc::make_mut(
                    textures.get_mut(&id).ok_or("Missing base font atlas")?,
                );
                if left + image.size[0] > texture.size[0] || top + image.size[1] > texture.size[1] {
                    return Err("Invalid font atlas patch".into());
                }
                for y in 0..image.size[1] {
                    let start = (top + y) * texture.size[0] + left;
                    texture.pixels[start..start + image.size[0]]
                        .copy_from_slice(&image.pixels[y * image.size[0]..(y + 1) * image.size[0]]);
                }
            } else {
                textures.insert(id, image);
            }
        }
    }
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let mut image = egui::ColorImage::filled([WIDTH, height], BG);
    for primitive in primitives {
        if !active() {
            return Err("Postcard rendering cancelled".into());
        }
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            return Err("Unsupported postcard paint callback".into());
        };
        let texture = textures
            .get(&mesh.texture_id)
            .ok_or("Missing postcard font texture")?;
        for (index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
            if index % 128 == 0 && !active() {
                return Err("Postcard rendering cancelled".into());
            }
            let vertices = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            raster(&mut image, texture, primitive.clip_rect, vertices);
        }
    }
    let mut rgba = Vec::with_capacity(WIDTH * height * 4);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let pixels = image::RgbaImage::from_raw(WIDTH as u32, height as u32, rgba)
        .ok_or("Invalid postcard pixels")?;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    if !active() {
        return Err("Postcard rendering cancelled".into());
    }
    Ok((image, png.into_inner()))
}
fn text(p: &egui::Painter, at: Pos2, value: &str, size: f32, color: Color32, width: f32) {
    let galley = p.layout_no_wrap(value.to_owned(), FontId::proportional(size), color);
    p.with_clip_rect(Rect::from_min_size(at, vec2(width, size * 1.8)))
        .galley(at, galley, color);
}
fn wrapped_text(p: &egui::Painter, at: Pos2, value: &str, size: f32, color: Color32, width: f32) {
    let galley = p.layout(value.to_owned(), FontId::proportional(size), color, width);
    p.galley(at, galley, color);
}
fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
fn top_left(a: Pos2, b: Pos2) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}
fn raster(
    image: &mut egui::ColorImage,
    texture: &egui::ColorImage,
    clip: Rect,
    mut v: [egui::epaint::Vertex; 3],
) {
    let mut area = edge(v[0].pos, v[1].pos, v[2].pos);
    if area.abs() < 0.00001 {
        return;
    }
    if area < 0.0 {
        v.swap(1, 2);
        area = -area;
    }
    let bounds = Rect::from_points(&v.map(|v| v.pos))
        .intersect(clip)
        .intersect(Rect::from_min_size(
            Pos2::ZERO,
            vec2(image.size[0] as f32, image.size[1] as f32),
        ));
    for y in
        bounds.top().floor().max(0.0) as usize..(bounds.bottom().ceil() as usize).min(image.size[1])
    {
        for x in bounds.left().floor().max(0.0) as usize
            ..(bounds.right().ceil() as usize).min(image.size[0])
        {
            let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
            let e = [
                edge(v[1].pos, v[2].pos, p),
                edge(v[2].pos, v[0].pos, p),
                edge(v[0].pos, v[1].pos, p),
            ];
            if e.iter().enumerate().any(|(i, &e)| {
                e < 0.0 || (e == 0.0 && !top_left(v[(i + 1) % 3].pos, v[(i + 2) % 3].pos))
            }) {
                continue;
            }
            let w = e.map(|e| e / area);
            let u = (0..3).map(|i| w[i] * v[i].uv.x).sum::<f32>();
            let vv = (0..3).map(|i| w[i] * v[i].uv.y).sum::<f32>();
            let tx = ((u * texture.size[0] as f32) as usize).min(texture.size[0] - 1);
            let ty = ((vv * texture.size[1] as f32) as usize).min(texture.size[1] - 1);
            let tex = texture.pixels[ty * texture.size[0] + tx].to_array();
            let mut src = [0.0; 4];
            for (channel, channel_value) in src.iter_mut().enumerate() {
                *channel_value = (0..3)
                    .map(|i| w[i] * f32::from(v[i].color[channel]))
                    .sum::<f32>()
                    * f32::from(tex[channel])
                    / 255.0;
            }
            let dst = image.pixels[y * image.size[0] + x].to_array();
            let blend = |c: usize| {
                (src[c] + f32::from(dst[c]) * (1.0 - src[3] / 255.0)).clamp(0.0, 255.0) as u8
            };
            image.pixels[y * image.size[0] + x] =
                Color32::from_rgba_premultiplied(blend(0), blend(1), blend(2), blend(3));
        }
    }
}
