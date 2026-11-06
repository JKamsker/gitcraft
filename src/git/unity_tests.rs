use std::path::{Path, PathBuf};

use super::*;
use crate::git::{Area, StatusFile};

fn staged(path: &str, code: char) -> StatusFile {
    StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area: Area::Staged,
    }
}

fn project(root: &str) -> UnityProject {
    UnityProject {
        root: root.to_owned(),
        version: None,
    }
}

#[test]
fn settings_read_from_unity_files() {
    let version =
        "m_EditorVersion: 2022.3.10f1\nm_EditorVersionWithRevision: 2022.3.10f1 (ff3792e53c62)\n";
    assert_eq!(editor_version(version).as_deref(), Some("2022.3.10f1"));
    assert_eq!(editor_version("nothing"), None);
    let editor = "%YAML 1.1\nEditorSettings:\n  m_ObjectHideFlags: 0\n  m_SerializationMode: 2\n";
    assert_eq!(serialization_is_text(editor), Some(true));
    assert_eq!(
        serialization_is_text("  m_SerializationMode: 1\n"),
        Some(false)
    );
    assert_eq!(serialization_is_text(""), None);
}

#[test]
fn what_needs_fixing_first() {
    let fine = UnitySetup {
        force_text: Some(true),
        vcs_mode: Some("Visible Meta Files".into()),
        missing_lfs: vec!["*.png".into()],
        unignored: vec!["Obj".into()],
        ..Default::default()
    };
    // LFS is the repository's choice; Obj alone isn't worth a hint.
    assert_eq!(fine.needs_fixing(), None);
    assert_eq!(fine.visible_meta(), Some(true));
    let perforce = UnitySetup {
        vcs_mode: Some("Perforce".into()),
        ..fine.clone()
    };
    assert_eq!(perforce.visible_meta(), Some(true));
    let hidden = UnitySetup {
        vcs_mode: Some("Hidden Meta Files".into()),
        ..fine.clone()
    };
    assert_eq!(
        hidden.needs_fixing().as_deref(),
        Some(".meta files are hidden")
    );
    let library = UnitySetup {
        unignored: vec!["Library".into()],
        ..fine.clone()
    };
    assert_eq!(
        library.needs_fixing().as_deref(),
        Some("Library isn't ignored")
    );
    let tracked = UnitySetup {
        tracked_generated: vec!["Temp".into()],
        ..library.clone()
    };
    assert_eq!(
        tracked.needs_fixing().as_deref(),
        Some("Temp has files committed")
    );
    let no_yaml = UnitySetup {
        missing_yaml: UNITY_YAML.iter().map(|p| (*p).to_owned()).collect(),
        ..fine
    };
    assert!(
        no_yaml
            .needs_fixing()
            .is_some_and(|s| s.contains("Smart Merge"))
    );
}

#[test]
fn attribute_and_ignore_lines() {
    let block = attributes_block(&["*.prefab".to_owned(), "*.fbx".to_owned()]);
    assert!(block.contains("*.prefab merge=unityyamlmerge eol=lf\n"));
    assert!(block.contains("*.fbx filter=lfs diff=lfs merge=lfs -text\n"));
    let block = ignore_block(
        &project("My Game/"),
        &["Library".to_owned(), "UserSettings".to_owned()],
    );
    assert!(block.contains("/My Game/[Ll]ibrary/\n"));
    assert!(block.contains("/My Game/[Uu]serSettings/\n"));
    assert!(ignore_block(&project(""), &["Temp".to_owned()]).contains("/[Tt]emp/\n"));
    // A folder whose name looks like a pattern matches only itself.
    let odd = ignore_block(&project("Game [Unity]/"), &["Temp".to_owned()]);
    assert!(odd.contains("/Game \\[Unity\\]/[Tt]emp/\n"), "{odd}");
}

