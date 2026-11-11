//! Nonce-bound request and response framing; no transport or connection state.
use super::*;

/// A word no command's output will hold by chance: new for every command.
pub(in crate::remote) fn nonce() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// `req` as the script a kept-open shell is sent (see the module's doc). The outputs are
/// copied before they are measured: something the command left running (a hook's background
/// job) may still be writing to them, and the bytes sent must be the bytes the header counts.
/// What can't be passed on or kept (a full disk) fails the command (125) and says why.
pub(in crate::remote) fn framed(req: &Request, nonce: &str) -> Result<String, String> {
    super::super::supervisor::request(req, nonce, None)
}

pub(in crate::remote) fn parse_ready(line: &[u8]) -> bool {
    line.trim_ascii_end() == READY.as_bytes()
}

/// `R <nonce> <code> <stdout bytes> <stderr bytes>` (`wc -c` may pad the numbers with
/// spaces), for the command of `nonce` only.
pub(in crate::remote) fn parse_header(line: &str, nonce: &str) -> Option<(i32, usize, usize)> {
    let mut words = line.split_whitespace();
    if words.next()? != "R" || words.next()? != nonce {
        return None;
    }
    let code = words.next()?.parse().ok()?;
    let out: usize = words.next()?.parse().ok()?;
    let err: usize = words.next()?.parse().ok()?;
    (words.next().is_none() && out <= MAX_OUTPUT && err <= MAX_OUTPUT).then_some((code, out, err))
}
