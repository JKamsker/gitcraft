//! Secret detection emits line numbers only.
use super::*;

pub(super) fn secret_lines(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.lines().enumerate().filter_map(|(i, line)| {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        let credential = [
            "password",
            "passwd",
            "secret",
            "api_key",
            "apikey",
            "access_token",
            "auth_token",
        ]
        .iter()
        .any(|k| lower.contains(k))
            && (l.contains('=') || l.contains(':'))
            && l.split_once(['=', ':'])
                .is_some_and(|(_, v)| v.trim_matches([' ', '\'', '"', ';', ',']).len() >= 8);
        let token = ["ghp_", "github_pat_", "glpat-", "sk_live_", "xoxb-", "AKIA"]
            .iter()
            .any(|p| {
                l.find(p).is_some_and(|n| {
                    l[n + p.len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                        .count()
                        >= 12
                })
            });
        (credential || token || l.contains("PRIVATE KEY-----")).then_some(i + 1)
    })
}

pub(super) fn blob_secret_lines(
    repo: &Path,
    oid: &str,
    active: &dyn Fn() -> bool,
) -> Result<Vec<usize>, String> {
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid outgoing object".into());
    }
    // Streaming on the repository's machine avoids downloading/allocating huge blobs. Only
    // line numbers leave this pipeline: neither a matched line nor its secret enters a UI result.
    let pattern = r#"PRIVATE KEY-----|ghp_[A-Za-z0-9_]{12,}|github_pat_[A-Za-z0-9_]{12,}|glpat-[A-Za-z0-9_-]{12,}|sk_live_[A-Za-z0-9_]{12,}|xoxb-[A-Za-z0-9-]{12,}|AKIA[A-Z0-9]{16}|(password|passwd|secret|api_key|apikey|access_token|auth_token)[[:space:]\"']*[:=].{8,64}"#;
    let script = format!(
        "!command -v grep >/dev/null && command -v cut >/dev/null && command -v head >/dev/null || exit 1; {{ {{ git --no-lazy-fetch cat-file blob {} || printf 'READ_ERROR\\n' >&3; }} | grep -ainE {} | cut -d: -f1 | head -n 257; }} 3>&1",
        oid,
        crate::remote::sh_quote(pattern)
    );
    let output = read(
        repo,
        &[
            "-c",
            &format!("alias.gitcraft-preflight-secrets={script}"),
            "gitcraft-preflight-secrets",
        ],
        active,
    )?;
    output
        .lines()
        .map(|n| {
            n.parse().map_err(|_| {
                "The content scan failed or exceeded its finding limit; no push was run.".into()
            })
        })
        .collect()
}
