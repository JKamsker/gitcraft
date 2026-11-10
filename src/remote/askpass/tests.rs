use super::super::vault;
use super::*;

/// What `env_for` gives ssh as `GITCRAFT_ASKPASS`.
fn session_for(host: &str) -> String {
    start_for(host).0
}

/// An ssh started for `host`: what it is given as `GITCRAFT_ASKPASS`, and its attempt.
fn start_for(host: &str) -> (String, Option<Attempt>) {
    match env_for(host) {
        Some((env, attempt)) => (
            env.into_iter()
                .find(|(k, _)| *k == ENV)
                .map(|(_, v)| v)
                .unwrap_or_default(),
            Some(attempt),
        ),
        None => (String::new(), None),
    }
}

/// Answers `prompt` asked by the ssh `found` names with `secret`, ticking "Remember
/// connection".
fn type_and_remember(found: &str, prompt: &str, secret: &str) -> Option<String> {
    let asked = ask_in_background(found.to_owned(), prompt, Kind::Secret);
    if let Some(q) = loop_until_shown(prompt) {
        answer_and_remember(q.id, secret.to_owned());
    }
    asked.join().ok().flatten()
}

/// Answers the next question shown whose prompt is `prompt` with `answer`; the question.
fn answer_shown(prompt: &str, answer_with: Option<&str>) -> Option<Question> {
    for _ in 0..500 {
        if let Some(q) = pending().filter(|q| q.prompt == prompt) {
            answer(q.id, answer_with.map(str::to_owned));
            return Some(q);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn ask_in_background(
    found: String,
    prompt: &str,
    kind: Kind,
) -> std::thread::JoinHandle<Option<String>> {
    let prompt = prompt.to_owned();
    std::thread::spawn(move || ask_app(&found, &prompt, kind).ok().flatten())
}

#[test]
fn questions_come_to_the_window_and_answers_go_back() {
    let _asking = asking();
    let prompt = "me@one's password: (answers)";
    let asked = ask_in_background(session_for("SSH · one"), prompt, Kind::Secret);
    let shown = answer_shown(prompt, Some("s3cret"));
    assert_eq!(
        shown.map(|q| (q.host, q.kind)),
        Some(("SSH · one".to_owned(), Kind::Secret))
    );
    assert_eq!(asked.join().ok().flatten(), Some("s3cret".to_owned()));
    // Declined.
    let asked = ask_in_background(session_for("SSH · one"), "Trust (yes/no)?", Kind::TrustHost);
    assert!(answer_shown("Trust (yes/no)?", None).is_some());
    assert_eq!(asked.join().ok().flatten(), None);
}

/// The machine's other shells get the password typed; one that asks again (it was wrong)
/// makes it asked of the user again.
#[test]
fn a_secret_typed_serves_the_machine_until_it_is_wrong() {
    let _asking = asking();
    let prompt = "me@two's password: (remembered)";
    let first = session_for("SSH · two");
    let asked = ask_in_background(first.clone(), prompt, Kind::Secret);
    answer_shown(prompt, Some("typed"));
    assert_eq!(asked.join().ok().flatten(), Some("typed".to_owned()));
    // Another shell of the machine: not asked.
    let other = session_for("SSH · two");
    assert_eq!(
        ask_app(&other, prompt, Kind::Secret).ok().flatten(),
        Some("typed".to_owned())
    );
    // That shell asks again: what was remembered is wrong, the user is asked.
    let again = ask_in_background(other, prompt, Kind::Secret);
    assert!(answer_shown(prompt, Some("right")).is_some());
    assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
}

/// What this run of the app knows of `host`'s secrets, gone (as after a restart).
fn restart_for(host: &str) {
    if let Some(server) = server() {
        lock(&server.state).known.retain(|(h, _), _| h != host);
    }
}

/// Kept with "Remember connection" once its ssh connected, a secret is given after a
/// restart without asking; wrong, it is forgotten there too, and the user told so.
#[test]
fn a_remembered_secret_outlives_the_app_until_it_is_wrong() {
    let _asking = asking();
    let (host, prompt) = ("SSH · four", "me@four's password: (kept)");
    let (found, attempt) = start_for(host);
    assert_eq!(
        type_and_remember(&found, prompt, "kept"),
        Some("kept".to_owned())
    );
    // Not kept before the connection is made.
    assert_eq!(vault::recall(host, prompt), None);
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    assert_eq!(vault::recall(host, prompt), Some("kept".to_owned()));

    restart_for(host);
    let after = session_for(host);
    assert_eq!(
        ask_app(&after, prompt, Kind::Secret).ok().flatten(),
        Some("kept".to_owned())
    );
    // That ssh asks again: the kept one was wrong.
    let again = ask_in_background(after, prompt, Kind::Secret);
    let shown = loop_until_shown(prompt);
    assert_eq!(shown.as_ref().map(|q| q.forgotten), Some(true));
    assert_eq!(vault::recall(host, prompt), None);
    if let Some(q) = shown {
        answer(q.id, Some("right".to_owned()));
    }
    assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
    // Typed without remembering: not kept.
    assert_eq!(vault::recall(host, prompt), None);
}

/// A connection that isn't made keeps nothing, even ticked.
#[test]
fn a_secret_is_not_kept_for_a_connection_that_failed() {
    let _asking = asking();
    let (host, prompt) = ("SSH · five", "me@five's password: (failed)");
    let (found, attempt) = start_for(host);
    assert_eq!(
        type_and_remember(&found, prompt, "typo"),
        Some("typo".to_owned())
    );
    drop(attempt);
    assert_eq!(vault::recall(host, prompt), None);
}

/// Wrong, then right, in one connection: the right one is kept, the wrong one never.
#[test]
fn only_the_secret_that_signed_in_is_kept() {
    let _asking = asking();
    let (host, prompt) = ("SSH · six", "me@six's password: (retyped)");
    let (found, attempt) = start_for(host);
    assert_eq!(
        type_and_remember(&found, prompt, "wrong"),
        Some("wrong".to_owned())
    );
    // ssh asks again: that was wrong. Typed again, not ticked this time.
    let again = ask_in_background(found, prompt, Kind::Secret);
    if let Some(q) = loop_until_shown(prompt) {
        answer(q.id, Some("right".to_owned()));
    }
    assert_eq!(again.join().ok().flatten(), Some("right".to_owned()));
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    assert_eq!(vault::recall(host, prompt), None);

    // Wrong ticked, then right ticked: the right one.
    let (found, attempt) = start_for(host);
    restart_for(host);
    assert_eq!(
        type_and_remember(&found, prompt, "wrong"),
        Some("wrong".to_owned())
    );
    assert_eq!(
        type_and_remember(&found, prompt, "right"),
        Some("right".to_owned())
    );
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    assert_eq!(vault::recall(host, prompt), Some("right".to_owned()));
}

/// The next question shown whose prompt is `prompt`, unanswered.
fn loop_until_shown(prompt: &str) -> Option<Question> {
    for _ in 0..500 {
        if let Some(q) = pending().filter(|q| q.prompt == prompt) {
            return Some(q);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// A request longer than any helper's is dropped once that much is read: never held whole,
/// never answered, nothing asked.
#[test]
fn an_endless_request_is_cut_off() -> Result<(), String> {
    let _asking = asking();
    let found = session_for("SSH · seven");
    let port: u16 = found
        .split_whitespace()
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or("no port")?;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    // No newline, ever: more than the limit, then the end of what it sends.
    let endless = vec![b'a'; usize::try_from(MAX_REQUEST).unwrap_or(0) + 4096];
    let _ = stream.write_all(&endless);
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);
    assert!(reply.is_empty(), "{:?}", String::from_utf8_lossy(&reply));
    Ok(())
}

/// Without the token, nothing is asked and nothing answered.
#[test]
fn a_stranger_gets_no_answer() {
    let _asking = asking();
    let real = session_for("SSH · three");
    let mut parts = real.split_whitespace();
    let (port, session) = (
        parts.next().unwrap_or_default(),
        parts.nth(1).unwrap_or_default(),
    );
    let forged = format!("{port} 0000000000000000 {session}");
    let reply = ask_app(&forged, "me@three's password: (stranger)", Kind::Secret);
    assert!(matches!(reply, Ok(None) | Err(_)), "{reply:?}");
    assert!(pending().is_none_or(|q| !q.prompt.contains("(stranger)")));
}

#[test]
fn what_ssh_asks_is_told_apart() {
    let _asking = asking();
    assert_eq!(Kind::of("me@h's password: ", None), Kind::Secret);
    assert_eq!(
        Kind::of("Enter passphrase for key 'k': ", None),
        Kind::Secret
    );
    assert_eq!(
        Kind::of(
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            None
        ),
        Kind::TrustHost
    );
    assert_eq!(
        Kind::of("Allow use of key?", Some("confirm")),
        Kind::Confirm
    );
    assert_eq!(
        Kind::of("Confirm user presence", Some("none")),
        Kind::Notice
    );
}
