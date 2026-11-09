//! Display known across-repository errors after their raw classification.
use crate::i18n::{message, text};

pub fn display_error(raw: &str) -> String {
    let id = match raw {
        "couldn't tell where your home folder is" => Some("interaction-across-home"),
        "stopped unexpectedly" => Some("interaction-across-stopped"),
        "the repository has no remote to push to" => Some("interaction-across-no-remote"),
        "there is no origin, and several remotes: open the repository to choose one" => {
            Some("interaction-across-no-origin")
        }
        _ => None,
    };
    if let Some(id) = id {
        return text(id).to_owned();
    }
    for kind in ["look", "fetch", "push"] {
        if raw == format!("the last {kind} of it is still running; try again once it ends") {
            return message("interaction-across-busy", &[("kind", kind.into())]);
        }
    }
    if let Some(duration) = raw
        .strip_prefix("no answer within ")
        .and_then(|s| s.strip_suffix("; it may still be running"))
    {
        if duration == "1 minute" {
            return text("interaction-across-timeout-one-minute").to_owned();
        }
        if duration == "1 second" {
            return text("interaction-across-timeout-one-second").to_owned();
        }
        if let Some((count, unit)) = duration.split_once(' ') {
            let unit = match unit {
                "minutes" => Some("minute"),
                "seconds" => Some("second"),
                _ => None,
            };
            if let (Ok(count), Some(unit)) = (count.parse::<u64>(), unit) {
                return message(
                    "interaction-across-timeout",
                    &[("count", count.into()), ("unit", unit.into())],
                );
            }
        }
    }
    if let Some((branch, tail)) = raw.split_once(" uses a separate push branch (")
        && branch_name(branch)
        && let Some(destination) = tail.strip_suffix("): open the repository to push it")
    {
        return message(
            "interaction-across-separate",
            &[
                ("branch", branch.into()),
                ("destination", destination.into()),
            ],
        );
    }
    if let Some(branch) = raw.strip_suffix(" tracks a local branch: there is nothing to push it to")
        && branch_name(branch)
    {
        return message("interaction-across-local", &[("branch", branch.into())]);
    }
    if let Some((branch, tail)) = raw.split_once(" tracks ")
        && branch_name(branch)
        && let Some(upstream) = tail.strip_suffix(
            ", a branch of another name: open the repository to push it where it belongs",
        )
    {
        return message(
            "interaction-across-other",
            &[("branch", branch.into()), ("upstream", upstream.into())],
        );
    }
    if let Some(branch) = raw.strip_suffix(
        ": its remote branch is gone (deleted there?): open the repository to push it again",
    ) && branch_name(branch)
    {
        return message("interaction-across-gone", &[("branch", branch.into())]);
    }
    if let Some(branch) = raw.strip_suffix(" has no upstream set any more")
        && branch_name(branch)
    {
        return message("interaction-across-upstream", &[("branch", branch.into())]);
    }
    if let Some((branch, tail)) = raw.split_once(" is set to be pushed to ")
        && branch_name(branch)
        && let Some(tail) = tail.strip_suffix(": open the repository to push it")
        && let Some((destination, remote)) = tail.split_once(", not ")
    {
        return message(
            "interaction-across-destination",
            &[
                ("branch", branch.into()),
                ("destination", destination.into()),
                ("remote", remote.into()),
            ],
        );
    }
    if let Some(tail) = raw.strip_prefix("couldn't search ")
        && let Some((machine, reason)) = tail.split_once(": ")
    {
        return message(
            "interaction-across-search",
            &[
                ("machine", machine.into()),
                ("reason", crate::git::display_error(reason).into()),
            ],
        );
    }
    crate::git::display_error(raw)
}

fn branch_name(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_push_and_search_envelopes_localize_without_rewriting_raw_diagnostics() {
        crate::i18n::set_language("de");
        for (raw, expected) in [
            (
                "topic/日本語 uses a separate push branch (publish/日本語): open the repository to push it",
                "topic/日本語 verwendet einen eigenen Push-Branch (publish/日本語): Öffne das Repository zum Pushen",
            ),
            (
                "topic tracks a local branch: there is nothing to push it to",
                "topic folgt einem lokalen Branch: Es gibt kein Ziel zum Pushen",
            ),
            (
                "topic tracks origin/main, a branch of another name: open the repository to push it where it belongs",
                "topic folgt origin/main, einem anders benannten Branch: Öffne das Repository, um an die richtige Stelle zu pushen",
            ),
            (
                "topic: its remote branch is gone (deleted there?): open the repository to push it again",
                "topic: Der Remote-Branch ist verschwunden (dort gelöscht?): Öffne das Repository, um erneut zu pushen",
            ),
            (
                "topic is set to be pushed to mirror, not origin: open the repository to push it",
                "topic soll nach mirror statt nach origin gepusht werden: Öffne das Repository zum Pushen",
            ),
            (
                "couldn't search SSH · build: Remote shell diagnostic 日本語",
                "SSH · build konnte nicht durchsucht werden: Remote shell diagnostic 日本語",
            ),
            (
                "no answer within 1234 seconds; it may still be running",
                "Keine Antwort innerhalb von 1.234 Sekunden; der Vorgang läuft möglicherweise noch",
            ),
            (
                "no answer within 1 minute; it may still be running",
                "Keine Antwort innerhalb von 1 Minute; der Vorgang läuft möglicherweise noch",
            ),
            (
                "no answer within 1 second; it may still be running",
                "Keine Antwort innerhalb von 1 Sekunde; der Vorgang läuft möglicherweise noch",
            ),
            (
                "no answer within 3 minutes; it may still be running",
                "Keine Antwort innerhalb von 3 Minuten; der Vorgang läuft möglicherweise noch",
            ),
        ] {
            assert_eq!(display_error(raw), expected);
        }
        for (kind, expected) in [
            ("look", "Die letzte Prüfung"),
            ("fetch", "Der letzte Abruf"),
            ("push", "Der letzte Push"),
        ] {
            let raw = format!("the last {kind} of it is still running; try again once it ends");
            assert!(display_error(&raw).starts_with(expected));
            assert_eq!(super::super::short_reason(&raw), "still busy");
        }
        for raw in [
            "External output 日本語",
            "unknown task has no upstream set any more",
            "external prefix topic tracks a local branch: there is nothing to push it to",
        ] {
            assert_eq!(display_error(raw), raw);
        }
        crate::i18n::set_language("en-US");
    }
}
