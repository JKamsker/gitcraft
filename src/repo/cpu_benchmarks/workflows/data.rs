use super::*;
use crate::codeowners::{CodeOwners, Flavor};

pub(super) fn run(bench: &mut Bench, inputs: &Inputs) -> Result<(), String> {
    if bench.family(&["owners/"]) {
        owners(bench);
    }
    if bench.family(&["find/"]) {
        search(bench);
    }
    if bench.family(&["highlight/"]) {
        syntax(bench);
    }
    if bench.family(&["images/"]) {
        images(bench)?;
    }
    if bench.family(&["rich/"]) {
        rich(bench, inputs)?;
    }
    let patch_input = inputs.diffs.first().ok_or("missing analysis patch")?;
    let patch = git::file_diff(
        &inputs.repo,
        &patch_input.revision,
        &patch_input.path,
        git::DiffOptions::default(),
    )?;
    assert!(!patch.is_empty());
    bench.measure("analysis/weakened-checks", || {
        black_box(crate::weakened::scan_patch(&patch));
    });
    let a: Vec<_> = (0..2000)
        .map(|i| format!("line {i} with useful source code"))
        .collect();
    for (case, stride) in [("sparse", 100), ("rewrite", 2)] {
        let b: Vec<_> = a
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i % stride == 0 {
                    format!("changed {i}")
                } else {
                    l.clone()
                }
            })
            .collect();
        let old: Vec<_> = a.iter().map(String::as_str).collect();
        let new: Vec<_> = b.iter().map(String::as_str).collect();
        bench.measure(&format!("text/myers-{case}"), || {
            black_box(diff::line_diff::diff_lines(&old, &new));
        });
    }
    let short = "let name = compute_value(input);";
    bench.measure("text/cap-50000-source-lines", || {
        for _ in 0..50_000 {
            let mut line = black_box(short.to_owned());
            black_box(highlight::cap_line(&mut line, None));
        }
    });
    Ok(())
}

fn owners(bench: &mut Bench) {
    let files: Vec<_> = (0..1000)
        .map(|i| format!("src/package{}/file{i}.rs", i % 20))
        .collect();
    for (name, flavor) in [
        ("github", Flavor::GitHub),
        ("gitlab", Flavor::GitLab),
        ("gitea", Flavor::Gitea),
    ] {
        let mut text = String::new();
        for section in 0..20 {
            if flavor == Flavor::GitLab {
                text.push_str(&format!("[Team{section}]\n"));
            }
            for rule in 0..20 {
                let pattern = if flavor == Flavor::Gitea {
                    format!("src/package{rule}/.*")
                } else {
                    format!("/src/package{rule}/")
                };
                text.push_str(&format!("{pattern} @team{section}\n"));
            }
        }
        bench.measure(&format!("owners/{name}-parse-400-rules"), || {
            black_box(CodeOwners::parse("CODEOWNERS", flavor, &text, false));
        });
        let parsed = CodeOwners::parse("CODEOWNERS", flavor, &text, false);
        assert!(!parsed.owners(&files[0]).is_empty());
        bench.measure(&format!("owners/{name}-lookup-1000-files"), || {
            for file in &files {
                black_box(parsed.owners(file));
            }
        });
        bench.measure(&format!("owners/{name}-lint-1000-files"), || {
            black_box(parsed.lint(&files));
        });
    }
}

