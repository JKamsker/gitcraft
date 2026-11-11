use super::*;
#[test]
fn physical_paths_include_every_drive_and_unc_ancestor_without_drive_relative_prefixes()
-> Result<(), String> {
    for root in [r"\\?\C:\parent\repo", r"\\?\UNC\server\share\parent\repo"] {
        let root = Path::new(root);
        let paths = physical_ancestors(root)?;
        assert_eq!(paths.last().map(std::path::PathBuf::as_path), Some(root));
        assert!(
            paths
                .iter()
                .all(|path| path.is_absolute() && path.has_root())
        );
        assert!(paths.len() >= 3);
        for pair in paths.windows(2) {
            assert_eq!(pair[1].parent(), Some(pair[0].as_path()));
        }
    }
    assert!(physical_ancestors(Path::new(r"C:relative")).is_err());
    Ok(())
}
