//! Follow the production dispatch, including Git attributes and reading both blobs.
use super::*;

pub(super) fn run(bench: &mut Bench) -> Result<(), String> {
    let repo = git::fixture::Repo::new("cpu-documents")?;
    let hooks = repo.dir.join(".git/no-hooks");
    std::fs::create_dir_all(&hooks).map_err(|e| e.to_string())?;
    repo.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    write(
        &repo.dir.join(".git/info/attributes"),
        "*.pdf -diff\n*.docx -diff\n*.xlsx -diff\n*.pptx -diff\n*.odt -diff\n*.zip -diff\n*.png !diff\n",
    )?;
    let old: Vec<_> = (0..300)
        .map(|i| format!("Paragraph {i} explains the current implementation."))
        .collect();
    let new: Vec<_> = old
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i % 50 == 0 {
                s.replace("current", "updated")
            } else {
                s.clone()
            }
        })
        .collect();
    let a: Vec<_> = old.iter().map(String::as_str).collect();
    let b: Vec<_> = new.iter().map(String::as_str).collect();
    let mut cases = vec![
        (
            "report.pdf",
            diff::make_pdf(&a, true),
            diff::make_pdf(&b, true),
        ),
        (
            "plain.pdf",
            diff::make_pdf(&a, false),
            diff::make_pdf(&b, false),
        ),
        ("spec.docx", diff::make_docx(&a), diff::make_docx(&b)),
    ];
    for (path, member, old, new) in [
        (
            "data.xlsx",
            "xl/worksheets/sheet1.xml",
            "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>42</v></c></row></sheetData></worksheet>",
            "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>43</v></c></row></sheetData></worksheet>",
        ),
        (
            "slides.pptx",
            "ppt/slides/slide1.xml",
            "<p:sld><a:p><a:r><a:t>Old plan</a:t></a:r></a:p></p:sld>",
            "<p:sld><a:p><a:r><a:t>New plan</a:t></a:r></a:p></p:sld>",
        ),
        (
            "notes.odt",
            "content.xml",
            "<office:text><text:p>Old plan</text:p></office:text>",
            "<office:text><text:p>New plan</text:p></office:text>",
        ),
    ] {
        cases.push((
            path,
            diff::make_zip(&[(member, old.as_bytes(), true)]),
            diff::make_zip(&[(member, new.as_bytes(), true)]),
        ));
    }
    let names: Vec<_> = (0..1000)
        .map(|i| format!("source/package{}/file{i}.txt", i % 20))
        .collect();
    let contents = "Useful contents\n".repeat(50);
    let members: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), contents.as_bytes(), i % 2 == 0))
        .collect();
    let mut changed = members.clone();
    changed[10].1 = b"Updated contents\n";
    changed.pop();
    changed.push(("added.txt", b"New member", true));
    cases.push((
        "sources.zip",
        diff::make_zip(&members),
        diff::make_zip(&changed),
    ));
    for ids in [true, false] {
        let notebook = |updated: bool| {
            let mut cells: Vec<_> = (0..200).map(|i| {
                let modified = updated && i % 50 == 0;
                let mut cell = serde_json::json!({"cell_type":"code","metadata":{},"source":format!("print({})\n", i+usize::from(modified)),"execution_count":i,"outputs":[{"output_type":"stream","name":"stdout","text":[format!("{}\n",i+usize::from(modified))]}]});
                if ids { cell["id"] = format!("cell{i}").into(); }
                cell
            }).collect();
            if updated {
                cells.swap(10, 11);
            }
            serde_json::to_vec(
                &serde_json::json!({"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":cells}),
            )
            .map_err(|e| e.to_string())
        };
        cases.push((
            if ids {
                "stable-ids.ipynb"
            } else {
                "without-ids.ipynb"
            },
            notebook(false)?,
            notebook(true)?,
        ));
    }
    cases.push((
        "schema.xml",
        b"<project><version>1</version></project>\n".to_vec(),
        b"<project><version>2</version></project>\n".to_vec(),
    ));
    cases.push((
        "model.bin",
        b"version https://git-lfs.github.com/spec/v1\noid sha256:75e3\nsize 5000\n".to_vec(),
        b"version https://git-lfs.github.com/spec/v1\noid sha256:dc35\nsize 7000\n".to_vec(),
    ));
    let png = |color| {
        let image = image::RgbaImage::from_pixel(1280, 720, image::Rgba(color));
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(output.into_inner())
    };
    cases.push((
        "screenshot.png",
        png([80, 120, 200, 180])?,
        png([90, 120, 200, 180])?,
    ));
    for (path, old, _) in &cases {
        write(&repo.dir.join(path), old)?;
    }
    repo.commit("initial content", &[])?;
    for (path, _, new) in &cases {
        write(&repo.dir.join(path), new)?;
    }
    let revision = repo.commit("update content", &[])?;
    for (path, _, _) in &cases {
        let key = DiffKey::Commit {
            hash: revision.clone(),
            path: (*path).into(),
        };
        bench.try_measure(&format!("documents/{path}"), || {
            let data = diff::load(repo.path(), &key, git::DiffOptions::default(), true)?;
            match *path {
                p if p.ends_with("ipynb") => {
                    assert!(data.notebook.is_some(), "{path}: notebook fallback")
                }
                "model.bin" => assert!(data.lfs.is_some(), "LFS pointer fallback"),
                "screenshot.png" => assert!(data.compare.is_some(), "image decode fallback"),
                "schema.xml" => assert!(data.lines.iter().any(|l| l.kind == diff::Kind::Add)),
                _ => {
                    let binary = data.binary.as_ref().ok_or("binary fallback")?;
                    assert!(binary.error.is_none(), "{path}: {:?}", binary.error);
                    if *path == "sources.zip" {
                        assert_eq!(
                            binary
                                .members
                                .as_ref()
                                .ok_or("missing archive changes")?
                                .len(),
                            3
                        );
                    } else {
                        assert!(
                            binary.text_changes > 0,
                            "{path}: missing extracted text changes"
                        );
                    }
                }
            }
            Ok(data)
        });
    }
    Ok(())
}