fn search(bench: &mut Bench) {
    use crate::repo::find::{Find, Layout, Query, Source, SourceKey};
    let lines: Vec<_> = (0..50_000)
        .map(|i| {
            if i % 10 == 0 {
                format!("let needle_{i} = compute_value(input); // 日本語 é")
            } else {
                format!("let value_{i} = compute_value(input); // useful code")
            }
        })
        .collect();
    let ctx = egui::Context::default();
    for (name, query) in [
        (
            "literal",
            Query {
                text: "needle".into(),
                ..Default::default()
            },
        ),
        (
            "regex",
            Query {
                text: "needle_[0-9]+".into(),
                regex: true,
                ..Default::default()
            },
        ),
        (
            "unicode",
            Query {
                text: "日本語".into(),
                ..Default::default()
            },
        ),
    ] {
        bench.measure(&format!("find/{name}-50000-lines"), || {
            let mut find = Find::default();
            find.query = query.clone();
            find.show();
            let key = SourceKey {
                what: Source::File {
                    path: "source.rs".into(),
                    rev: None,
                },
                at: lines.as_ptr() as usize,
                len: lines.len(),
            };
            for _ in 0..10_000 {
                find.update(
                    &ctx,
                    key.clone(),
                    lines.len(),
                    |i| lines.get(i).map(String::as_str),
                    Layout::Lines,
                );
                if find.summary().is_some_and(|(s, _)| !s.contains('…')) {
                    break;
                }
            }
            assert_eq!(find.count(), 5000);
            black_box(find);
        });
    }
}

fn syntax(bench: &mut Bench) {
    for (path, block) in [
        (
            "main.py",
            "# Example\nclass Item:\n    def value(self, x: int) -> str:\n        return f'value {x}'\n",
        ),
        (
            "App.tsx",
            "// Example\nexport const App = ({count}: {count: number}) => <div>{count + 1}</div>;\n",
        ),
        (
            "App.vue",
            "<template><div>{{ count }}</div></template>\n<script setup lang=\"ts\">\nconst count: number = 1;\n</script>\n",
        ),
        (
            "Main.cs",
            "// Example\npublic class Item {\n public string Name { get; set; } = \"example\";\n}\n",
        ),
        (
            "main.cpp",
            "// Example\n#include <string>\nstd::string value(int n) { return std::to_string(n + 1); }\n",
        ),
        (
            "main.go",
            "// Example\npackage main\nfunc value(n int) int { return n + 1 }\n",
        ),
        (
            "main.java",
            "// Example\nclass Item { public int value(int n) { return n + 1; } }\n",
        ),
        (
            "script.ps1",
            "# Example\nfunction Get-Value { param([int]$n) Write-Output ($n + 1) }\n",
        ),
    ] {
        let source = block.repeat(150);
        let lines: Vec<_> = source.lines().collect();
        assert!(highlight::highlight(path, lines.iter().copied(), true).is_some());
        bench.measure(&format!("highlight/{path}"), || {
            black_box(highlight::highlight(path, lines.iter().copied(), true));
        });
    }
}

fn images(bench: &mut Bench) -> Result<(), String> {
    use egui::{Color32, ColorImage};
    use image::{ImageEncoder as _, codecs::png::PngEncoder};
    let rgba: Vec<_> = (0..1920 * 1080)
        .flat_map(|i| [(i % 251) as u8, 90, 120, 128])
        .collect();
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&rgba, 1920, 1080, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    bench.try_measure("images/decode-png-1080p", || diff::cpu_images::decode(&png));
    bench.try_measure("images/decode-fit-pair-1080p", || {
        let old = diff::cpu_images::decode(&png)?;
        let new = diff::cpu_images::decode(&png)?;
        Ok(diff::cpu_images::ready(Some(old), Some(new)))
    });
    let old = ColorImage::from_rgba_unmultiplied([1920, 1080], &rgba);
    for (name, stride) in [
        ("identical", usize::MAX),
        ("one-percent-edited", 100),
        ("fully-edited", 1),
    ] {
        let mut new = old.clone();
        if stride != usize::MAX {
            for i in (0..new.pixels.len()).step_by(stride) {
                new.pixels[i] = Color32::from_rgba_unmultiplied(180, 100, 50, 150);
            }
        }
        let expected = if stride == usize::MAX {
            0
        } else {
            new.pixels.len().div_ceil(stride)
        };
        assert_eq!(diff::cpu_images::difference(&old, &new).changed, expected);
        bench.measure(&format!("images/difference-{name}-1080p"), || {
            black_box(diff::cpu_images::difference(&old, &new));
        });
    }
    let smaller = ColorImage::filled([1280, 720], Color32::WHITE);
    bench.measure("images/difference-resized", || {
        black_box(diff::cpu_images::difference(&old, &smaller));
    });
    Ok(())
}

