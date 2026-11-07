use super::{
    Invocation,
    files::{Snapshot, label},
    save::Output,
};
use crate::repo::{conflict::ConflictEditor, file_comparison::ReadOnlyComparison};

pub(super) enum Loaded {
    Diff(Box<ReadOnlyComparison>),
    Merge(Box<Merge>),
}
pub(super) struct Merge {
    pub inputs: Vec<Snapshot>,
    pub output: Output,
    pub editor: Option<ConflictEditor>,
    pub notice: Option<String>,
    pub computed: Option<String>,
    whole: Option<usize>,
    original_binary: bool,
}
pub(super) fn load(invocation: &Invocation, active: &dyn Fn() -> bool) -> Result<Loaded, String> {
    match invocation {
        Invocation::Diff(paths) => {
            let a = Snapshot::read(&paths[0], active)?;
            let b = Snapshot::read(&paths[1], active)?;
            ReadOnlyComparison::prepare(
                [label(&paths[0]), label(&paths[1])],
                [a.bytes, b.bytes],
                [a.stamp.is_some(), b.stamp.is_some()],
                active,
            )
            .map(|comparison| Loaded::Diff(Box::new(comparison)))
        }
        Invocation::Merge(paths) => {
            let inputs = paths[..3]
                .iter()
                .map(|p| Snapshot::read(p, active))
                .collect::<Result<Vec<_>, _>>()?;
            let output = Output::capture(&paths[3], &inputs)?;
            let initial = output.initial_bytes().map(Vec::from);
            let computed = compute(&inputs, active);
            if !active() {
                return Err("File merge cancelled".into());
            }
            let mut notice = None;
            let computed = match computed {
                Ok(text) => Some(text),
                Err(e) => {
                    notice = Some(e);
                    None
                }
            };
            let original_binary = initial.as_ref().is_some_and(|b| text(b).is_none());
            let start = initial
                .as_deref()
                .and_then(text)
                .map(str::to_owned)
                .or_else(|| {
                    if initial.is_none() {
                        computed.clone()
                    } else {
                        None
                    }
                });
            let editor = match start.map(ConflictEditor::standalone).transpose() {
                Ok(editor) => editor,
                Err(e) => {
                    notice = Some(e);
                    None
                }
            };
            Ok(Loaded::Merge(Box::new(Merge {
                inputs,
                output,
                editor,
                computed,
                notice,
                whole: None,
                original_binary,
            })))
        }
        _ => Err("Expected a standalone file invocation".into()),
    }
}
fn text(bytes: &[u8]) -> Option<&str> {
    (!bytes.contains(&0))
        .then(|| std::str::from_utf8(bytes).ok())
        .flatten()
}
impl Merge {
    pub fn choose(&mut self, side: usize) {
        let bytes = if side < 3 {
            self.inputs.get(side).map(|s| s.bytes.as_slice())
        } else {
            self.output.initial_bytes()
        };
        if let Some(bytes) = bytes {
            self.editor = text(bytes).and_then(|s| ConflictEditor::standalone(s.to_owned()).ok());
            self.whole = Some(side);
        }
    }
    pub fn use_computed(&mut self) -> Result<(), String> {
        let text = self
            .computed
            .clone()
            .ok_or("No computed text merge is available")?;
        self.editor = Some(ConflictEditor::standalone(text)?);
        self.whole = None;
        Ok(())
    }
    fn whole_bytes(&self) -> Option<&[u8]> {
        let side = self.whole?;
        let bytes = if side < 3 {
            self.inputs.get(side).map(|s| s.bytes.as_slice())
        } else {
            self.output.initial_bytes()
        }?;
        // Text MERGED may still contain unresolved markers. Only an explicit
        // whole input (or binary original) bypasses text conflict validation.
        if side == 3 && !self.original_binary {
            return None;
        }
        self.editor
            .as_ref()
            .is_none_or(|e| e.result.as_bytes() == bytes)
            .then_some(bytes)
    }
    pub fn can_save(&self) -> bool {
        self.whole_bytes().is_some() || self.editor.as_ref().is_some_and(ConflictEditor::resolved)
    }
    pub fn candidate(&self) -> Result<Vec<u8>, String> {
        if let Some(bytes) = self.whole_bytes() {
            return Ok(bytes.to_vec());
        }
        self.editor
            .as_ref()
            .ok_or("Choose a whole input or prepare a text result")?
            .solved()
            .map(String::into_bytes)
    }
}
fn compute(inputs: &[Snapshot], active: &dyn Fn() -> bool) -> Result<String, String> {
    if inputs.iter().any(|s| text(&s.bytes).is_none()) {
        return Err(
            "Binary or non-UTF8 inputs: choose a whole side to preserve exact bytes".into(),
        );
    }
    if inputs
        .iter()
        .any(|s| text(&s.bytes).is_some_and(|s| s.split_inclusive('\n').count() > 20_000))
    {
        return Err("More than 20,000 lines in an input; choose a whole side".into());
    }
    let temp = crate::git::file_comparison::Scratch::new()?;
    for (name, input) in ["base", "local", "remote"].into_iter().zip(inputs) {
        std::fs::write(temp.0.join(name), &input.bytes).map_err(|e| e.to_string())?;
    }
    let empty = temp.0.join("empty-config");
    std::fs::write(&empty, []).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("git");
    crate::shell_path::apply(&mut cmd);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.current_dir(&temp.0)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_CEILING_DIRECTORIES", &temp.0)
        .args([
            "--no-lazy-fetch",
            "merge-file",
            "--stdout",
            "--diff3",
            "--marker-size=7",
            "-L",
            "LOCAL",
            "-L",
            "BASE",
            "-L",
            "REMOTE",
            "--",
            "local",
            "base",
            "remote",
        ]);
    let out = crate::processes::run_while(
        cmd,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: 8 * 1024 * 1024,
            stderr_limit: Some(64 * 1024),
            ..Default::default()
        },
        active,
    )?
    .complete()?;
    if !out.status.code().is_some_and(|c| (0..=127).contains(&c)) {
        return Err(
            "Git could not prepare a three-way merge; choose a whole input or edit existing MERGED"
                .into(),
        );
    }
    if out.stdout.len() > super::files::FILE_LIMIT as usize {
        return Err("Computed result exceeds 2 MiB; choose a whole input".into());
    }
    String::from_utf8(out.stdout).map_err(|_| "The computed merge is not UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        base: &[u8],
        local: &[u8],
        remote: &[u8],
        merged: Option<&[u8]>,
    ) -> Result<
        (
            crate::git::file_comparison::Scratch,
            [std::path::PathBuf; 4],
        ),
        String,
    > {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let paths = ["base", "local", "remote", "merged"].map(|n| temp.0.join(n));
        for (path, bytes) in paths[..3].iter().zip([base, local, remote]) {
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        if let Some(bytes) = merged {
            std::fs::write(&paths[3], bytes).map_err(|e| e.to_string())?;
        }
        Ok((temp, paths))
    }
    #[test]
    fn existing_result_is_kept_and_computed_conflicts_require_a_choice() -> Result<(), String> {
        let (_temp, paths) = fixture(
            b"base\n",
            b"local\n",
            b"remote\n",
            Some(b"prior manual result\r\nlast"),
        )?;
        let Loaded::Merge(mut merge) = load(&Invocation::Merge(paths.clone()), &|| true)? else {
            return Err("not merge".into());
        };
        assert_eq!(merge.candidate()?, b"prior manual result\r\nlast");
        merge.use_computed()?;
        assert!(!merge.can_save());
        assert!(merge.candidate().is_err());
        merge.choose(1);
        assert_eq!(merge.candidate()?, b"local\n");
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"prior manual result\r\nlast"
        );
        Ok(())
    }
    #[test]
    fn nonoverlapping_three_way_merge_preserves_the_missing_final_newline() -> Result<(), String> {
        let (_temp, paths) = fixture(
            b"one\nbase\nthree\nfour\nfive\nbase-last",
            b"one\nlocal\nthree\nfour\nfive\nbase-last",
            b"one\nbase\nthree\nfour\nfive\nremote-last",
            None,
        )?;
        let Loaded::Merge(merge) = load(&Invocation::Merge(paths.clone()), &|| true)? else {
            return Err("not merge".into());
        };
        assert!(merge.can_save());
        assert_eq!(
            merge.candidate()?,
            b"one\nlocal\nthree\nfour\nfive\nremote-last"
        );
        assert!(!paths[3].exists());
        Ok(())
    }
    #[test]
    fn binary_whole_side_choices_preserve_bytes_and_stale_inputs_refuse_save() -> Result<(), String>
    {
        let (_temp, paths) = fixture(
            b"\0\xffbase",
            b"\0\xfelocal",
            b"\0\xfdremote",
            Some(b"\0old result"),
        )?;
        let Loaded::Merge(mut merge) = load(&Invocation::Merge(paths.clone()), &|| true)? else {
            return Err("not merge".into());
        };
        assert!(!merge.can_save());
        merge.choose(2);
        assert_eq!(merge.candidate()?, b"\0\xfdremote");
        std::fs::write(&paths[1], b"concurrent input").map_err(|e| e.to_string())?;
        assert!(
            merge
                .output
                .save(&merge.candidate()?, &merge.inputs)
                .is_err()
        );
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"\0old result"
        );
        Ok(())
    }
    #[test]
    fn file_modes_use_isolated_byte_comparison_without_repository_fallback() -> Result<(), String> {
        let (_temp, paths) = fixture(b"base\n", b"local\n", b"remote\n", None)?;
        // The byte comparator clears inherited Git configuration before invoking Git.
        (|| {
            assert!(matches!(
                load(
                    &Invocation::Diff([paths[0].clone(), paths[1].clone()]),
                    &|| true
                )?,
                Loaded::Diff(_)
            ));
            assert!(matches!(
                load(&Invocation::Merge(paths), &|| true)?,
                Loaded::Merge(_)
            ));
            Ok(())
        })()
    }
}
