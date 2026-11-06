//! git's progress output (`--progress`), as it comes on the error output: phases such as
//! "Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s", each redrawn in place with a
//! `\r` until it ends with `, done.` and a newline.

/// Where a long git command is: the phase it is in, and how far into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// "Receiving objects", "Resolving deltas", "Writing objects"…
    pub phase: String,
    /// How far into the phase, 0 to 100, when git says (counting phases don't).
    pub percent: Option<u8>,
    /// The rest of git's line: "(450/1000), 1.20 MiB | 2.00 MiB/s".
    pub detail: String,
}

impl Progress {
    /// "Receiving objects: 45%", or the phase alone when it has no percentage.
    pub fn label(&self) -> String {
        match self.percent {
            Some(p) => format!("{}: {p}%", self.phase),
            None => self.phase.clone(),
        }
    }

    /// How far into the phase, from 0 to 1, when git says.
    pub fn fraction(&self) -> Option<f32> {
        self.percent.map(|p| f32::from(p.min(100)) / 100.0)
    }
}

/// Words git starts a message with that are not a phase ("error: 3 files…").
const NOT_PHASES: [&str; 4] = ["error", "fatal", "warning", "hint"];

/// The progress `line` reports, if it is one of git's progress lines (with or without the
/// `remote: ` the server's own lines start with); `None` for any other line.
pub fn parse_line(line: &str) -> Option<Progress> {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map_or(line, str::trim_start);
    let (phase, rest) = line.split_once(':')?;
    let rest = rest.trim_start();
    let is_phase = !phase.is_empty()
        && phase.chars().all(|c| c.is_alphabetic() || c == ' ')
        && phase.starts_with(char::is_alphabetic)
        && !NOT_PHASES.contains(&phase.to_lowercase().as_str());
    // A phase is followed by a count or a percentage.
    if !is_phase || !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    let (percent, detail) = match after.strip_prefix('%') {
        Some(tail) => (digits.parse::<u8>().ok().map(|p| p.min(100)), tail.trim()),
        None => (None, rest),
    };
    Some(Progress {
        phase: phase.to_owned(),
        percent,
        detail: detail.to_owned(),
    })
}

/// The most text kept of a command's error output; what came before is left out.
const MAX_TEXT: usize = 4 << 20;

/// Reads a command's error output as it comes, a chunk at a time: what it ends up saying (each
/// line redrawn with `\r` only as it was last drawn, as a terminal would show it) and the
/// latest progress line. A line that starts with its `marker` is taken out of the text and
/// kept apart (the process number of a command run on another machine).
#[derive(Default)]
pub struct ProgressReader {
    marker: Option<&'static str>,
    marked: Option<String>,
    /// The bytes since the last `\r` or `\n`.
    pending: Vec<u8>,
    /// The line drawn last before a `\r`, until a newline ends it.
    redrawn: Option<String>,
    text: String,
    latest: Option<Progress>,
    /// Some of the start of the text was left out (see [`MAX_TEXT`]).
    cut: bool,
}

impl ProgressReader {
    /// A reader that takes lines starting with `marker` apart (see [`Self::marked`]).
    pub fn with_marker(marker: &'static str) -> Self {
        Self {
            marker: Some(marker),
            ..Self::default()
        }
    }

    /// Takes in the next `bytes`; true when they brought a new progress line.
    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        let mut changed = false;
        for &b in bytes {
            match b {
                b'\r' => changed |= self.end_segment(false),
                b'\n' => changed |= self.end_segment(true),
                _ => self.pending.push(b),
            }
        }
        changed
    }

    /// The segment read since the last `\r` / `\n` ends; `newline`: the line is done, and
    /// stays in the text as last drawn.
    fn end_segment(&mut self, newline: bool) -> bool {
        let segment = String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned();
        if let Some(marker) = self.marker
            && let Some(rest) = segment.trim().strip_prefix(marker)
        {
            if self.marked.is_none() {
                self.marked = Some(rest.trim().to_owned());
            }
            return false;
        }
        let progress = parse_line(&segment);
        let changed = progress.is_some() && progress != self.latest;
        if progress.is_some() {
            self.latest = progress;
        }
        if newline {
            // `x\r\n`: the line is the one drawn last.
            let drawn = self.redrawn.take().unwrap_or_default();
            self.push_line(if segment.is_empty() { &drawn } else { &segment });
        } else if !segment.is_empty() {
            self.redrawn = Some(segment);
        }
        changed
    }

    fn push_line(&mut self, line: &str) {
        // An empty line between redraws says nothing.
        if line.trim().is_empty() {
            return;
        }
        self.text.push_str(line);
        self.text.push('\n');
        if self.text.len() > MAX_TEXT {
            let from = self.text.len() - MAX_TEXT;
            let from = self.text[from..]
                .find('\n')
                .map_or(self.text.len(), |i| from + i + 1);
            self.text.drain(..from);
            self.cut = true;
        }
    }

    /// The latest progress line read.
    pub fn latest(&self) -> Option<&Progress> {
        self.latest.as_ref()
    }

    /// What followed the marker on its line, once it came.
    pub fn marked(&self) -> Option<&str> {
        self.marked.as_deref()
    }

    /// Everything read: each redrawn line as last drawn.
    pub fn finish(mut self) -> String {
        let rest = String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned();
        let drawn = self.redrawn.take().unwrap_or_default();
        self.push_line(if rest.is_empty() { &drawn } else { &rest });
        if self.cut {
            format!("(earlier output left out)\n{}", self.text)
        } else {
            self.text
        }
    }
}

