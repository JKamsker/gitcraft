//! Finite presentation of launch diagnostics. Parser/protocol/error producers remain raw.
pub fn message(raw: &str) -> Option<String> {
    if raw == super::args::USAGE {
        return Some(super::args::usage().to_owned());
    }
    // Only the complete original help suffix identifies a parser usage failure.
    if let Some(cause) = raw.strip_suffix(&format!("\n\n{}", super::args::USAGE)) {
        let reason = if let Some(flag) = cause.strip_prefix("unknown option ") {
            crate::i18n::message(
                "interaction-launch-unknown-option",
                &[("flag", flag.into())],
            )
        } else {
            simple(cause)?
        };
        return Some(crate::i18n::message(
            "interaction-launch-with-usage",
            &[
                ("reason", reason.into()),
                ("usage", super::args::usage().into()),
            ],
        ));
    }
    if let Some(said) = simple(raw) {
        return Some(said);
    }
    if let Some(url) = raw.strip_suffix(" is not a link gitcraft can open: it is too long") {
        return Some(crate::i18n::message(
            "interaction-launch-web-0",
            &[("url", url.into())],
        ));
    }
    if let Some(url) =
        raw.strip_suffix(" is not a link gitcraft can open: it has spaces or control characters")
    {
        return Some(crate::i18n::message(
            "interaction-launch-web-1",
            &[("url", url.into())],
        ));
    }
    if let Some(url) = raw.strip_suffix(
        " is not a link gitcraft can open: only https:// (or http://) links are opened",
    ) {
        return Some(crate::i18n::message(
            "interaction-launch-web-2",
            &[("url", url.into())],
        ));
    }
    if let Some(url) =
        raw.strip_suffix(" is not a link gitcraft can open: its host isn't a host name")
    {
        return Some(crate::i18n::message(
            "interaction-launch-web-3",
            &[("url", url.into())],
        ));
    }
    if let Some(url) =
        raw.strip_suffix(" is not a link gitcraft can open: its path isn't a forge's")
    {
        return Some(crate::i18n::message(
            "interaction-launch-web-4",
            &[("url", url.into())],
        ));
    }
    if let Some(url) = raw.strip_suffix(
        " is not a link gitcraft can open: it doesn't point at a repository, pull request, commit or file",
    ) {
        return Some(crate::i18n::message("interaction-launch-web-5", &[("url", url.into())]));
    }
    if let Some(value) = raw.strip_suffix(" is not a line number (the first line is 1)")
        && value.bytes().all(|b| b.is_ascii_digit())
        && !value.is_empty()
    {
        return Some(crate::i18n::message(
            "interaction-launch-line",
            &[("value", value.into())],
        ));
    }
    if let Some(value) = raw.strip_prefix("Name the two commits as a..b (or a...b), not ") {
        return Some(crate::i18n::message(
            "interaction-launch-compare",
            &[("value", value.into())],
        ));
    }
    if let Some(value) = raw.strip_suffix(" is not a commit: it starts with -")
        && value.starts_with('-')
    {
        return Some(crate::i18n::message(
            "interaction-launch-rev-option",
            &[("value", value.into())],
        ));
    }
    if let Some(value) = raw.strip_suffix(" is not a commit name") {
        return Some(crate::i18n::message(
            "interaction-launch-rev-name",
            &[("value", value.into())],
        ));
    }
    if let Some(value) = raw.strip_suffix(" is not a file") {
        return Some(crate::i18n::message(
            "interaction-launch-file",
            &[("path", value.into())],
        ));
    }
    if let Some(value) = raw.strip_prefix("No such folder: ") {
        return Some(crate::i18n::message(
            "interaction-launch-folder",
            &[("path", value.into())],
        ));
    }
    if let Some(value) = raw.strip_suffix(
        ": finish or close the conflict or rebase editor first, then run the command again",
    ) {
        return Some(crate::i18n::message(
            "interaction-launch-editor",
            &[("name", value.into())],
        ));
    }
    let separator = " is a folder: give a file (or open the folder with gitcraft ";
    for (at, _) in raw.match_indices(separator) {
        let path = &raw[..at];
        if raw == format!("{path}{separator}{path})") {
            return Some(crate::i18n::message(
                "interaction-launch-directory",
                &[("path", path.into())],
            ));
        }
    }
    if let Some(body) = raw.strip_suffix(": fetch, then open the link again")
        && body.matches(" isn't in ").count() == 1
        && let Some((item, root)) = body.split_once(" isn't in ")
    {
        if let Some(hash) = item
            .strip_prefix("Commit ")
            .filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Some(crate::i18n::message(
                "interaction-launch-missing-linked-commit",
                &[("hash", hash.into()), ("root", root.into())],
            ));
        }
        return Some(crate::i18n::message(
            "interaction-launch-missing-linked-file",
            &[("path", item.into()), ("root", root.into())],
        ));
    }
    linked_pull_request(raw)
}

