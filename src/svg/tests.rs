use super::*;
#[test]
fn nested_clip_paths_are_refused_before_parser_for_attributes_and_css() -> Result<(), String> {
    for depth in [1, 4, 8] {
        for route in ["attribute", "inline", "css", "escaped-css"] {
            let mut definitions = String::new();
            let mut styles = String::new();
            for i in 0..depth {
                let child = if i + 1 < depth {
                    let target = i + 1;
                    let reference = match route {
                        "attribute" => format!("clip-path='url(#c{target})'"),
                        "inline" => format!("style='clip-path:url(#c{target})'"),
                        _ => {
                            let property = if route == "css" {
                                "clip-path"
                            } else {
                                r"cl\69 p-path"
                            };
                            styles.push_str(&format!(".p{i} {{{property}:url(#c{target})}}"));
                            format!("class='p{i}'")
                        }
                    };
                    // Each level visits the next clip twice, despite linear XML size.
                    format!("<rect width='8' height='8' {reference}/>").repeat(2)
                } else {
                    "<rect width='8' height='8'/>".into()
                };
                definitions.push_str(&format!("<clipPath id='c{i}'>{child}</clipPath>"));
            }
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><style>{styles}</style><defs>{definitions}</defs><rect width='8' height='8' clip-path='url(#c0)'/></svg>"
            );
            assert!(svg.len() < 4096);
            let mut parser_entered = false;
            let error = render_with_parser(svg.as_bytes(), SizeHint::default(), |_, _| {
                parser_entered = true;
                Err("Parser must not receive recursive clipping".into())
            })
            .err()
            .ok_or("Clip graph unexpectedly accepted")?;
            assert!(error.contains("expansion/effect") && error.contains("source view"));
            assert!(!parser_entered, "depth={depth}, route={route}");
        }
    }
    Ok(())
}
#[test]
fn nested_distinct_markers_are_refused_before_parser_for_attributes_and_css() -> Result<(), String>
{
    for depth in [1, 4, 8] {
        for route in ["attribute", "inline", "css", "escaped-css"] {
            let mut definitions = String::new();
            let mut styles = String::new();
            for i in 0..depth {
                let target = if i + 1 < depth { i + 1 } else { i };
                let reference = match route {
                    "attribute" => format!("marker-mid='url(#m{target})'"),
                    "inline" => format!("style='marker-mid:url(#m{target})'"),
                    _ => {
                        let property = if route == "css" {
                            "marker-mid"
                        } else {
                            r"m\61 rker-mid"
                        };
                        styles.push_str(&format!(".p{i} {{{property}:url(#m{target})}}"));
                        format!("class='p{i}'")
                    }
                };
                let child = if i + 1 < depth {
                    format!("<path d='M0 0 L1 1 L2 0 L3 1' {reference}/>")
                } else {
                    "<circle r='1'/>".into()
                };
                definitions.push_str(&format!(
                    "<marker id='m{i}' markerWidth='4' markerHeight='4'>{child}</marker>"
                ));
            }
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><style>{styles}</style><defs>{definitions}</defs><path d='M0 0L1 1L2 0L3 1' marker-mid='url(#m0)'/></svg>"
            );
            assert!(svg.len() < 4096);
            let mut parser_entered = false;
            let result = render_with_parser(svg.as_bytes(), SizeHint::default(), |_, _| {
                parser_entered = true;
                Err("Parser must never receive expansion input".into())
            });
            let error = result.err().ok_or("Marker preview unexpectedly accepted")?;
            assert!(error.contains("expansion/effect") && error.contains("source view"));
            assert!(!parser_entered, "depth={depth}, route={route}");
        }
    }
    Ok(())
}
#[test]
fn preflight_refusal_precedes_parser_allocation_and_ordinary_style_still_renders()
-> Result<(), String> {
    let over_budget = format!(
        "<svg xmlns='http://www.w3.org/2000/svg'>{}</svg>",
        "<g/>".repeat(4096)
    );
    let mut parser_entered = false;
    let error = render_with_parser(over_budget.as_bytes(), SizeHint::default(), |_, _| {
        parser_entered = true;
        Err("Unexpected parser allocation".into())
    })
    .err()
    .ok_or("Injected source budget should refuse")?;
    assert!(!parser_entered);
    assert!(error.contains("complexity") && error.contains("source view"));
    let ordinary = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><style>.paint {fill:magenta}</style><rect class="paint" width="8" height="8" style="opacity:1"/></svg>"#;
    let image = render(ordinary, SizeHint::default())?;
    assert!(image.pixels.iter().any(|p| p.a() > 0));
    Ok(())
}
#[test]
fn external_absolute_relative_and_file_resources_are_never_resolved() -> Result<(), String> {
    // Keep the outside resource on the CWD volume so the relative positive
    // control also works when Windows puts its temp directory on another drive.
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let temp = tempfile::tempdir_in(&cwd).map_err(|e| e.to_string())?;
    let path = temp.path().join("outside.svg");
    std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><path fill="magenta" d="M0 0h8v8H0z"/></svg>"#).map_err(|e| e.to_string())?;
    // Reading the benign outside resource would turn the otherwise transparent image magenta.
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
        relative.replace('\\', "/"),
        url::Url::from_file_path(&path)
            .map_err(|_| "Invalid file URL")?
            .into(),
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
            ImagePoll::Pending { .. } if start.elapsed() < std::time::Duration::from_secs(5) => {
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
