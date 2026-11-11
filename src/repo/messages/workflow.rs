//! Exact workflow paint contracts supplied at immutable 32a7cb12.
//! No producer/control/command identities are changed. Source-owned version headings,
//! test-lane results and push headlines retain their typed owner paint boundaries.

mod templates;
pub(super) use templates::ROWS as TEMPLATES;

mod adapters;
pub(super) use adapters::*;

fn decimal(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

// Three supplied whole producer variants require explicit selector/default bindings.
// The delimiters are literal application envelopes; every inserted ref/name stays opaque.
pub(super) fn special(raw: &str) -> Option<String> {
    if let Some(body) = raw.strip_suffix(": Ctrl+Z puts them back as they were")
        && let Some((branch, target)) =
            body.split_once("'s commits are signed off here, but not pushed")
    {
        let (target, remote) = if target.is_empty() {
            ("no", "")
        } else {
            ("yes", target.strip_prefix(" to ")?)
        };
        return Some(crate::i18n::message(
            "repo-workflows-status-dco-local-only",
            &[
                ("branch", branch.into()),
                ("target", target.into()),
                ("remote", remote.into()),
            ],
        ));
    }
    let (kept, names) = if raw == "No fork remote can go" {
        ("no", "")
    } else {
        let names = raw
            .strip_prefix("No fork remote can go\n\nKept: ")?
            .strip_suffix(", whose branches have commits no branch here has.")?;
        ("yes", names)
    };
    Some(crate::i18n::message(
        "repo-workflows-status-forks-none-to-remove",
        &[("kept", kept.into()), ("names", names.into())],
    ))
}

/// Finite legacy receipt grammar; unknown or reordered clauses do not become operations.
/// This only adapts the supplied timeline-receipt argument of a recognized status wrapper.
pub(super) fn receipt(raw: &str) -> Option<String> {
    let mut clauses = raw.split(", ");
    let first_id = match clauses.next()? {
        "Final files identical" => "repo-workflows-timeline-sentence-31b36481",
        "Final files differ" => "repo-workflows-timeline-sentence-99e79f74",
        _ => return None,
    };
    let mut rendered = vec![crate::i18n::text(first_id).to_owned()];
    let mut rank = 0;
    let mut signature_replaced = false;
    for clause in clauses {
        if clause == "too many commits to follow one by one" {
            if rank != 0 {
                return None;
            }
            rank = 5;
            rendered
                .push(crate::i18n::text("repo-workflows-timeline-sentence-4a61e684").to_owned());
            continue;
        }
        if signature_replaced {
            for (suffix, id, name) in [
                (
                    " dropped",
                    "repo-workflows-timeline-signatures-b59d95ba",
                    "b-a-1",
                ),
                (
                    " more signed",
                    "repo-workflows-timeline-signatures-33c87627",
                    "a-b-1",
                ),
            ] {
                if let Some(number) = clause
                    .strip_suffix(suffix)
                    .and_then(decimal)
                    .filter(|n| *n > 0)
                {
                    rendered.pop();
                    rendered.push(crate::i18n::message(id, &[(name, number.into())]));
                    signature_replaced = false;
                    break;
                }
            }
            if clause.ends_with(" dropped") || clause.ends_with(" more signed") {
                if signature_replaced {
                    return None;
                }
                continue;
            }
        }
        signature_replaced = false;
        let fixed = match clause {
            "authors kept" => Some((1, "repo-workflows-timeline-sentence-d30ebcc6")),
            "messages kept" => Some((2, "repo-workflows-timeline-sentence-360c0cec")),
            "signatures replaced" => {
                signature_replaced = true;
                Some((3, "repo-workflows-timeline-signatures-aa14a1ba"))
            }
            _ => None,
        };
        let (next_rank, text) = if let Some((next, id)) = fixed {
            (next, crate::i18n::text(id).to_owned())
        } else if let Some((old, new)) = clause.split_once(" commits → ") {
            (
                4,
                crate::i18n::message(
                    "repo-workflows-timeline-sentence-b5ea0271",
                    &[("old", decimal(old)?.into()), ("new", decimal(new)?.into())],
                ),
            )
        } else {
            let mut known = None;
            for (one, many, id, next) in [
                (
                    " author changed",
                    " authors changed",
                    "repo-workflows-timeline-authors-changed",
                    1,
                ),
                (
                    " message changed",
                    " messages changed",
                    "repo-workflows-timeline-messages-changed",
                    2,
                ),
                (
                    " commit now signed",
                    " commits now signed",
                    "repo-workflows-timeline-commits-signed",
                    3,
                ),
                (
                    " signature dropped",
                    " signatures dropped",
                    "repo-workflows-timeline-signatures-dropped",
                    3,
                ),
            ] {
                for (suffix, singular) in [(one, true), (many, false)] {
                    if let Some(number) = clause
                        .strip_suffix(suffix)
                        .and_then(decimal)
                        .filter(|n| *n > 0)
                        && (number == 1) == singular
                    {
                        known = Some((next, crate::i18n::message(id, &[("count", number.into())])));
                    }
                }
            }
            known?
        };
        if next_rank <= rank {
            return None;
        }
        rank = next_rank;
        rendered.push(text);
    }
    let sentence = rendered.join(", ");
    let mut chars = sentence.chars();
    Some(match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => sentence,
    })
}