#[test]
fn every_asset_and_folder_needs_its_meta() {
    let p = project("");
    let files = [
        staged("Assets/Hero.png", 'A'),
        staged("Assets/Hero.png.meta", 'A'),
        staged("Assets/New Folder/ü.mat", 'A'),
        staged("Assets/Orphan.asset.meta", 'A'),
        staged("Assets/Gone.prefab", 'D'),
        staged("Assets/Kept.anim.meta", 'D'),
        staged("Assets/Both.prefab", 'D'),
        staged("Assets/Both.prefab.meta", 'D'),
        staged("Packages/manifest.json", 'A'),
        staged("Assets/Old Folder.meta", 'A'),
    ];
    // In the index: the metas of the deleted prefab and the kept anim, and a folder.
    let index = IndexListing::of(
        [
            "Assets/Gone.prefab.meta",
            "Assets/Kept.anim",
            "Assets/Old Folder/x.png",
        ]
        .map(str::to_owned),
    );
    let problems: Vec<(String, MetaKind)> = meta_problems(&p, &files, &|p| index.has(p))
        .into_iter()
        .map(|m| (m.partner, m.kind))
        .collect();
    assert_eq!(
        problems,
        [
            (
                "Assets/Gone.prefab.meta".to_owned(),
                MetaKind::DeletedWithoutMeta
            ),
            (
                "Assets/Kept.anim".to_owned(),
                MetaKind::MetaDeletedAssetKept
            ),
            (
                "Assets/New Folder.meta".to_owned(),
                MetaKind::AddedWithoutMeta
            ),
            (
                "Assets/New Folder/ü.mat.meta".to_owned(),
                MetaKind::AddedWithoutMeta
            ),
            ("Assets/Orphan.asset".to_owned(), MetaKind::MetaWithoutAsset),
        ]
    );
    assert!(meta_problems(&p, &[], &|p| index.has(p)).is_empty());
}

#[test]
fn what_unity_makes_no_meta_for_isnt_flagged() {
    let p = project("");
    let files = [
        staged("Assets/.gitkeep", 'A'),
        staged("Assets/Samples~/x.cs", 'A'),
        staged("Assets/Docs~/readme.md", 'A'),
        staged("Assets/CVS/Entries", 'A'),
        staged("Assets/scratch.tmp", 'A'),
        staged("Assets/.hidden/y.png", 'A'),
    ];
    assert!(meta_problems(&p, &files, &|_| false).is_empty());
    assert!(skipped_by_unity("Assets/a/b~/c.png", "Assets/"));
    assert!(!skipped_by_unity("Assets/a/b/c.png", "Assets/"));
}

#[test]
fn a_project_in_a_folder_checks_its_own_assets() {
    let p = project("Game/");
    let files = [
        staged("Assets/x.png", 'A'),
        staged("Game/Assets/y.png", 'A'),
    ];
    let problems = meta_problems(&p, &files, &|_| false);
    let partners: Vec<&str> = problems.iter().map(|m| m.partner.as_str()).collect();
    assert_eq!(partners, ["Game/Assets/y.png.meta"]);
}

#[test]
fn the_index_listing_knows_files_and_folders() {
    let index = IndexListing::of(["Assets/a/b/c.png".to_owned(), "Assets/d.png".to_owned()]);
    assert!(index.has("Assets/d.png"));
    assert!(index.has("Assets/a/"));
    assert!(index.has("Assets/a/b/"));
    assert!(!index.has("Assets/a"));
    assert!(!index.has("Assets/e/"));
    let attrs = parse_attrs("a.png\0filter\0lfs\0a.png\0merge\0unspecified\0");
    assert_eq!(
        attrs
            .get(&("a.png".into(), "filter".into()))
            .map(String::as_str),
        Some("lfs")
    );
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

pub(crate) fn unity_repo(name: &str, root: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-unity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let settings = dir.join(root).join("ProjectSettings");
    std::fs::create_dir_all(&settings).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join(root).join("Assets")).map_err(|e| e.to_string())?;
    let write = |p: PathBuf, t: &str| std::fs::write(p, t).map_err(|e| e.to_string());
    write(
        settings.join("ProjectVersion.txt"),
        "m_EditorVersion: 2022.3.10f1\n",
    )?;
    write(
        settings.join("EditorSettings.asset"),
        "EditorSettings:\n  m_SerializationMode: 1\n",
    )?;
    write(
        settings.join("VersionControlSettings.asset"),
        "VersionControlSettings:\n  m_Mode: Visible Meta Files\n",
    )?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args)?;
    }
    Ok(dir)
}

