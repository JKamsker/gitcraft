//! The LFS storage through the code paths the views use, on a clone whose LFS objects were
//! never downloaded.

use super::{StorageAct, caught, changes_lfs_files, download_action, track_action};
use crate::git::{self, lfs_fixtures::pushed_and_cloned};
use crate::repo::{Action, RepoTab};

fn steps(action: Result<Action, String>) -> Result<(String, Vec<Vec<String>>), String> {
    match action? {
        Action::Ops { label, ops, .. } => Ok((label, ops)),
        _ => Err("not an operation".to_owned()),
    }
}

#[test]
fn files_downloaded_by_name_alone() -> Result<(), String> {
    let paths = ["art dir/a b.bin".to_owned(), "x[1]*.bin".to_owned()];
    let (label, ops) = steps(download_action(&paths))?;
    assert_eq!(label, "LFS: Download 2 files");
    assert_eq!(
        ops,
        [[
            "lfs",
            "pull",
            "--include",
            "/art dir/a b.bin,/x\\[1\\]\\*.bin",
            "--exclude",
            ""
        ]]
    );
    let (label, _) = steps(download_action(&paths[..1]))?;
    assert_eq!(label, "LFS: Download art dir/a b.bin");
    assert!(download_action(&["a,b.bin".to_owned()]).is_err_and(|e| e.contains("comma")));
    assert!(download_action(&["trail.bin ".to_owned()]).is_err_and(|e| e.contains("space")));
    let (label, ops) = steps(download_action(&[]))?;
    assert_eq!(
        (label.as_str(), ops),
        (
            "LFS: Download",
            vec![vec!["lfs".to_owned(), "pull".to_owned()]]
        )
    );
    // So many that they don't fit one command line: several steps, every file named once.
    let many: Vec<String> = (0..3000)
        .map(|i| format!("art/level {i:04}/texture.png"))
        .collect();
    let (_, ops) = steps(download_action(&many))?;
    assert!(ops.len() > 1);
    assert!(ops.iter().all(|step| step[3].len() <= 16 * 1024 + 64));
    let named: usize = ops.iter().map(|step| step[3].split(',').count()).sum();
    assert_eq!(named, many.len());
    assert!(
        matches!(track_action("*.psd"), Action::Ops { label, .. } if label == "Track *.psd in LFS")
    );
    Ok(())
}

#[test]
fn which_operations_list_the_lfs_files_again() {
    for label in [
        "LFS: Download a.bin",
        "LFS: Pull",
        "Pull",
        "Track *.psd in LFS",
        "Stash",
    ] {
        assert!(changes_lfs_files(label), "{label}");
    }
    for label in ["Stage", "Unstage all", "Commit", "Fetch", "Push"] {
        assert!(!changes_lfs_files(label), "{label}");
    }
}

#[test]
fn a_job_that_panics_still_answers() {
    let r: Result<(), String> = caught(|| std::panic::resume_unwind(Box::new("boom")));
    assert_eq!(r, Err("Internal error: boom".to_owned()));
    assert_eq!(caught(|| Ok(3)), Ok(3));
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..600 {
        tab.poll(ctx);
        tab.lfs_storage_tick(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn panel_has<T>(tab: &RepoTab, get: impl Fn(&super::Panel) -> Option<T>) -> Option<T> {
    tab.lfs_storage.panel.as_ref().and_then(get)
}

/// Opening a clone with nothing downloaded: its files are marked as pointers, the panel
/// measures the storage, one file is downloaded from its menu's action, a folder is set to be
/// the only one fetched, and a new file type is tracked.
#[test]
fn storage_in_the_tab() -> Result<(), String> {
    let Some((work, clone)) = pushed_and_cloned("tab")? else {
        return Ok(());
    };
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(clone.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.lfs && t.lfs_storage.files_read);
    assert!(tab.lfs_storage.only_pointer("ü.bin"));
    assert!(tab.lfs_storage.only_pointer("art dir/a b.bin"));
    assert!(!tab.lfs_storage.only_pointer("README"));
    assert_eq!(tab.lfs_storage.patterns, ["*.bin"]);
    assert!(tab.is_checkout(None));
    assert!(tab.is_checkout(Some(&tab.refs.head.clone())));
    assert!(!tab.is_checkout(Some("0123abc")));

    tab.apply(Action::LfsStorage(StorageAct::OpenPanel), &ctx);
    wait(&mut tab, &ctx, |t| {
        panel_has(t, |p| p.filters.as_ref().map(|_| ())).is_some()
    });
    let usage = panel_has(&tab, |p| p.usage.clone());
    assert_eq!(usage.and_then(Result::ok).map(|u| u.objects), Some(0));

    tab.apply(download_action(&["ü.bin".to_owned()])?, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.lfs_storage.listing && !t.lfs_storage.only_pointer("ü.bin")
    });
    assert!(!tab.lfs_storage.only_pointer("ü.bin"), "{:?}", tab.status);
    assert!(tab.lfs_storage.only_pointer("art dir/a b.bin"));
    assert_eq!(
        std::fs::read(clone.join("ü.bin")).map(|b| b.len()).ok(),
        Some(700)
    );
    // Measured again after the download.
    wait(&mut tab, &ctx, |t| {
        panel_has(t, |p| p.usage.clone().and_then(Result::ok)).is_some_and(|u| u.objects == 1)
    });

    tab.status = None;
    let folder = git::folder_pattern("art dir").ok_or("no pattern")?;
    tab.apply(Action::LfsStorage(StorageAct::FetchOnly(folder)), &ctx);
    wait(&mut tab, &ctx, |t| t.status.is_some());
    assert_eq!(
        tab.status.clone().map(|s| s.0).as_deref(),
        Some("LFS now fetches only art dir")
    );
    assert_eq!(git::fetch_filters(&clone)?.include, ["/art dir"]);
    let list = vec![git::folder_pattern("ü x").ok_or("no pattern")?];
    tab.apply(
        Action::LfsStorage(StorageAct::SetFilter {
            exclude: true,
            list,
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        panel_has(t, |p| p.filters.clone().and_then(Result::ok))
            .is_some_and(|f| !f.exclude.is_empty())
    });
    assert_eq!(git::fetch_filters(&clone)?.exclude, ["/ü x"]);

    tab.apply(Action::LfsStorage(StorageAct::CheckPrune), &ctx);
    wait(&mut tab, &ctx, |t| {
        panel_has(t, |p| p.plan.clone()).is_some()
    });
    let plan = panel_has(&tab, |p| p.plan.clone());
    assert!(matches!(&plan, Some(Ok(p)) if p.local == 1), "{plan:?}");

    tab.apply(track_action("*.psd"), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.loading && t.lfs_storage.patterns.len() == 2
    });
    let attrs = std::fs::read_to_string(clone.join(".gitattributes")).unwrap_or_default();
    assert!(attrs.contains("*.psd filter=lfs"), "{attrs}");
    assert_eq!(tab.lfs_storage.patterns, ["*.bin", "*.psd"]);
    tab.apply(Action::LfsStorage(StorageAct::ClosePanel), &ctx);
    assert!(tab.lfs_storage.panel.is_none());
    if let Some(base) = work.parent() {
        let _ = std::fs::remove_dir_all(base);
    }
    Ok(())
}
