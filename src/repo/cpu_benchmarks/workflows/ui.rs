use super::*;

fn frame(ctx: &egui::Context, time: &mut u64, mut draw: impl FnMut(&mut egui::Ui)) {
    *time += 1;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 800.0),
            )),
            time: Some(*time as f64 / 60.0),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| draw(ui));
        },
    );
    black_box(ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point));
    output.drop_without_applying_deltas();
}

pub(super) fn run(bench: &mut Bench, inputs: &Inputs) -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    egui_extras::install_image_loaders(&ctx);
    let mut settings = Settings {
        avatars: false,
        offline: true,
        ..Default::default()
    };
    settings.put_for_test(&ctx);
    let mut tab = RepoTab::new_deferred(inputs.repo.clone(), &ctx);
    tab.refs = git::refs(&inputs.repo)?;
    tab.loading = false;
    tab.has_more = false;
    let mut time = 0;
    bench.measure("startup/create-20-deferred-tabs", || {
        for _ in 0..20 {
            black_box(RepoTab::new_deferred(inputs.repo.clone(), &ctx));
        }
    });
    bench.measure("settings/serialize", || {
        assert!(black_box(ron::ser::to_string(&settings)).is_ok());
    });
    bench.measure("ui/palette-items", || {
        black_box(tab.palette_items());
    });
    for _ in 0..20 {
        frame(&ctx, &mut time, |ui| tab.sidebar(ui, &mut Vec::new()));
    }
    bench.measure("ui/sidebar-frame", || {
        frame(&ctx, &mut time, |ui| tab.sidebar(ui, &mut Vec::new()))
    });

    let patch = inputs.diffs.first().ok_or("missing diff fixture")?;
    for split in [false, true] {
        settings.diff_split = split;
        settings.put_for_test(&ctx);
        let key = DiffKey::Commit {
            hash: patch.revision.clone(),
            path: patch.path.clone(),
        };
        let mut view = crate::repo::DiffView::new(key.clone());
        view.data = Some(Ok(diff::load(
            &inputs.repo,
            &key,
            git::DiffOptions::default(),
            true,
        )?));
        view.unfolded = true;
        view.algorithm = diff::algorithm_for(&settings, key.path());
        tab.diff = Some(view);
        for _ in 0..20 {
            frame(&ctx, &mut time, |ui| tab.diff_view(ui, &mut Vec::new()));
        }
        let mode = if split { "split" } else { "unified" };
        bench.measure(&format!("ui/diff-{mode}-frame"), || {
            frame(&ctx, &mut time, |ui| tab.diff_view(ui, &mut Vec::new()))
        });
        let mut scroll = false;
        bench.measure(&format!("ui/diff-{mode}-scroll-frame"), || {
            scroll = !scroll;
            if let Some(view) = &mut tab.diff {
                view.scroll_to = Some(if scroll { 475.0 } else { 0.0 });
            }
            frame(&ctx, &mut time, |ui| tab.diff_view(ui, &mut Vec::new()));
        });
    }
    tab.diff = None;
    use crate::repo::file_view::{FileTab, FileView};
    for (name, which) in [
        ("whole-file", FileTab::File),
        ("file-history", FileTab::History),
        ("blame", FileTab::Blame),
    ] {
        let mut view = FileView::new("src/repo/load.rs", Some(inputs.revision.clone()), which);
        match which {
            FileTab::File => {
                view.text = Some(crate::repo::file_text::load(
                    &inputs.repo,
                    Some(&inputs.revision),
                    &view.path,
                    true,
                ))
            }
            FileTab::History => {
                view.history = Some(git::file_history(
                    &inputs.repo,
                    Some(&inputs.revision),
                    &view.path,
                ))
            }
            FileTab::Blame => {
                view.blame = Some(crate::repo::file_view::load_blame(
                    &inputs.repo,
                    Some(&inputs.revision),
                    &view.path,
                    true,
                ))
            }
        }
        assert!(view.text.as_ref().is_none_or(Result::is_ok));
        assert!(view.history.as_ref().is_none_or(Result::is_ok));
        assert!(view.blame.as_ref().is_none_or(Result::is_ok));
        tab.file_view = Some(view);
        for _ in 0..20 {
            frame(&ctx, &mut time, |ui| tab.file_view(ui, &mut Vec::new()));
        }
        bench.measure(&format!("ui/{name}-frame"), || {
            frame(&ctx, &mut time, |ui| tab.file_view(ui, &mut Vec::new()))
        });
    }
    terminal(bench)
}

fn terminal(bench: &mut Bench) -> Result<(), String> {
    let rows = std::num::NonZeroU16::new(24).ok_or("terminal rows")?;
    let cols = std::num::NonZeroU16::new(120).ok_or("terminal columns")?;
    let transcript: String = (0..2000)
        .map(|i| {
            format!(
                "\x1b[32mCompiling\x1b[0m package_{i} v1.2.3 日本語\r\n\rprogress {i}\x1b[K\r\n"
            )
        })
        .collect();
    bench.measure("terminal/parse-build-output", || {
        let mut parser = vt100::Parser::new(rows, cols, 5000);
        for chunk in transcript.as_bytes().chunks(16 * 1024) {
            parser.process(chunk);
        }
        assert!(parser.screen().contents().contains("package_1999"));
        black_box(parser);
    });
    let mut parser = vt100::Parser::new(rows, cols, 5000);
    parser.process(transcript.as_bytes());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut time = 0;
    for _ in 0..20 {
        frame(&ctx, &mut time, |ui| {
            crate::terminal::cpu_paint(ui, parser.screen())
        });
    }
    bench.measure("terminal/paint-build-output-frame", || {
        frame(&ctx, &mut time, |ui| {
            crate::terminal::cpu_paint(ui, parser.screen())
        });
    });
    Ok(())
}