fn rich(bench: &mut Bench, inputs: &Inputs) -> Result<(), String> {
    use diff::rich::{Rich, RichKind};
    let repo = git::fixture::Repo::new("cpu-rich")?;
    let hooks = repo.dir.join(".git/no-hooks");
    std::fs::create_dir_all(&hooks).map_err(|e| e.to_string())?;
    repo.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    let old_json = serde_json::to_string(
        &(0..5000)
            .map(|i| (format!("key_{i}"), i))
            .collect::<std::collections::BTreeMap<_, _>>(),
    )
    .map_err(|e| e.to_string())?;
    let lock = String::from_utf8(git::file_at(
        &inputs.repo,
        Some(&inputs.revision),
        "Cargo.lock",
    )?)
    .map_err(|e| e.to_string())?;
    let csv: String = std::iter::once("id,name,value\n".to_owned())
        .chain((0..2000).map(|i| format!("{i},item{i},{i}\n")))
        .collect();
    let markdown = "# Guide\n\nA useful paragraph explaining the current implementation and how to use this feature.\n\n".repeat(80);
    let api = r#"{"openapi":"3.0.0","info":{"title":"Test","version":"1"},"paths":{"/items":{"get":{"responses":{"200":{"description":"OK"}}}}}}"#;
    let cases = [
        (
            "settings.json",
            old_json,
            "{\"key_0\":0}".to_owned(),
            RichKind::Config,
        ),
        (
            "settings.yaml",
            "server:\n  host: localhost\n  port: 80\n".into(),
            "server:\n  host: localhost\n  port: 8080\n".into(),
            RichKind::Config,
        ),
        (
            "settings.toml",
            "[server]\nhost = \"localhost\"\nport = 80\n".into(),
            "[server]\nhost = \"localhost\"\nport = 8080\n".into(),
            RichKind::Config,
        ),
        (
            "Cargo.lock",
            lock.clone(),
            lock.replacen("version = \"", "version = \"9", 1),
            RichKind::Deps,
        ),
        (
            "table.csv",
            csv.clone(),
            csv.replace("item100,100", "item100,101"),
            RichKind::Table,
        ),
        (
            "README.md",
            markdown.clone(),
            markdown.replace("current implementation", "updated implementation"),
            RichKind::Prose,
        ),
        (
            "openapi.json",
            api.into(),
            api.replace("/items", "/products"),
            RichKind::Api,
        ),
        (
            "data.dvc",
            "outs:\n- md5: aaaaaaaa\n  size: 100\n  path: data.csv\n".into(),
            "outs:\n- md5: bbbbbbbb\n  size: 200\n  path: data.csv\n".into(),
            RichKind::Dvc,
        ),
    ];
    for (path, old, _, _) in &cases {
        write(&repo.dir.join(path), old)?;
    }
    repo.commit("initial documents", &[])?;
    for (path, _, new, _) in &cases {
        write(&repo.dir.join(path), new)?;
    }
    repo.commit("update documents", &[])?;
    for (path, _, _, kind) in &cases {
        let key = DiffKey::Commit {
            hash: repo.rev("HEAD")?,
            path: (*path).to_owned(),
        };
        bench.try_measure(&format!("rich/{path}"), || {
            let result = diff::rich::load(repo.path(), &key, *kind, None)?;
            assert!(
                matches!(result, Rich::Ready(_)),
                "Rich fixture fell back to plain: {path}"
            );
            Ok(result)
        });
    }
    Ok(())
}
