//! Askpass socket protocol, prompt delivery, and remembered-secret decisions.
use super::*;

/// One request line, its `\n` included; `None` for one longer than [`MAX_REQUEST`], cut
/// short, or not text.
pub(super) fn read_request(stream: &TcpStream) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    BufReader::new(stream.take(MAX_REQUEST)).read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    Ok(String::from_utf8(line).ok())
}

pub(super) fn serve(stream: TcpStream) -> std::io::Result<()> {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let Some(server) = server() else {
        return Ok(());
    };
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let Some(line) = read_request(&stream)? else {
        return Ok(());
    };
    stream.set_read_timeout(None)?;
    let mut words = line.split_whitespace();
    let (Some(token), Some(session), Some(kind), prompt) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return Ok(());
    };
    // Only an ssh this app started knows the token.
    if !same(token.as_bytes(), server.token.as_bytes()) {
        return Ok(());
    }
    let (Ok(session), Some(kind)) = (session.parse::<u64>(), Kind::from_word(kind)) else {
        return Ok(());
    };
    let prompt = prompt
        .and_then(|p| base64::engine::general_purpose::STANDARD.decode(p).ok())
        .map(|p| String::from_utf8_lossy(&p).trim().to_owned())
        .unwrap_or_default();
    let reply = ask(
        server,
        session,
        kind,
        prompt,
        IDS.fetch_add(1, Ordering::Relaxed),
    );
    let mut stream = stream;
    match reply {
        Some(answer) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(answer);
            writeln!(stream, "OK {encoded}")
        }
        None => writeln!(stream, "NO"),
    }
}

/// What is known of a secret's answer before the user is asked.
pub(super) enum Known {
    /// Give this.
    Use(String),
    /// What was given this ssh before was wrong (and is forgotten here).
    Wrong,
    /// Nothing here: maybe in the system's store.
    Nothing,
}

/// The answer to `prompt` of `session`'s ssh: a secret known for its machine (typed since the
/// app started, or kept in the system's store), else the user's.
pub(super) fn ask(
    server: &Server,
    session: u64,
    kind: Kind,
    prompt: String,
    id: u64,
) -> Option<String> {
    let host = lock(&server.state)
        .sessions
        .get(&session)
        .cloned()
        .unwrap_or_else(|| "SSH".to_owned());
    let mut forgotten = false;
    if kind == Kind::Secret {
        match known(server, session, &host, &prompt) {
            Known::Use(secret) => return Some(secret),
            // Not to be kept, nor kept any longer if it was.
            Known::Wrong => {
                forget_to_keep(server, session, &prompt);
                forgotten = super::super::vault::forget(&host, &prompt);
            }
            Known::Nothing => {
                if let Some(saved) = super::super::vault::recall(&host, &prompt) {
                    remember(server, session, &host, &prompt, &saved);
                    return Some(saved);
                }
            }
        }
    }
    let (tx, rx) = channel();
    let repaint = {
        let mut state = lock(&server.state);
        state.waiting.push(Waiting {
            question: Question {
                id,
                host: host.clone(),
                prompt: prompt.clone(),
                kind,
                forgotten,
            },
            reply: tx,
        });
        state.last_asked = Some(Instant::now());
        state.repaint.clone()
    };
    if let Some(ctx) = repaint {
        ctx.request_repaint();
    }
    let (answer, keep) = rx.recv().ok().flatten()?;
    if kind == Kind::Secret {
        remember(server, session, &host, &prompt, &answer);
        // Kept only once this ssh has connected with it (see `Attempt`): a wrong one never is.
        if keep {
            let mut state = lock(&server.state);
            let list = state.to_keep.entry(session).or_default();
            list.retain(|k| k.prompt != prompt);
            list.push(ToKeep {
                host,
                prompt,
                secret: answer.clone(),
            });
        }
    }
    Some(answer)
}

/// `session`'s answer to `prompt` was wrong: not to be kept.
pub(super) fn forget_to_keep(server: &Server, session: u64, prompt: &str) {
    if let Some(list) = lock(&server.state).to_keep.get_mut(&session) {
        list.retain(|k| k.prompt != prompt);
    }
}

/// What is known of `prompt`'s answer for `session`'s ssh to `host`. Asked again by the same
/// ssh, what it was given was wrong: forgotten.
pub(super) fn known(server: &Server, session: u64, host: &str, prompt: &str) -> Known {
    let mut state = lock(&server.state);
    let key = (host.to_owned(), prompt.to_owned());
    let once = (session, prompt.to_owned());
    if state.answered_from_known.remove(&once) {
        state.known.remove(&key);
        invalidate_host(host);
        return Known::Wrong;
    }
    match state.known.get(&key).cloned() {
        Some(secret) => {
            state.answered_from_known.insert(once);
            Known::Use(secret)
        }
        None => Known::Nothing,
    }
}

/// Keeps `secret` for the machine's other shells, and marks it given to this ssh: it asking
/// again means it was wrong (see [`known`]).
pub(super) fn remember(server: &Server, session: u64, host: &str, prompt: &str, secret: &str) {
    let mut state = lock(&server.state);
    if state
        .known
        .insert((host.to_owned(), prompt.to_owned()), secret.to_owned())
        .is_some_and(|old| old != secret)
    {
        invalidate_host(host);
    }
    state
        .answered_from_known
        .insert((session, prompt.to_owned()));
}
