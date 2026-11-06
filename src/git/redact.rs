//! Hiding secrets in what is kept of a git call (the activity log, the clipboard): a token in a
//! URL, an `Authorization` header (git prints them with `GIT_TRACE_CURL` set), a password typed
//! into the tab.

/// What stands for a secret.
const HIDDEN: &str = "***";

/// `text` with its secrets hidden: in a URL, the whole sign-in of an `http(s)` one
/// (`https://ghp_token@host`, `https://user:pass@host`) and the password of any other
/// (`ssh://git@host` keeps its user: it is no secret); what follows `Authorization:`; and
/// `password`, the one typed into the tab, wherever it is.
pub fn redact(text: &str, password: Option<&str>) -> String {
    let mut out: String = text.split_inclusive('\n').map(redact_line).collect();
    if let Some(p) = password.filter(|p| p.len() >= 3) {
        out = out.replace(p, HIDDEN);
    }
    out
}

fn redact_line(line: &str) -> String {
    if let Some(at) = find_ignore_case(line, "authorization:") {
        let end = at + "authorization:".len();
        let eol = if line.ends_with('\n') { "\n" } else { "" };
        return format!("{} {HIDDEN}{eol}", &line[..end]);
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find("://") {
        let scheme_start = rest[..i]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || "+-.".contains(c)))
            .map_or(0, |j| j + 1);
        let scheme = rest[scheme_start..i].to_ascii_lowercase();
        let after = &rest[i + 3..];
        let authority_end = after
            .find(|c: char| c == '/' || c.is_whitespace() || c == '\'' || c == '"')
            .unwrap_or(after.len());
        out.push_str(&rest[..i + 3]);
        match after[..authority_end].rfind('@') {
            Some(at) => {
                let userinfo = &after[..at];
                if scheme.starts_with("http") {
                    out.push_str(HIDDEN);
                } else {
                    match userinfo.split_once(':') {
                        Some((user, _)) => {
                            out.push_str(user);
                            out.push(':');
                            out.push_str(HIDDEN);
                        }
                        None => out.push_str(userinfo),
                    }
                }
                out.push_str(&after[at..authority_end]);
            }
            None => out.push_str(&after[..authority_end]),
        }
        rest = &after[authority_end..];
    }
    out.push_str(rest);
    out
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_tokens_and_passwords_in_urls() {
        for (text, want) in [
            (
                "https://ghp_abc123@github.com/o/r.git",
                "https://***@github.com/o/r.git",
            ),
            ("HTTPS://ana:s3cret@host/r", "HTTPS://***@host/r"),
            (
                "git clone 'https://tok@dev.azure.com/x' dépôt",
                "git clone 'https://***@dev.azure.com/x' dépôt",
            ),
            ("ssh://git@host/r.git", "ssh://git@host/r.git"),
            ("ssh://me:pw@host/r", "ssh://me:***@host/r"),
            ("git@github.com:o/r.git", "git@github.com:o/r.git"),
            ("https://host/a@b/c", "https://host/a@b/c"),
            (
                "fatal: unable to access 'https://x:y@h/': 403\nnext https://t@h",
                "fatal: unable to access 'https://***@h/': 403\nnext https://***@h",
            ),
            ("no urls here", "no urls here"),
            ("", ""),
        ] {
            assert_eq!(redact(text, None), want, "{text:?}");
        }
    }

    #[test]
    fn hides_authorization_headers_and_the_typed_password() {
        let trace = "=> Send header: Authorization: Basic YW5hOnB3\n=> Send header: Host: x\n";
        assert_eq!(
            redact(trace, None),
            "=> Send header: Authorization: ***\n=> Send header: Host: x\n"
        );
        assert_eq!(
            redact("said p4ssw0rd twice: p4ssw0rd", Some("p4ssw0rd")),
            "said *** twice: ***"
        );
        // Too short to hide without hiding everything else.
        assert_eq!(redact("a b", Some("a")), "a b");
    }
}