// Repeated repository fields are accepted only when one complete reconstruction matches.
fn linked_pull_request(raw: &str) -> Option<String> {
    let body = raw.strip_suffix(" itself")?;
    let separator = " on the forge, or clone ";
    let mut found = None;
    for (at, _) in body.match_indices(separator) {
        let repo = &body[at + separator.len()..];
        let Some((header, n)) = body[..at].rsplit_once(" open #") else {
            continue;
        };
        if repo.is_empty()
            || n.is_empty()
            || n.len() > 12
            || !n.bytes().all(|b| b.is_ascii_digit())
            || !n.parse::<u64>().is_ok_and(|n| n > 0)
        {
            continue;
        }
        let suffix = format!(
            " has {repo} only as another remote than origin, whose pull requests it shows:"
        );
        let Some(root) = header.strip_suffix(&suffix) else {
            continue;
        };
        if found.is_some() {
            return None;
        }
        found = Some((root, repo, n));
    }
    let (root, repo, n) = found?;
    Some(crate::i18n::message(
        "interaction-launch-linked-pr",
        &[
            ("root", root.into()),
            ("repo", repo.into()),
            ("number", n.into()),
        ],
    ))
}

fn simple(raw: &str) -> Option<String> {
    let id = match raw {
        "The commits to compare are not valid text" => "interaction-launch-commits-text",
        "The commit is not valid text" => "interaction-launch-commit-text",
        "No commit given" => "interaction-launch-no-commit",
        "too many arguments" => "interaction-launch-too-many",
        "log takes a file" => "interaction-launch-log-count",
        "blame takes a file, and :<line> if you like" => "interaction-launch-blame-count",
        "show takes a commit" => "interaction-launch-show-count",
        "compare takes two commits, as a..b" => "interaction-launch-compare-count",
        "This command opens a dialog in the window" => "interaction-launch-dialog",
        "a broken %-escape" => "interaction-launch-percent",
        "it isn't valid text" => "interaction-launch-not-text",
        "Not a gitcraft link gitcraft can open (too long)" => "interaction-launch-scheme-0",
        "Not a gitcraft link gitcraft can open (no gitcraft://)" => "interaction-launch-scheme-1",
        "Not a gitcraft link gitcraft can open (no ?url=)" => "interaction-launch-scheme-2",
        "Not a gitcraft link gitcraft can open (it only opens: gitcraft://open?url=…)" => {
            "interaction-launch-scheme-3"
        }
        "Not a gitcraft link gitcraft can open (url is given twice)" => {
            "interaction-launch-scheme-4"
        }
        "Not a gitcraft link gitcraft can open (only url= is taken)" => {
            "interaction-launch-scheme-5"
        }
        "Not a gitcraft link gitcraft can open (no url=)" => "interaction-launch-scheme-6",
        _ => return None,
    };
    Some(crate::i18n::text(id).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::{Parsed, args, link, open_link, resolve};
    use std::ffi::OsString;

    #[test]
    fn actual_argument_and_link_parsers_keep_raw_results_while_presentation_changes()
    -> Result<(), String> {
        let cases: &[(&[&str], &str)] = &[
            (&["log"], "log erwartet eine Datei"),
            (&["blame"], "blame erwartet eine Datei und optional :<line>"),
            (&["show"], "show erwartet einen Commit"),
            (&["compare"], "compare erwartet zwei Commits als a..b"),
            (&["--unknown日本語"], "unbekannte Option --unknown日本語"),
            (&["one", "two"], "zu viele Argumente"),
            (
                &["blame", "日本語:0"],
                "0 ist keine Zeilennummer (die erste Zeile ist 1)",
            ),
            (
                &["compare", "日本語"],
                "Gib die beiden Commits als a..b (oder a...b) an, statt 日本語",
            ),
            (
                &["show", "-日本語"],
                "-日本語 ist kein Commit: Die Angabe beginnt mit -",
            ),
            (&["show", "has space"], "has space ist kein Commit-Name"),
            (&["show", ""], "Kein Commit angegeben"),
        ];
        for (input, german) in cases {
            let input: Vec<OsString> = input.iter().map(OsString::from).collect();
            let parsed = args::parse(input.clone());
            let Parsed::Fail(raw) = &parsed else {
                return Err(format!("expected parser failure: {input:?}"));
            };
            crate::i18n::set_language("de");
            let displayed = message(raw).ok_or_else(|| format!("unmapped actual error: {raw}"))?;
            assert!(displayed.starts_with(german), "{displayed}");
            if raw.ends_with(args::USAGE) {
                assert!(displayed.ends_with(args::usage()));
                assert!(displayed.contains("Aufruf:"));
            }
            assert_eq!(args::parse(input), parsed);
            crate::i18n::set_language("en-US");
            assert_eq!(message(raw).as_ref(), Some(raw));
        }
        let scheme_inputs = [
            "bad",
            "gitcraft://open",
            "gitcraft://run?url=x",
            "gitcraft://open?url=x&url=y",
            "gitcraft://open?x=y",
            "gitcraft://open?url=%Q0",
            "gitcraft://open?url=%FF",
        ];
        for input in scheme_inputs {
            let raw = link::parse_scheme_link(input)
                .err()
                .ok_or_else(|| "expected scheme error".to_owned())?;
            crate::i18n::set_language("de");
            let displayed = message(&raw).ok_or_else(|| format!("unmapped: {raw}"))?;
            assert_ne!(displayed, raw);
            assert_eq!(link::parse_scheme_link(input).err().as_ref(), Some(&raw));
            crate::i18n::set_language("en-US");
            assert_eq!(message(&raw), Some(raw));
        }
        for input in [
            "ftp://github.com/o/r",
            "https://bad host/o/r",
            "https://localhost/o/r",
            "https://github.com/o/../r",
            "https://github.com/o/r/unknown",
        ] {
            let raw = link::parse_web_url(input)
                .err()
                .ok_or_else(|| "expected web error".to_owned())?;
            crate::i18n::set_language("de");
            assert!(
                message(&raw)
                    .ok_or_else(|| format!("unmapped: {raw}"))?
                    .starts_with("gitcraft kann den Link ")
            );
            assert_eq!(link::parse_web_url(input).err().as_ref(), Some(&raw));
            crate::i18n::set_language("en-US");
            assert_eq!(message(&raw), Some(raw));
        }
        let long = "x".repeat(4097);
        for raw in [
            link::parse_scheme_link(&long).err(),
            link::parse_web_url(&long).err(),
        ]
        .into_iter()
        .flatten()
        {
            crate::i18n::set_language("de");
            assert_ne!(message(&raw), Some(raw.clone()));
            crate::i18n::set_language("en-US");
            assert_eq!(message(&raw), Some(raw));
        }
        crate::i18n::set_language("de");
        assert_eq!(message(args::USAGE).as_deref(), Some(args::usage()));
        let version = args::parse([OsString::from("--version")]);
        assert_eq!(
            version,
            Parsed::Print(format!("gitcraft {}", env!("CARGO_PKG_VERSION")))
        );
        if let Parsed::Print(raw) = version {
            assert_eq!(message(&raw), None);
        }
        Ok(())
    }

    #[test]
    fn cached_link_resolution_and_known_path_envelopes_keep_original_refs_paths_and_unknown_data()
    -> Result<(), String> {
        let link = link::parse_web_url("https://github.com/o/r/pull/1234")?;
        let root = std::path::PathBuf::from("/opaque/root 日本語");
        let clones = [open_link::Known {
            path: root.clone(),
            remotes: Some(vec![
                crate::git::Remote {
                    name: "origin".into(),
                    url: "https://github.com/fork/r.git".into(),
                },
                crate::git::Remote {
                    name: "upstream".into(),
                    url: "https://github.com/o/r.git".into(),
                },
            ]),
        }];
        // Cached remotes and a pull-request target make this actual resolver path purely in-memory.
        let raw = open_link::resolve_link(&link, &clones)
            .err()
            .ok_or_else(|| "expected other-remote rejection".to_owned())?;
        crate::i18n::set_language("de");
        assert_eq!(message(&raw),Some("/opaque/root 日本語 hat o/r nur als zusätzliches Remote; die Pull Requests werden für origin angezeigt: Öffne #1234 auf der Forge oder klone o/r selbst".into()));
        assert_eq!(
            open_link::resolve_link(&link, &clones).err().as_ref(),
            Some(&raw)
        );
        assert_eq!(clones[0].path, root);
        assert_eq!(link.target, link::Target::PullRequest(1234));
        let dialog =
            resolve::resolve_among(&args::Command::Clone, std::path::Path::new("/opaque"), &[])
                .err()
                .ok_or_else(|| "expected dialog result".to_owned())?;
        assert_eq!(dialog, "This command opens a dialog in the window");
        assert_eq!(
            message(&dialog),
            Some("Dieser Befehl öffnet einen Dialog im Fenster".into())
        );
        let values = [
            ("/日本語 is not a file", "/日本語 ist keine Datei"),
            ("No such folder: /日本語", "Ordner nicht gefunden: /日本語"),
            (
                "/日本語 is a folder: give a file (or open the folder with gitcraft /日本語)",
                "/日本語 ist ein Ordner: Gib eine Datei an (oder öffne den Ordner mit gitcraft /日本語)",
            ),
            (
                "file 日本語 isn't in /root: fetch, then open the link again",
                "file 日本語 ist nicht in /root: Führe Fetch aus und öffne den Link erneut",
            ),
            (
                "Commit deadbeef isn't in /root: fetch, then open the link again",
                "Commit deadbeef ist nicht in /root: Führe Fetch aus und öffne den Link erneut",
            ),
            (
                "日本語: finish or close the conflict or rebase editor first, then run the command again",
                "日本語: Beende oder schließe zuerst den Konflikt- oder Rebase-Editor und führe dann den Befehl erneut aus",
            ),
        ];
        for (raw, de) in values {
            assert_eq!(message(raw).as_deref(), Some(de));
        }
        // Ambiguous raw human sentences stay opaque rather than guessing a boundary inside a filename.
        assert_eq!(
            message("a isn't in b isn't in c: fetch, then open the link again"),
            None
        );
        for unknown in [
            "external 日本語\n: untouched",
            "example is not a link gitcraft can open: external unknown reason",
            "Not a gitcraft link gitcraft can open (unknown reason)",
        ] {
            assert_eq!(message(unknown), None);
        }
        crate::i18n::set_language("en-US");
        for (raw, _) in values {
            assert_eq!(message(raw).as_deref(), Some(raw));
        }
        assert_eq!(message(&raw), Some(raw));
        Ok(())
    }
}