#[cfg(test)]
mod tests;

pub(super) fn shared_fixed(raw: &str) -> Option<String> {
    match raw {
        "Fetch" => Some(crate::operation_labels::display(raw)),
        "Pull" => Some(crate::operation_labels::display(raw)),
        "Push" => Some(crate::operation_labels::display(raw)),
        "Commit" => Some(crate::operation_labels::display(raw)),
        "Amend" => Some(crate::operation_labels::display(raw)),
        "Stage" => Some(crate::operation_labels::display(raw)),
        "Stage all" => Some(crate::operation_labels::display(raw)),
        "Unstage" => Some(crate::operation_labels::display(raw)),
        "Unstage all" => Some(crate::operation_labels::display(raw)),
        "Discard" => Some(crate::operation_labels::display(raw)),
        "Stash" => Some(crate::operation_labels::display(raw)),
        "Pop stash" => Some(crate::operation_labels::display(raw)),
        "Apply stash" => Some(crate::operation_labels::display(raw)),
        "Drop stash" => Some(crate::operation_labels::display(raw)),
        "Set identity" => Some(crate::operation_labels::display(raw)),
        "Use the profile's author" => Some(crate::operation_labels::display(raw)),
        "Mark resolved" => Some(crate::operation_labels::display(raw)),
        "Reload" => Some(crate::operation_labels::display(raw)),
        "Sync" => Some(crate::operation_labels::display(raw)),
        "Push for review" => Some(crate::operation_labels::display(raw)),
        "Push, skipping hooks" => Some(crate::operation_labels::display(raw)),
        "Apply patch" => Some(crate::operation_labels::display(raw)),
        "Cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Revert" => Some(crate::operation_labels::display(raw)),
        "Rebase" => Some(crate::operation_labels::display(raw)),
        "Merge" => Some(crate::operation_labels::display(raw)),
        "DVC: Pull" => Some(crate::operation_labels::display(raw)),
        "Abort merge" => Some(crate::operation_labels::display(raw)),
        "Abort rebase" => Some(crate::operation_labels::display(raw)),
        "Abort cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Abort revert" => Some(crate::operation_labels::display(raw)),
        "Abort am" => Some(crate::operation_labels::display(raw)),
        "Continue merge" => Some(crate::operation_labels::display(raw)),
        "Continue rebase" => Some(crate::operation_labels::display(raw)),
        "Continue cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Continue revert" => Some(crate::operation_labels::display(raw)),
        "Continue am" => Some(crate::operation_labels::display(raw)),
        _ => None,
    }
}

/// Reassemble domain-owned rows without changing precedence or public slice types.
pub(super) const fn ordered<T: Copy, const N: usize>(groups: &[&[(usize, T)]]) -> [T; N] {
    let mut result = [groups[0][0].1; N];
    let mut seen = [false; N];
    let mut group = 0;
    let mut count = 0;
    while group < groups.len() {
        let mut row = 0;
        while row < groups[group].len() {
            let (index, value) = groups[group][row];
            assert!(!seen[index]);
            seen[index] = true;
            result[index] = value;
            count += 1;
            row += 1;
        }
        group += 1;
    }
    assert!(count == N);
    result
}

#[cfg(test)]
#[path = "workflow/assembly_tests.rs"]
mod assembly_tests;
