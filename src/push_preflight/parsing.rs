//! Single-ref command grammar and immutable explicit lease expectations.

pub(crate) fn has_push(ops: &[Vec<String>]) -> bool {
    ops.iter().any(|op| push_words(op).is_some())
}

pub(super) fn push_words(op: &[String]) -> Option<&[String]> {
    let mut words = op;
    while matches!(words.first().map(String::as_str), Some("-c")) {
        words = words.get(2..)?;
    }
    (words.first()?.as_str() == "push").then_some(&words[1..])
}

type ParsedPush = (
    Option<String>,
    Option<String>,
    bool,
    bool,
    Vec<String>,
    bool,
);

pub(super) fn parse(words: &[String]) -> Result<ParsedPush, String> {
    let (mut positional, mut force, mut tracking, mut extras) =
        (Vec::new(), false, false, Vec::new());
    let mut ended = false;
    let mut deleting = false;
    for word in words {
        if !ended && word == "--" {
            ended = true;
            continue;
        }
        if !ended && word.starts_with('-') {
            match word.as_str() {
                "-u" | "--set-upstream" => tracking = true,
                "-f" | "--force" | "--force-with-lease" => force = true,
                "--no-follow-tags" | "--porcelain" | "--no-verify" => {},
                "--recurse-submodules=check" => extras.push(word.clone()),
                "--delete" | "-d" => deleting = true,
                _ if word.starts_with("--force-with-lease=") => force = true,
                _ if word.starts_with("--push-option=") => extras.push(word.clone()),
                _ => return Err("This push option needs an explicit single-ref preview; multi-ref, matching, mirror, atomic and recursive pushes are not sent by this checklist.".into()),
            }
        } else {
            positional.push(word.clone());
        }
    }
    if positional.len() > 2 {
        return Err("Preview one destination at a time; no push was run.".into());
    }
    Ok((
        positional.first().cloned(),
        positional.get(1).cloned(),
        force,
        tracking,
        extras,
        deleting,
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CapturedLease {
    Absent,
    Object(String),
}

/// Shorthand leases still use this destination's fetched tip. Explicit absent
/// and object expectations constrain it and may never contradict one another.
pub(super) fn lease_requirement(
    words: &[String],
    destination: &str,
    deleting: bool,
) -> Result<Option<CapturedLease>, String> {
    let mut captured = None;
    for lease in words
        .iter()
        .take_while(|word| word.as_str() != "--")
        .filter_map(|arg| arg.strip_prefix("--force-with-lease="))
    {
        let Some((reference, expected)) = lease.split_once(':') else {
            if deleting {
                return Err("Deletion needs an exact ref lease".into());
            }
            if lease != destination {
                return Err("The push lease must name this exact ref.".into());
            }
            continue; // DCO's tracked shorthand keeps the fetched-tip semantics.
        };
        if reference != destination
            || (deleting && expected.is_empty())
            || (!expected.is_empty() && !valid_object_id(expected))
        {
            return Err(if deleting {
                "The deletion lease must name this exact ref and object."
            } else {
                "The push lease must name this exact ref and object."
            }
            .into());
        }
        let requirement = if expected.is_empty() {
            CapturedLease::Absent
        } else {
            CapturedLease::Object(expected.to_owned())
        };
        if captured.as_ref().is_some_and(|prior| prior != &requirement) {
            return Err(
                "The push contains contradictory explicit leases; reopen its original action."
                    .into(),
            );
        }
        captured = Some(requirement);
    }
    Ok(captured)
}

/// An action's explicit expectation is immutable across scan/reload/approval.
/// Tracking data may corroborate it, never replace it with a newer object.
pub(super) fn captured_lease(
    words: &[String],
    destination: &str,
    base: Option<String>,
    deleting: bool,
) -> Result<Option<String>, String> {
    let Some(requirement) = lease_requirement(words, destination, deleting)? else {
        return Ok(base);
    };
    let expected = match requirement {
        CapturedLease::Absent => None,
        CapturedLease::Object(expected) => Some(expected),
    };
    if base
        .as_deref()
        .is_some_and(|tip| Some(tip) != expected.as_deref())
    {
        return Err(if deleting {
            "The fetched target differs from the deletion lease; reopen its original action."
        } else {
            "The fetched target differs from the captured push lease; reopen its original action."
        }
        .into());
    }
    Ok(expected)
}

pub(super) fn companion_branch(op: &[String]) -> Option<&str> {
    match op {
        [command, flag, end, name]
            if command == "branch" && matches!(flag.as_str(), "-d" | "-D") && end == "--" =>
        {
            Some(name)
        }
        [command, end, name, oid] if command == "branch" && end == "--" && valid_object_id(oid) => {
            Some(name)
        }
        [command, upstream, end, name]
            if command == "branch" && upstream.starts_with("--set-upstream-to=") && end == "--" =>
        {
            Some(name)
        }
        [command, key, _] if command == "config" => key
            .strip_prefix("branch.")
            .and_then(|key| key.rsplit_once('.'))
            .and_then(|(name, option)| matches!(option, "remote" | "merge").then_some(name)),
        _ => None,
    }
}

pub(super) fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

// The internals branch picker extends only implicit plain pushes. Explicit refs
// and workflow-reviewed plans keep the exact root plan chosen by their caller.
pub(super) fn saved_branch_spec(current: &str, saved: &str, implicit: bool) -> Option<String> {
    (implicit && !current.is_empty() && !saved.is_empty())
        .then(|| format!("refs/heads/{current}:refs/heads/{saved}"))
}

/// The scanner's existing operation gate, also usable with supplied recording argv.
/// Rewriting commands are deliberately excluded: only the completed tip is reviewed.
pub(crate) fn validate_operations(original: &[Vec<String>]) -> Result<(), String> {
    for op in original {
        if push_words(op).is_some() {
            let mut prefix = op.as_slice();
            while let [flag, value, rest @ ..] = prefix {
                if flag != "-c" {
                    break;
                }
                if value != "push.default=simple" {
                    return Err(
                        "Preview explicit refs without temporary push configuration overrides."
                            .into(),
                    );
                }
                prefix = rest;
            }
        } else if !op.first().is_some_and(|v| v.starts_with('\0')) && companion_branch(op).is_none()
        {
            return Err(
                "Complete this local operation first, then preview its push separately.".into(),
            );
        }
    }
    Ok(())
}
