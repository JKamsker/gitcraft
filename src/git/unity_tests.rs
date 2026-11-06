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
fn recommended_attributes_missing() {
    let attrs =
        "*.unity merge=unityyamlmerge eol=lf\n# *.prefab merge=unityyamlmerge\n*.png filter=lfs\n";
    let missing = missing_patterns(attrs);
    assert!(!missing.contains(&"*.unity".to_owned()));
    assert!(
        missing.contains(&"*.prefab".to_owned()),
        "a comment is no line"
    );
    assert!(!missing.contains(&"*.png".to_owned()));
    assert_eq!(
        missing_patterns("").len(),
        UNITY_YAML.len() + UNITY_BINARY.len()
    );
    let block = attributes_block(&["*.prefab".to_owned(), "*.fbx".to_owned()]);
    assert!(block.contains("*.prefab merge=unityyamlmerge eol=lf\n"));
    assert!(block.contains("*.fbx filter=lfs diff=lfs merge=lfs -text\n"));
}

#[test]
fn ignore_lines_for_the_project_folder() {
    let block = ignore_block(
        &project("My Game/"),
        &["Library".to_owned(), "UserSettings".to_owned()],
    );
    assert!(block.contains("/My Game/[Ll]ibrary/\n"));
    assert!(block.contains("/My Game/[Uu]serSettings/\n"));
    assert!(ignore_block(&project(""), &["Temp".to_owned()]).contains("/[Tt]emp/\n"));
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
    let tracked = |p: &str| {
        [
            "Assets/Gone.prefab.meta",
            "Assets/Kept.anim",
            "Assets/Old Folder/",
        ]
        .contains(&p)
    };
    let problems: Vec<(String, MetaKind)> = meta_problems(&p, &files, &tracked)
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
    let asked = meta_candidates(&p, &files);
    for wanted in [
        "Assets/Hero.png.meta",
        "Assets/New Folder.meta",
        "Assets/Orphan.asset/",
        "Assets/Gone.prefab.meta",
    ] {
        assert!(asked.contains(&wanted.to_owned()), "{wanted}");
    }
    assert!(!asked.iter().any(|p| p.starts_with("Packages")));
    assert!(meta_problems(&p, &[], &tracked).is_empty());
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

/// A project in a folder with a space is found, untracked; its settings and the repository's
/// files are checked; the fixes are appended and then nothing is missing.
#[test]
fn a_project_found_and_checked() -> Result<(), String> {
    let dir = unity_repo("found", "My Game/")?;
    let found = unity_project(&dir)?.ok_or("not found")?;
    assert_eq!(found.root, "My Game/");
    assert_eq!(found.version.as_deref(), Some("2022.3.10f1"));
    let setup = unity_setup(&dir, &found)?;
    assert_eq!(setup.force_text, Some(false));
    assert_eq!(setup.visible_meta, Some(true));
    assert_eq!(setup.unignored.len(), UNITY_IGNORED.len());
    assert!(!setup.all_good());

    std::fs::write(dir.join(".gitignore"), "*.log\r\n").map_err(|e| e.to_string())?;
    append_block(
        &dir,
        ".gitattributes",
        &attributes_block(&setup.missing_attributes),
    )?;
    append_block(&dir, ".gitignore", &ignore_block(&found, &setup.unignored))?;
    let setup = unity_setup(&dir, &found)?;
    assert!(
        setup.missing_attributes.is_empty(),
        "{:?}",
        setup.missing_attributes
    );
    assert!(setup.unignored.is_empty(), "{:?}", setup.unignored);
    let ignore = std::fs::read_to_string(dir.join(".gitignore")).map_err(|e| e.to_string())?;
    assert!(ignore.starts_with("*.log\r\n\r\n# Unity"), "{ignore:?}");
    assert!(!ignore.replace("\r\n", "").contains('\n'), "line ends kept");
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).map_err(|e| e.to_string())?;
    assert!(attrs.starts_with("# Unity"), "{attrs:?}");

    // The index answers which partners exist, a folder by what is inside it.
    std::fs::create_dir_all(dir.join("My Game/Assets/Art")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("My Game/Assets/Art/ü x.png"), "p").map_err(|e| e.to_string())?;
    git(&dir, &["add", "--", "My Game/Assets/Art/ü x.png"])?;
    let asked = [
        "My Game/Assets/Art/".to_owned(),
        "My Game/Assets/Art/ü x.png".to_owned(),
        "My Game/Assets/Art.meta".to_owned(),
    ];
    let found_in_index = in_index(&dir, &asked)?;
    assert!(found_in_index.contains("My Game/Assets/Art/"));
    assert!(found_in_index.contains("My Game/Assets/Art/ü x.png"));
    assert!(!found_in_index.contains("My Game/Assets/Art.meta"));
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
