//! Merge tree.

use super::*;

/// A file a merge would leave conflicted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictFile {
    pub path: String,
    /// What conflicts in it, as git names it: `contents`, `binary`, `modify/delete`,
    /// `rename/delete`, `add/add`, `file/directory`…
    pub kinds: Vec<String>,
}

impl ConflictFile {
    /// Only its lines conflict (both sides changed the text), so a merge tool working from
    /// the code's syntax may solve it. A binary file, or one deleted or renamed on one side,
    /// is a choice no tool makes.
    pub fn text_only(&self) -> bool {
        !self.kinds.is_empty() && self.kinds.iter().all(|k| k == "contents" || k == "add/add")
    }
}

/// One merge, worked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeMerge {
    /// The merged tree (with conflict markers in it when there are conflicts).
    pub tree: String,
    /// The conflicted files, in git's order; empty when it merges cleanly.
    pub conflicts: Vec<ConflictFile>,
}

/// Merges `ours` and `theirs` (commits; any tree-ish when `base` is given and git is 2.45 or
/// newer), from their merge bases or from `base`.
pub fn merge_tree(
    repo: &Path,
    base: Option<&str>,
    ours: &str,
    theirs: &str,
) -> Result<TreeMerge, String> {
    let base_arg = base.map(|b| format!("--merge-base={b}"));
    let mut args = vec!["merge-tree", "--write-tree", "--name-only", "-z"];
    if let Some(b) = &base_arg {
        args.push(b);
    }
    args.extend([END_OPTS, ours, theirs]);
    // 0: merged cleanly; 1: conflicts. Anything else is an error (an object that isn't there).
    let (code, out) = run_code(Some(repo), &args, &[0, 1])?;
    parse_merge_tree(&out, code == 0)
}

/// What `merge-tree --write-tree --name-only -z` printed: the tree, NUL; on conflicts, each
/// conflicted file NUL-ended, an empty field, then the messages, each as
/// `<count> NUL <path>… NUL <type> NUL <message> NUL`, the type being `CONFLICT (kind)` or
/// something informational (`Auto-merging`).
pub fn parse_merge_tree(out: &[u8], clean: bool) -> Result<TreeMerge, String> {
    let text = String::from_utf8_lossy(out);
    let mut fields = text.split('\0');
    let tree = fields.next().unwrap_or_default().trim().to_owned();
    if tree.is_empty() || !tree.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(crate::i18n::message(
            "backend-unexpected-answer-from-git-merge-tree-tree-8d1aa0",
            &[("tree", format!("{:?}", tree).into())],
        ));
    }
    if clean {
        return Ok(TreeMerge {
            tree,
            conflicts: Vec::new(),
        });
    }
    let mut conflicts: Vec<ConflictFile> = fields
        .by_ref()
        .take_while(|f| !f.is_empty())
        .map(|path| ConflictFile {
            path: path.to_owned(),
            kinds: Vec::new(),
        })
        .collect();
    if conflicts.is_empty() {
        return Err(crate::i18n::text(
            "backend-git-merge-tree-reported-conflicts-but-named-no-file-ea5861",
        )
        .to_owned());
    }
    read_messages(fields, &mut conflicts);
    Ok(TreeMerge { tree, conflicts })
}

/// Adds the kind of each `CONFLICT (kind)` message to the conflicted files it names.
pub(super) fn read_messages<'a>(
    mut fields: impl Iterator<Item = &'a str>,
    conflicts: &mut [ConflictFile],
) {
    while let Some(count) = fields.next() {
        let Ok(count) = count.parse::<usize>() else {
            return;
        };
        let paths: Vec<&str> = fields.by_ref().take(count).collect();
        let (Some(kind), Some(_message)) = (fields.next(), fields.next()) else {
            return;
        };
        let Some(kind) = kind
            .strip_prefix("CONFLICT (")
            .and_then(|k| k.strip_suffix(')'))
        else {
            continue;
        };
        for file in conflicts
            .iter_mut()
            .filter(|f| paths.contains(&f.path.as_str()))
        {
            if !file.kinds.iter().any(|k| k == kind) {
                file.kinds.push(kind.to_owned());
            }
        }
    }
}
