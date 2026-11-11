use super::{Undo, checkout_remote_undo, remote_start};
use crate::git;
use crate::repo::OpKind;
use crate::repo::operations::run_steps;

#[test]
fn remote_branches_start_from_their_full_ref() {
    assert_eq!(remote_start("origin/x"), "refs/remotes/origin/x");
    assert_eq!(remote_start("remotes/origin/x"), "refs/remotes/origin/x");
    assert_eq!(remote_start("refs/remotes/o/x"), "refs/remotes/o/x");
}

/// A tag named like the remote-tracking branch is no obstacle to checking it out.
#[test]
fn a_tag_named_like_the_remote_branch_does_not_shadow_it() {
    let base = std::env::temp_dir().join(format!("gitcraft-cmd-tag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::create_dir_all(&base);
    let (up, clone) = (base.join("up"), base.join("clone"));
    let up_s = up.to_string_lossy().into_owned();
    let clone_s = clone.to_string_lossy().into_owned();
    let setup: [(Option<&std::path::Path>, Vec<&str>); 7] = [
        (None, vec!["init", "-q", "-b", "main", &up_s]),
        (
            Some(&up),
            vec![
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "a",
            ],
        ),
        (Some(&up), vec!["branch", "feat"]),
        (None, vec!["clone", "-q", &up_s, &clone_s]),
        (Some(&clone), vec!["config", "user.name", "t"]),
        (Some(&clone), vec!["config", "user.email", "t@t"]),
        (Some(&clone), vec!["tag", "origin/feat", "HEAD"]),
    ];
    let set_up = setup.iter().all(|(dir, args)| git::run(*dir, args).is_ok());
    let redo = match checkout_remote_undo("main", "feat", "origin/feat") {
        Undo::Custom { redo, .. } => redo,
        _ => Vec::new(),
    };
    let (result, _) = run_steps(&clone, &redo, OpKind::plain(), None);
    let merge = git::run(Some(&clone), &["config", "branch.feat.merge"]);
    let _ = std::fs::remove_dir_all(&base);
    assert!(set_up);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        merge.map(|m| m.trim().to_owned()),
        Ok("refs/heads/feat".to_owned())
    );
}
