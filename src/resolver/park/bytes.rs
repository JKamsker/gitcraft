use super::*;

#[derive(Serialize, Deserialize)]
pub(super) struct CleanFile {
    path: String,
    object: String,
    bytes: Vec<u8>,
}

pub(super) fn head_files(repo: &Path) -> Result<Vec<(String, String)>, String> {
    run(repo, &["ls-tree", "-r", "-z", "HEAD"])?
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (fields, path) = entry.split_once('\t')?;
            let mut fields = fields.split_whitespace();
            let mode = fields.next()?;
            if !matches!(mode, "100644" | "100755") {
                return None;
            }
            fields.next()?;
            Some(
                crate::mcp::args::repo_path(path)
                    .map(|path| (path.to_owned(), fields.next().unwrap_or("").to_owned())),
            )
        })
        .collect()
}

pub(super) fn clean_baseline(repo: &Path) -> Result<Vec<CleanFile>, String> {
    head_files(repo)?
        .into_iter()
        .map(|(path, object)| {
            let bytes =
                crate::safe_read::read_inside(repo, &path, u64::MAX - 1).ok_or_else(|| {
                    format!("Could not record clean bytes of {path}; merge remains put aside")
                })?;
            Ok(CleanFile {
                path,
                object,
                bytes,
            })
        })
        .collect()
}

/// Git status compares filtered content. Prove the raw bytes safe before any checkout,
/// read-tree, rollback or raw archive write can overwrite a clean-filter-hidden edit.
pub(super) fn clean_bytes_guard(repo: &Path, parked: &Parked) -> Result<(), String> {
    let baseline: Vec<CleanFile> =
        std::fs::read(discovery::locate(repo, &parked.id)?.join("clean-baseline.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    for (path, object) in head_files(repo)? {
        let actual = crate::safe_read::read_inside(repo, &path, u64::MAX - 1);
        let expected = match baseline
            .iter()
            .find(|f| f.path == path && f.object == object)
        {
            Some(file) => file.bytes.clone(),
            None => git::run_bytes(
                Some(repo),
                &["cat-file", "--filters", &format!("HEAD:{path}")],
            )?,
        };
        if actual.as_deref() != Some(expected.as_slice()) {
            return Err(format!(
                "{path} has raw bytes changed outside Git's filtered status; save them elsewhere before bringing the merge back"
            ));
        }
    }
    Ok(())
}
