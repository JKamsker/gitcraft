use super::*;

pub(super) fn is_marker(line: &str, kind: char) -> bool {
    let mut chars = line.chars();
    (&mut chars).take(7).filter(|&c| c == kind).count() == 7
        && matches!(chars.next(), None | Some(' '))
}

/// Splits a file with conflict markers into common text and conflicts (diff3 bases kept).
/// A conflict that never ends (no `>>>>>>>` before the end of the file) stays text.
pub(in crate::repo) fn parse(text: &str) -> (Vec<Segment>, String, String) {
    enum State {
        Common,
        Ours,
        Base,
        Theirs,
    }
    let mut segments = Vec::new();
    let (mut common, mut ours, mut base, mut theirs) =
        (String::new(), String::new(), None::<String>, String::new());
    let (mut ours_label, mut theirs_label) = (String::new(), String::new());
    // The lines of the conflict being read, markers included, in case it never ends.
    let mut raw = String::new();
    let mut state = State::Common;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        if !matches!(state, State::Common) {
            raw.push_str(line);
        }
        match state {
            State::Common if is_marker(bare, '<') => {
                raw.clear();
                raw.push_str(line);
                if !common.is_empty() {
                    segments.push(Segment::Common(std::mem::take(&mut common)));
                }
                if ours_label.is_empty() {
                    ours_label = bare.trim_start_matches('<').trim().to_owned();
                }
                state = State::Ours;
            }
            State::Common => common.push_str(line),
            State::Ours if is_marker(bare, '|') => {
                base = Some(String::new());
                state = State::Base;
            }
            State::Ours | State::Base if is_marker(bare, '=') => state = State::Theirs,
            State::Ours => ours.push_str(line),
            State::Base => {
                if let Some(b) = &mut base {
                    b.push_str(line);
                }
            }
            State::Theirs if is_marker(bare, '>') => {
                if theirs_label.is_empty() {
                    theirs_label = bare.trim_start_matches('>').trim().to_owned();
                }
                raw.clear();
                segments.push(Segment::Conflict {
                    ours: std::mem::take(&mut ours),
                    base: base.take(),
                    theirs: std::mem::take(&mut theirs),
                });
                state = State::Common;
            }
            State::Theirs => theirs.push_str(line),
        }
    }
    if !matches!(state, State::Common) {
        // Not a conflict after all: keep the text as it was rather than lose it.
        common.push_str(&raw);
    }
    if !common.is_empty() {
        segments.push(Segment::Common(common));
    }
    (segments, ours_label, theirs_label)
}

/// The file for the current choices (`recorded`: the text of each [`Choice::Recorded`]);
/// undecided conflicts keep their markers.
pub(super) fn render(
    segments: &[Segment],
    choices: &[Option<Choice>],
    recorded: &[Option<String>],
    labels: (&str, &str),
) -> String {
    let mut out = String::new();
    let mut k = 0;
    for seg in segments {
        match seg {
            Segment::Common(text) => out.push_str(text),
            Segment::Conflict { ours, base, theirs } => {
                let recorded = recorded.get(k).and_then(Option::as_deref);
                match choices.get(k).copied().flatten() {
                    Some(Choice::Recorded) if recorded.is_some() => {
                        out.push_str(recorded.unwrap_or_default());
                    }
                    Some(Choice::Ours) => out.push_str(ours),
                    Some(Choice::Theirs) => out.push_str(theirs),
                    Some(Choice::OursThenTheirs) => {
                        out.push_str(ours);
                        out.push_str(theirs);
                    }
                    Some(Choice::TheirsThenOurs) => {
                        out.push_str(theirs);
                        out.push_str(ours);
                    }
                    None | Some(Choice::Recorded) => {
                        out.push_str(&format!("<<<<<<< {}\n", labels.0));
                        out.push_str(ours);
                        if let Some(base) = base {
                            out.push_str("||||||| base\n");
                            out.push_str(base);
                        }
                        out.push_str("=======\n");
                        out.push_str(theirs);
                        out.push_str(&format!(">>>>>>> {}\n", labels.1));
                    }
                }
                k += 1;
            }
        }
    }
    out
}

/// The lines of `text` that look like one of git's conflict markers.
pub(super) fn marker_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| ['<', '|', '=', '>'].iter().any(|&kind| is_marker(l, kind)))
}

/// How many conflicts are left in `result`: each whole one, and what is left of those taken
/// apart by hand (a marker deleted, the others kept). `segments`: the file as it was parsed,
/// whose marker-like lines that are text (a start marker that never closed, a heading's
/// `=======` on one side) may stay.
pub(super) fn unresolved_in(result: &str, segments: &[Segment]) -> usize {
    use std::collections::HashMap;
    let mut text_markers: HashMap<&str, usize> = HashMap::new();
    for seg in segments {
        let texts = match seg {
            Segment::Common(text) => [Some(text), None, None],
            Segment::Conflict { ours, base, theirs } => [Some(ours), base.as_ref(), Some(theirs)],
        };
        for line in texts.into_iter().flatten().flat_map(|t| marker_lines(t)) {
            *text_markers.entry(line).or_default() += 1;
        }
    }
    let (now, _, _) = parse(result);
    let mut whole = 0;
    // Marker lines outside a whole conflict that the file didn't have as text, by kind.
    let (mut starts, mut middles, mut ends) = (0, 0, 0);
    for seg in &now {
        let text = match seg {
            Segment::Conflict { .. } => {
                whole += 1;
                continue;
            }
            Segment::Common(text) => text,
        };
        for line in marker_lines(text) {
            match text_markers.get_mut(line) {
                Some(n) if *n > 0 => *n -= 1,
                _ if line.starts_with('<') => starts += 1,
                _ if line.starts_with('>') => ends += 1,
                _ => middles += 1,
            }
        }
    }
    // A conflict taken apart keeps its start or its end marker, or at least a middle one.
    whole + starts.max(ends).max(usize::from(middles > 0))
}

// Whether `file` is a symbolic link (not what it points to).