/// A project in a folder with a space is found, untracked; its settings and what git does with
/// its files are checked (attributes in the project's own folder count); the fixes are appended
/// and then nothing is missing; a committed Library is found and taken out of git.
#[test]
fn a_project_found_and_checked() -> Result<(), String> {
    let dir = unity_repo("found", "My Game/")?;
    let found = unity_project(&dir)?.ok_or("not found")?;
    assert_eq!(found.root, "My Game/");
    assert_eq!(found.version.as_deref(), Some("2022.3.10f1"));
    // Attributes kept in the project's folder, in upper case for PNGs.
    std::fs::write(
        dir.join("My Game/.gitattributes"),
        "*.unity merge=unityyamlmerge eol=lf\n*.[pP][nN][gG] filter=lfs diff=lfs merge=lfs -text\n",
    )
    .map_err(|e| e.to_string())?;
    let setup = unity_setup(&dir, &found)?;
    assert_eq!(setup.force_text, Some(false));
    assert_eq!(setup.visible_meta(), Some(true));
    assert!(!setup.missing_yaml.contains(&"*.unity".to_owned()));
    assert!(setup.missing_yaml.contains(&"*.prefab".to_owned()));
    assert!(!setup.missing_lfs.contains(&"*.png".to_owned()));
    assert_eq!(setup.unignored.len(), UNITY_IGNORED.len());
    assert!(setup.tracked_generated.is_empty());

    std::fs::write(dir.join(".gitignore"), "*.log\r\n").map_err(|e| e.to_string())?;
    let missing: Vec<String> = setup
        .missing_yaml
        .iter()
        .chain(&setup.missing_lfs)
        .cloned()
        .collect();
    append_block(&dir, ".gitattributes", &attributes_block(&missing))?;
    append_block(&dir, ".gitignore", &ignore_block(&found, &setup.unignored))?;
    let setup = unity_setup(&dir, &found)?;
    assert!(setup.missing_yaml.is_empty(), "{:?}", setup.missing_yaml);
    assert!(setup.missing_lfs.is_empty(), "{:?}", setup.missing_lfs);
    assert!(setup.unignored.is_empty(), "{:?}", setup.unignored);
    let ignore = std::fs::read_to_string(dir.join(".gitignore")).map_err(|e| e.to_string())?;
    assert!(ignore.starts_with("*.log\r\n\r\n# Unity"), "{ignore:?}");
    assert!(!ignore.replace("\r\n", "").contains('\n'), "line ends kept");
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).map_err(|e| e.to_string())?;
    assert!(attrs.starts_with("# Unity"), "{attrs:?}");

    // Library committed before it was ignored.
    std::fs::create_dir_all(dir.join("My Game/Library")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("My Game/Library/cache.bin"), "c").map_err(|e| e.to_string())?;
    git(&dir, &["add", "-f", "--", "My Game/Library/cache.bin"])?;
    assert_eq!(unity_setup(&dir, &found)?.tracked_generated, ["Library"]);
    untrack_generated(&dir, &found, &["Library".to_owned()])?;
    assert!(unity_setup(&dir, &found)?.tracked_generated.is_empty());
    assert!(
        dir.join("My Game/Library/cache.bin").exists(),
        "kept on disk"
    );

    // The index under Assets/, in one listing.
    std::fs::create_dir_all(dir.join("My Game/Assets/Art")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("My Game/Assets/Art/ü x.png"), "p").map_err(|e| e.to_string())?;
    git(&dir, &["add", "--", "My Game/Assets/Art/ü x.png"])?;
    let index = index_under(&dir, "My Game/Assets/")?;
    assert!(index.has("My Game/Assets/Art/"));
    assert!(index.has("My Game/Assets/Art/ü x.png"));
    assert!(!index.has("My Game/Assets/Art.meta"));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn no_project_no_unity() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-unity-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a/b/ProjectSettings")).map_err(|e| e.to_string())?;
    // Two folders down: not looked for.
    std::fs::write(dir.join("a/b/ProjectSettings/ProjectVersion.txt"), "x")
        .map_err(|e| e.to_string())?;
    git(&dir, &["init", "-q"])?;
    let found = unity_project(&dir)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, None);
    Ok(())
}