/// `text` without git's progress lines: what is left says how it went (an error, a summary).
pub fn without_progress(text: &str) -> String {
    text.lines()
        .filter(|l| parse_line(l).is_none())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(phase: &str, percent: Option<u8>, detail: &str) -> Option<Progress> {
        Some(Progress {
            phase: phase.to_owned(),
            percent,
            detail: detail.to_owned(),
        })
    }

    #[test]
    fn reads_the_lines_of_a_fetch_a_push_and_a_clone() {
        let cases = [
            (
                "Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s",
                progress(
                    "Receiving objects",
                    Some(45),
                    "(450/1000), 1.20 MiB | 2.00 MiB/s",
                ),
            ),
            (
                "remote: Counting objects: 100% (5/5), done.",
                progress("Counting objects", Some(100), "(5/5), done."),
            ),
            (
                "remote: Enumerating objects: 5, done.",
                progress("Enumerating objects", None, "5, done."),
            ),
            (
                "Writing objects: 100% (3/3), 280 bytes | 280.00 KiB/s, done.",
                progress(
                    "Writing objects",
                    Some(100),
                    "(3/3), 280 bytes | 280.00 KiB/s, done.",
                ),
            ),
            (
                "Resolving deltas:   0% (0/2)",
                progress("Resolving deltas", Some(0), "(0/2)"),
            ),
            (
                "Updating files:  50% (1/2)",
                progress("Updating files", Some(50), "(1/2)"),
            ),
            ("Cloning into 'repo'...", None),
            ("remote: Total 3 (delta 0), reused 0 (delta 0)", None),
            ("Delta compression using up to 8 threads", None),
            ("From github.com:o/r", None),
            ("To ssh://example.com/r.git", None),
            (" * [new branch]      main       -> origin/main", None),
            (
                "fatal: unable to access 'https://x/': Could not resolve host",
                None,
            ),
            ("error: 3 files would be overwritten", None),
            ("hint: Updates were rejected", None),
            ("", None),
            ("remote:", None),
        ];
        for (line, want) in cases {
            assert_eq!(parse_line(line), want, "{line:?}");
        }
    }

    #[test]
    fn a_label_says_the_phase_and_how_far() {
        let p = parse_line("Receiving objects:  45% (450/1000)");
        assert_eq!(
            p.as_ref().map(Progress::label).as_deref(),
            Some("Receiving objects: 45%")
        );
        assert_eq!(p.and_then(|p| p.fraction()), Some(0.45));
        let counting = parse_line("remote: Enumerating objects: 12");
        assert_eq!(
            counting.as_ref().map(Progress::label).as_deref(),
            Some("Enumerating objects")
        );
        assert_eq!(counting.and_then(|p| p.fraction()), None);
        // A percentage over 100 (never seen, but bytes are bytes) stays at 100.
        let over = parse_line("Receiving objects: 250% (1/1)");
        assert_eq!(over.and_then(|p| p.percent), Some(100));
    }

    /// Output as git writes it on a pipe: redraws with `\r`, in chunks cut anywhere (in the
    /// middle of a multi-byte character too).
    #[test]
    fn redrawn_lines_are_kept_as_last_drawn_and_progress_follows() {
        let raw = "Cloning into 'répo ü'...\n\
                   remote: Enumerating objects: 3, done.\n\
                   Receiving objects:  33% (1/3)\rReceiving objects:  66% (2/3)\r\
                   Receiving objects: 100% (3/3), done.\n\
                   Resolving deltas:  50% (1/2)\r";
        let bytes = raw.as_bytes();
        let mut reader = ProgressReader::default();
        let mut seen = Vec::new();
        for chunk in bytes.chunks(3) {
            if reader.feed(chunk) {
                seen.extend(reader.latest().map(Progress::label));
            }
        }
        assert_eq!(
            seen,
            [
                "Enumerating objects",
                "Receiving objects: 33%",
                "Receiving objects: 66%",
                "Receiving objects: 100%",
                "Resolving deltas: 50%",
            ]
        );
        assert_eq!(
            reader.finish(),
            "Cloning into 'répo ü'...\n\
             remote: Enumerating objects: 3, done.\n\
             Receiving objects: 100% (3/3), done.\n\
             Resolving deltas:  50% (1/2)\n"
        );
    }

    #[test]
    fn a_marked_line_is_taken_apart_once() {
        let mut reader = ProgressReader::with_marker("__PID__");
        reader.feed(b"warning: x\n__PID__ 4242\nfatal: y\n__PID__ 7\n");
        assert_eq!(reader.marked(), Some("4242"));
        assert_eq!(reader.finish(), "warning: x\nfatal: y\n");
    }

    #[test]
    fn errors_are_read_without_the_progress_before_them() {
        let text = "remote: Counting objects: 100% (5/5), done.\n\
                    Receiving objects:  10% (1/10)\n\
                    fatal: the remote end hung up unexpectedly";
        assert_eq!(
            without_progress(text),
            "fatal: the remote end hung up unexpectedly"
        );
        assert_eq!(without_progress(""), "");
    }

    #[test]
    fn huge_output_keeps_its_end() {
        let mut reader = ProgressReader::default();
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..(MAX_TEXT / 1024 + 10) {
            reader.feed(line.as_bytes());
        }
        reader.feed(b"the end\n");
        let text = reader.finish();
        assert!(text.starts_with("(earlier output left out)\n"));
        assert!(text.ends_with("the end\n"));
        assert!(text.len() <= MAX_TEXT + 64);
        // An empty output stays empty.
        assert_eq!(ProgressReader::default().finish(), "");
    }
}
