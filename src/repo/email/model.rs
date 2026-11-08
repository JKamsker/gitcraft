//! Pure mail decoding and plans. No transport is performed by this module.
use mailparse::MailHeaderMap;
use sha2::{Digest, Sha256};

pub(super) const MAX: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug)]
pub(super) struct Message {
    pub headers: String,
    pub body: String,
    pub signature: Vec<u8>,
    pub raw: String,
}
impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        self.headers == other.headers
            && self.body == other.body
            && self.signature == other.signature
    }
}
impl Eq for Message {}
pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn parts(
    mail: &mailparse::ParsedMail<'_>,
    depth: usize,
    body: &mut String,
    signature: &mut Vec<u8>,
) -> Result<(), String> {
    if depth > 24 {
        return Err("Mail MIME nesting exceeds 24 levels".into());
    }
    signature.extend_from_slice(mail.ctype.mimetype.as_bytes());
    let mut params = mail.ctype.params.iter().collect::<Vec<_>>();
    params.sort();
    for (key, value) in params {
        if key != "boundary" {
            signature.extend_from_slice(format!("{key}={value}\0").as_bytes());
        }
    }
    for value in mail.headers.get_all_values("Content-Disposition") {
        signature.extend_from_slice(value.as_bytes());
    }
    signature.extend_from_slice(&(mail.subparts.len() as u64).to_be_bytes());
    if mail.subparts.is_empty() {
        let raw = mail.get_body_raw().map_err(|e| e.to_string())?;
        signature.extend_from_slice(&(raw.len() as u64).to_be_bytes());
        signature.extend_from_slice(&raw);
        if mail.ctype.mimetype.starts_with("text/") {
            body.push_str(&mail.get_body().map_err(|e| e.to_string())?);
            body.push('\n');
        } else {
            body.push_str(&format!(
                "[{} attachment: {} decoded bytes]\n",
                mail.ctype.mimetype,
                raw.len()
            ));
        }
    } else {
        for part in &mail.subparts {
            parts(part, depth + 1, body, signature)?;
        }
    }
    Ok(())
}
pub(super) fn parse(bytes: &[u8]) -> Result<Message, String> {
    if bytes.len() > MAX {
        return Err("Mail exceeds 32 MiB".into());
    }
    let bytes = if bytes.starts_with(b"From ") {
        bytes
            .splitn(2, |b| *b == b'\n')
            .nth(1)
            .ok_or("Incomplete mbox envelope")?
    } else {
        bytes
    };
    let mail = mailparse::parse_mail(bytes).map_err(|e| e.to_string())?;
    let mut headers = String::new();
    let mut signature = Vec::new();
    for key in ["From", "To", "Cc", "Bcc", "Subject", "Reply-To"] {
        let values = mail.headers.get_all_values(key);
        headers.push_str(&format!("{key}: {}\n", values.join(", ")));
        signature.extend_from_slice(key.as_bytes());
        for value in values {
            signature.extend_from_slice(&(value.len() as u64).to_be_bytes());
            signature.extend_from_slice(value.as_bytes());
        }
    }
    if mail.headers.get_first_value("Subject").is_none() {
        return Err("Mail has no Subject header".into());
    }
    let mut body = String::new();
    parts(&mail, 0, &mut body, &mut signature)?;
    Ok(Message {
        headers,
        body,
        signature,
        raw: String::from_utf8_lossy(bytes).into_owned(),
    })
}
pub(super) fn mailbox(bytes: &[u8]) -> Result<Vec<Message>, String> {
    if bytes.len() > MAX {
        return Err("Series exceeds 32 MiB".into());
    }
    // b4 save_git_am_mbox uses this exact envelope and intentionally does not
    // escape bare body "From " lines. Splitting every such line corrupts prose.
    const ENVELOPE: &[u8] = b"From git@z Thu Jan  1 00:00:00 1970\n";
    if !bytes.starts_with(ENVELOPE) {
        return Err("Expected b4's git-am mailbox envelope".into());
    }
    let mut starts = vec![0];
    for (i, window) in bytes.windows(ENVELOPE.len()).enumerate() {
        if i > 0 && bytes[i - 1] == b'\n' && window == ENVELOPE {
            starts.push(i);
        }
    }
    if starts.len() > 1024 {
        return Err("More than 1024 messages".into());
    }
    starts.push(bytes.len());
    starts
        .windows(2)
        .map(|w| parse(&bytes[w[0]..w[1]]))
        .collect()
}
pub(super) fn source(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 4096
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        return Err("Enter a public-inbox HTTPS URL or Message-ID".into());
    }
    if value.starts_with("https://") {
        let url = url::Url::parse(value).map_err(|e| e.to_string())?;
        if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
            return Err("Inbox URL cannot contain credentials".into());
        }
    } else if value.contains("://") || !value.contains('@') || value.contains(char::is_whitespace) {
        return Err("Expected a HTTPS URL or Message-ID".into());
    }
    Ok(value.into())
}
pub(super) fn recipients(to: &str, cc: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    for (flag, value) in [("--to", to), ("--cc", cc)] {
        if value.trim().is_empty() {
            continue;
        }
        if value.len() > 8192 || value.chars().any(char::is_control) || value.starts_with('-') {
            return Err("Invalid recipient list".into());
        }
        let parsed = mailparse::addrparse(value).map_err(|e| format!("Recipient list: {e}"))?;
        if parsed.is_empty() {
            return Err("Empty recipient list".into());
        }
        args.extend([flag.into(), value.into()]);
    }
    Ok(args)
}
/// The installed CLI's shebang identifies its Python environment (including pipx/venvs).
pub(super) fn interpreter(prefix: &[u8]) -> Result<String, String> {
    let line = std::str::from_utf8(prefix.split(|b| *b == b'\n').next().unwrap_or_default())
        .map_err(|_| "b4 entrypoint has no Python shebang")?;
    let words = line
        .strip_prefix("#!")
        .ok_or("Installed b4 must have a Python script entrypoint")?
        .split_whitespace()
        .collect::<Vec<_>>();
    match words.as_slice() {
        [path]
            if std::path::Path::new(path).is_absolute()
                && path
                    .rsplit('/')
                    .next()
                    .is_some_and(|p| p.starts_with("python")) =>
        {
            Ok((*path).into())
        }
        ["/usr/bin/env", name] if ["python", "python3"].contains(name) => Ok((*name).into()),
        _ => Err(
            "Unsupported b4 entrypoint; use the supported source build's Python entrypoint".into(),
        ),
    }
}
pub(super) fn reroll_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    let value = if let Some(inner) = value.strip_prefix('<') {
        inner
            .strip_suffix('>')
            .ok_or("Unbalanced Message-ID brackets")?
    } else {
        value
    };
    if value.contains("://") || value.contains(['<', '>']) {
        return Err("Use the previous sent Message-ID, not a URL".into());
    }
    source(value)?;
    Ok(value.into())
}
/// Fixed alias; Python code, interpreter and every native argument remain positional.
pub(super) fn command(python: &str, args: &[String], confirmed: bool) -> Vec<String> {
    let mut result = vec![
        "-c".into(),
        "alias.gitcraft-b4=!f() { py=\"$1\"; shift; exec \"$py\" -I -c \"$@\"; }; f".into(),
        "gitcraft-b4".into(),
        python.into(),
        include_str!("b4_entry.py").into(),
        if confirmed {
            "confirmed-send"
        } else {
            "no-send"
        }
        .into(),
    ];
    result.extend_from_slice(args);
    result
}
pub(super) fn send_reviewed<T>(
    shown: &[Message],
    fresh: &[Message],
    send: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if fresh != shown {
        return Err("Outgoing recipients or content changed; preview again before sending".into());
    }
    send()
}
