//! Behaviour observations reuse the existing fenced, recoverable workspace owner.
//! A sample is reliable only after two fresh worktrees agree and cleanup succeeds.
use super::{Machine, runner, workspace::Workspace};
use crate::{git, processes, remote};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

pub(crate) const CAPTURE: usize = 2 << 20;
const CACHE_BYTES: usize = 32 << 20;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Mode {
    Text,
    Number,
    Png,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Text(String),
    Number(f64),
    Png {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}
impl Value {
    fn bytes(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Number(_) => 8,
            Self::Png { rgba, .. } => rgba.len(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sample {
    pub value: Value,
    pub code: i32,
    pub stderr: Vec<u8>,
}
impl From<Value> for Sample {
    fn from(value: Value) -> Self {
        Self {
            value,
            code: 0,
            stderr: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Observation {
    Stable(Sample),
    Inconsistent,
}
impl Observation {
    fn bytes(&self) -> usize {
        match self {
            Self::Stable(v) => v.value.bytes() + v.stderr.len(),
            Self::Inconsistent => 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub repository: String,
    pub tree: String,
    pub command: String,
    pub mode: Mode,
}
#[derive(Clone, Default)]
pub(crate) struct Cache(Arc<Mutex<VecDeque<(Key, Observation)>>>);
impl Cache {
    pub(crate) fn get(&self, key: &Key) -> Option<Observation> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
    pub(crate) fn put(&self, key: Key, value: Observation) {
        let mut entries = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(k, _)| *k != key);
        entries.push_back((key, value));
        while entries.len() > 64
            || entries.iter().map(|(_, v)| v.bytes()).sum::<usize>() > CACHE_BYTES
        {
            entries.pop_front();
        }
    }
    pub(crate) fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}
pub(crate) fn first_difference(frames: &[Option<&Observation>]) -> Option<usize> {
    frames
        .windows(2)
        .position(|pair| {
            matches!((&pair[0], &pair[1]),
        (Some(Observation::Stable(a)), Some(Observation::Stable(b))) if a != b)
        })
        .map(|i| i + 1)
}
pub(crate) fn parse(mode: Mode, bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > CAPTURE {
        return Err("Frame output exceeds 2 MiB; no result cached".into());
    }
    match mode {
        Mode::Text => Ok(Value::Text(
            std::str::from_utf8(bytes)
                .map_err(|_| "Text output is not UTF-8")?
                .to_owned(),
        )),
        Mode::Number => {
            let n: f64 = std::str::from_utf8(bytes)
                .map_err(|_| "Benchmark output is not UTF-8")?
                .trim()
                .parse()
                .map_err(|_| "Benchmark must print one finite number")?;
            if !n.is_finite() {
                return Err("Benchmark must print one finite number".into());
            }
            Ok(Value::Number(n))
        }
        Mode::Png => {
            let mut reader = image::ImageReader::with_format(
                std::io::Cursor::new(bytes),
                image::ImageFormat::Png,
            );
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(1024);
            limits.max_image_height = Some(1024);
            limits.max_alloc = Some(16 << 20);
            reader.limits(limits);
            let image = reader
                .decode()
                .map_err(|e| format!("PNG frame could not be decoded (maximum 1024×1024): {e}"))?
                .into_rgba8();
            Ok(Value::Png {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            })
        }
    }
}
fn repeated(a: impl Into<Sample>, b: impl Into<Sample>) -> Observation {
    let (a, b) = (a.into(), b.into());
    if a == b {
        Observation::Stable(a)
    } else {
        Observation::Inconsistent
    }
}
pub(crate) fn run(
    repo: &Path,
    hash: &str,
    command: &str,
    mode: Mode,
    timeout: Duration,
) -> Result<Observation, String> {
    if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("A pinned full commit ID is required".into());
    }
    if command.trim().is_empty() || command.len() > 8192 {
        return Err("Give a command of at most 8192 bytes".into());
    }
    let mut ws = Workspace::open(repo, &Machine::of(repo), None)?;
    let result = (|| {
        let mut values = Vec::new();
        for n in 0..2 {
            let (path, there) = ws.slot(n);
            let setup = Duration::from_secs(600);
            let add = ws.run_git(
                &ws.repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "--no-checkout",
                    "--detach",
                    &there,
                    hash,
                ],
                setup,
            )?;
            successful(add)?;
            successful(ws.run_git(
                &path,
                &["checkout", "-q", "--force", "--detach", hash],
                setup,
            )?)?;
            let authority = ws.authority(remote::workspace_authority::FenceMode::Dispatch)?;
            let record = ws.owner_record(&authority)?;
            let words = runner::sh_alias(
                "gitcraft-observe",
                "exec sh -c \"$1\" </dev/null",
                &[command],
            );
            let args: Vec<_> = words.iter().map(String::as_str).collect();
            let output = processes::checked(git::run_workspace_command(
                &path,
                &args,
                &record,
                timeout,
                crate::tasks::cancellation(),
                git::WorkspaceCommand::new(&authority),
            ));
            let output = match output {
                Ok(out) => out,
                Err(e) => {
                    if matches!(e, processes::Failure::Failed(_)) {
                        ws.invalidate();
                    }
                    return Err(e.message());
                }
            };
            if output.stop_kind == Some(processes::StopKind::Uncertain) {
                ws.invalidate();
            }
            // A command result is never usable while its actual owner remains unconfirmed.
            if let Err(e) = remote::recover_owner(&record) {
                ws.invalidate();
                return Err(format!("Observation cleanup unknown: {e}"));
            }
            let output = output.complete()?;
            let code = output
                .status
                .code()
                .ok_or("Observation command ended by a signal")?;
            if output.stderr.len() > CAPTURE {
                return Err("Frame stderr exceeds 2 MiB; no result cached".into());
            }
            let value = parse(mode, &output.stdout).map_err(|e| {
                format!(
                    "{e} (exit {code}); stderr: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(8192)
                        .collect::<String>()
                )
            })?;
            values.push(Sample {
                value,
                code,
                stderr: output.stderr,
            });
        }
        Ok(repeated(values.remove(0), values.remove(0)))
    })();
    finish(result, ws.close())
}
fn successful(exit: git::Exit) -> Result<(), String> {
    if exit.code == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "Observation checkout failed: {}",
            String::from_utf8_lossy(&exit.stderr)
                .chars()
                .take(8192)
                .collect::<String>()
        ))
    }
}
fn finish(
    result: Result<Observation, String>,
    cleanup: Result<(), String>,
) -> Result<Observation, String> {
    match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(cleanup)) => Err(format!("{e}; workspace cleanup: {cleanup}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> Key {
        Key {
            repository: "ssh://machine/repo".into(),
            tree: "tree".into(),
            command: "command".into(),
            mode: Mode::Text,
        }
    }
    #[test]
    fn recording_repeats_and_first_difference_never_fill_unknown_or_flaky_gaps() {
        let a = Value::Text("a".into());
        let b = Value::Text("b".into());
        assert_eq!(repeated(a.clone(), b.clone()), Observation::Inconsistent);
        let good = Some(repeated(a.clone(), a));
        let changed = Some(repeated(b.clone(), b));
        assert_eq!(
            first_difference(&[good.as_ref(), None, changed.as_ref()]),
            None
        );
        assert_eq!(
            first_difference(&[
                good.as_ref(),
                Some(&Observation::Inconsistent),
                changed.as_ref()
            ]),
            None
        );
        assert_eq!(
            first_difference(&[good.as_ref(), changed.as_ref()]),
            Some(1)
        );
        let before = Sample::from(Value::Text("same stdout".into()));
        let mut after = before.clone();
        after.code = 1;
        assert_eq!(repeated(before.clone(), after), Observation::Inconsistent);
        let mut after = before.clone();
        after.stderr = vec![255];
        assert_eq!(repeated(before, after), Observation::Inconsistent);
    }
    #[test]
    fn recording_cache_is_bounded_and_keyed_by_copy_tree_command_and_mode() -> Result<(), String> {
        let cache = Cache::default();
        let k = key();
        cache.put(k.clone(), Observation::Inconsistent);
        for mut other in [k.clone(), k.clone(), k.clone(), k.clone()]
            .into_iter()
            .enumerate()
        {
            match other.0 {
                0 => other.1.repository.push('2'),
                1 => other.1.tree.push('2'),
                2 => other.1.command.push('2'),
                _ => other.1.mode = Mode::Number,
            }
            assert!(cache.get(&other.1).is_none());
        }
        for i in 0..20 {
            let mut k = k.clone();
            k.tree = i.to_string();
            cache.put(
                k,
                Observation::Stable(Value::Text("x".repeat(CAPTURE)).into()),
            );
        }
        assert!(
            cache
                .0
                .lock()
                .map_err(|error| error.to_string())?
                .iter()
                .map(|(_, v)| v.bytes())
                .sum::<usize>()
                <= CACHE_BYTES
        );
        cache.clear();
        assert!(cache.get(&k).is_none());
        Ok(())
    }
    #[test]
    fn recording_output_and_cleanup_errors_remain_errors() -> Result<(), String> {
        assert!(parse(Mode::Number, b"NaN").is_err());
        assert!(parse(Mode::Number, b"1 2").is_err());
        assert_eq!(
            parse(Mode::Number, b" 1.25\n").map_err(|error| error.to_string())?,
            Value::Number(1.25)
        );
        assert!(parse(Mode::Text, &[255]).is_err());
        assert!(parse(Mode::Text, &vec![0; CAPTURE + 1]).is_err());
        assert!(parse(Mode::Png, b"not an image").is_err());
        let error = finish(
            Err("primary ownership error".into()),
            Err("cleanup error".into()),
        )
        .err()
        .ok_or("expected an error")?;
        assert!(error.contains("primary ownership error") && error.contains("cleanup error"));
        assert!(finish(Ok(Observation::Inconsistent), Err("cleanup failed".into())).is_err());
        Ok(())
    }
    #[test]
    fn recording_png_frames_compare_pixels_and_refuse_oversized_dimensions() -> Result<(), String> {
        use image::ImageEncoder;
        fn png(width: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
            let mut bytes = Vec::new();
            image::codecs::png::PngEncoder::new(&mut bytes)
                .write_image(rgba, width, 1, image::ExtendedColorType::Rgba8)
                .map_err(|error| error.to_string())?;
            Ok(bytes)
        }
        let a = parse(Mode::Png, &png(1, &[1, 2, 3, 255])?).map_err(|error| error.to_string())?;
        assert_eq!(
            a,
            Value::Png {
                width: 1,
                height: 1,
                rgba: vec![1, 2, 3, 255]
            }
        );
        assert_eq!(
            repeated(a.clone(), a.clone()),
            Observation::Stable(a.into())
        );
        assert!(parse(Mode::Png, &png(1025, &vec![0; 1025 * 4])?).is_err());
        Ok(())
    }
}
