#[test]
fn commit_index_worktree_and_extensionless_sops_are_recognized_without_decrypting()
-> Result<(), String> {
    let repo = Repo::new()?;
    let path = "encrypted-config";
    let old = json("age1first", "cipher1");
    let indexed = json("age1second", "cipher2");
    let working = json("age1third", "cipher3");
    repo.write(path, &old)?;
    let root = repo.commit()?;
    repo.write(path, &indexed)?;
    repo.git(&["add", "--", path])?;
    repo.write(path, &working)?;
    assert_eq!(
        read_sides(&repo.0, &work(path, Area::Staged, 'M'))?,
        (old.clone(), indexed)
    );
    assert_eq!(
        read_sides(
            &repo.0,
            &DiffKey::Commit {
                hash: root,
                path: path.into()
            }
        )?,
        (String::new(), old)
    );
    assert!(probe(&repo.0, &work(path, Area::Unstaged, 'M')));
    assert!(
        crate::repo::diff::load(
            &repo.0,
            &work(path, Area::Unstaged, 'M'),
            git::DiffOptions::default(),
            true
        )?
        .sops
    );
    Ok(())
}

#[test]
fn git_textconv_cannot_decrypt_ordinary_patches_or_run_before_opt_in() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    let driver = repo.0.join("decrypting-textconv");
    let marker = repo.0.join("textconv-ran");
    std::fs::write(
        &driver,
        format!(
            "#!/bin/sh\nprintf ran > {}; printf 'fixture-private-conversion'\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    repo.write(".gitattributes", "secret.json diff=sops-fixture\n")?;
    repo.git(&[
        "config",
        "diff.sops-fixture.textconv",
        &driver.to_string_lossy(),
    ])?;
    repo.write(path, &json("age1fixture", "cipher1"))?;
    let first = repo.commit()?;
    // Untracked sources have their own Git --no-index path: exercise actual load/UI.
    let untracked = "untracked-secret.json";
    repo.write(".gitattributes", "*.json diff=sops-fixture\n")?;
    repo.write(untracked, &json("age1fixture", "cipher1"))?;
    let key = work(untracked, Area::Untracked, '?');
    let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
    assert!(data.sops);
    assert!(!marker.exists());
    assert!(
        data.lines
            .iter()
            .all(|l| !l.text.contains("fixture-private-conversion"))
    );
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = crate::repo::RepoTab::new_deferred(repo.0.clone(), &ctx);
    let mut view = crate::repo::DiffView::new(key);
    view.next_request();
    view.data = Some(Ok(data));
    tab.diff = Some(view);
    let mut actions = Vec::new();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut actions));
    });
    let text = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.shapes.clear();
    out.textures_delta.clear();
    assert!(text.contains("Show decrypted"));
    assert!(!text.contains("fixture-private-conversion"));
    // Both the initial log-L query and selected patch must use ciphertext lines.
    let history = git::line_history(&repo.0, &git::LinesAt::Rev(first.clone()), path, (1, 1))?;
    assert!(!history.is_empty());
    assert!(!marker.exists());
    let selected = crate::repo::diff::load_lines(
        &repo.0,
        &DiffKey::Commit {
            hash: first.clone(),
            path: path.into(),
        },
        Some(&[(1, 1)]),
        git::DiffOptions::default(),
        true,
    )?;
    assert!(selected.sops);
    assert!(
        selected
            .lines
            .iter()
            .all(|l| !l.text.contains("fixture-private-conversion"))
    );
    assert!(!marker.exists());
    // WorkTree mapping into HEAD must also avoid conversion.
    git::line_history(&repo.0, &git::LinesAt::WorkTree, path, (1, 1))?;
    assert!(!marker.exists());
    repo.write(path, &json("age1fixture", "cipher2"))?;
    for key in [
        work(path, Area::Unstaged, 'M'),
        DiffKey::Range {
            from: first,
            to: None,
            path: path.into(),
        },
    ] {
        let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
        assert!(data.sops);
        assert!(!marker.exists());
        assert!(
            data.lines
                .iter()
                .all(|l| !l.text.contains("fixture-private-conversion"))
        );
    }
    Ok(())
}

#[test]
fn unreadable_oversize_and_nonregular_sources_fail_closed_before_textconv() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    let driver = repo.0.join("converter");
    let marker = repo.0.join("converter-ran");
    std::fs::write(
        &driver,
        format!(
            "#!/bin/sh\nprintf ran > {}; printf converted\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    repo.write(".gitattributes", "*.json diff=sops-fixture\n")?;
    repo.git(&[
        "config",
        "diff.sops-fixture.textconv",
        &driver.to_string_lossy(),
    ])?;
    for before in [json("age1fixture", "old"), "ordinary before\n".into()] {
        repo.write(path, &before)?;
        repo.git(&["add", "."])?;
        let oversized = format!(
            "{}{}",
            json("age1fixture", "new"),
            " ".repeat(2 * 1024 * 1024)
        );
        repo.write(path, &oversized)?;
        let key = work(path, Area::Unstaged, 'M');
        assert!(reader::read_sides(&repo.0, &key).is_err_and(|e| e.contains("2 MiB")));
        assert_eq!(probe_checked(&repo.0, &key), None);
        let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
        assert!(!marker.exists());
        assert!(data.lines.iter().all(|l| !l.text.contains("converted")));
    }
    // An actual missing working file is still an empty side, unlike an unavailable read.
    std::fs::remove_file(repo.0.join(path)).map_err(|e| e.to_string())?;
    let sides = reader::read_sides(&repo.0, &work(path, Area::Unstaged, 'M'))?;
    assert!(sides.1.is_empty());
    std::fs::create_dir(repo.0.join(path)).map_err(|e| e.to_string())?;
    assert!(reader::read_sides(&repo.0, &work(path, Area::Unstaged, 'M')).is_err());
    Ok(())
}
