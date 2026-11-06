//! The LFS storage through the code paths the views use, on a clone whose LFS objects were
//! never downloaded.

use super::{StorageAct, caught, download_action, include_patterns, track_action};
use crate::git::{self, lfs_fixtures::pushed_and_cloned};
use crate::repo::{Action, RepoTab};

#[test]
fn files_downloaded_by_name_alone() -> Result<(), String> {
    let paths = ["art dir/a b.bin".to_owned(), "x[1]*.bin".to_owned()];
    assert_eq!(
        include_patterns(&paths),
        "/art dir/a b.bin,/x\\[1\\]\\*.bin"
    );
    match download_action(&paths[..1]) {
        Ok(Action::Ops { label, ops, .. }) => {
            assert_eq!(label, "LFS: Download art dir/a b.bin");
            assert_eq!(
                ops,
                [[
                    "lfs",
                    "pull",
                    "--include",
                    "/art dir/a b.bin",
                    "--exclude",
                    ""
                ]]
            );
        }
        _ => return Err("not an operation".to_owned()),
    }
    assert!(download_action(&["a,b.bin".to_owned()]).is_err_and(|e| e.contains("comma")));
    match download_action(&[]) {
        Ok(Action::Ops { label, ops, .. }) => {
            assert_eq!(label, "LFS: Download");
            assert_eq!(ops, [["lfs", "pull"]]);
        }
        _ => return Err("not an operation".to_owned()),
    }
    assert!(
        matches!(track_action("*.psd"), Action::Ops { label, .. } if label == "Track *.psd in LFS")
    );
    Ok(())
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
    wait(&mut tab, &ctx, |t| t.lfs && t.lfs_storage.files.len() == 2);
    assert!(tab.lfs_storage.only_pointer("ü.bin"));
    assert!(tab.lfs_storage.only_pointer("art dir/a b.bin"));
    assert!(!tab.lfs_storage.only_pointer("README"));
    assert!(tab.is_checkout(None));
    assert!(tab.is_checkout(Some(&tab.refs.head.clone())));
    assert!(!tab.is_checkout(Some("0123abc")));

    tab.apply(Action::LfsStorage(StorageAct::OpenPanel), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.lfs_storage
            .panel
            .as_ref()
            .is_some_and(|p| p.usage.is_some() && p.filters.is_some())
    });
    let usage = tab.lfs_storage.panel.as_ref().and_then(|p| p.usage.clone());
    assert_eq!(usage.and_then(Result::ok).map(|u| u.objects), Some(0));

    let action = download_action(&["ü.bin".to_owned()])?;
    tab.apply(action, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.loading && !t.lfs_storage.only_pointer("ü.bin")
    });
    assert!(!tab.lfs_storage.only_pointer("ü.bin"), "{:?}", tab.status);
    assert!(tab.lfs_storage.only_pointer("art dir/a b.bin"));
    assert_eq!(
        std::fs::read(clone.join("ü.bin")).map(|b| b.len()).ok(),
        Some(700)
    );

    tab.status = None;
    tab.apply(
        Action::LfsStorage(StorageAct::FetchOnly("art dir".to_owned())),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| t.status.is_some());
    assert_eq!(git::fetch_filters(&clone).0, ["art dir"]);
    tab.apply(
        Action::LfsStorage(StorageAct::SetFilter {
            exclude: true,
            list: vec!["ü x".to_owned()],
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        t.lfs_storage
            .panel
            .as_ref()
            .and_then(|p| p.filters.as_ref())
            .is_some_and(|f| !f.1.is_empty())
    });
    assert_eq!(git::fetch_filters(&clone).1, ["ü x"]);

    tab.apply(Action::LfsStorage(StorageAct::CheckPrune), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.lfs_storage
            .panel
            .as_ref()
            .is_some_and(|p| p.plan.is_some())
    });
    let plan = tab.lfs_storage.panel.as_ref().and_then(|p| p.plan.clone());
    assert!(matches!(&plan, Some(Ok(p)) if p.local == 1), "{plan:?}");

    tab.apply(track_action("*.psd"), &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none());
    let attrs = std::fs::read_to_string(clone.join(".gitattributes")).unwrap_or_default();
    assert!(attrs.contains("*.psd filter=lfs"), "{attrs}");
    tab.apply(Action::LfsStorage(StorageAct::ClosePanel), &ctx);
    assert!(tab.lfs_storage.panel.is_none());
    if let Some(base) = work.parent() {
        let _ = std::fs::remove_dir_all(base);
    }
    Ok(())
}
