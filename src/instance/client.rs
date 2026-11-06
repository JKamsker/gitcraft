//! A later start's side: hands its request to the running instance and reads the answer.

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::discovery::Found;
use super::message::{self, Reply, Request};

/// How long connecting to the running instance may take: it is on this machine.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// How long the running instance has to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

/// What became of a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Handoff {
    /// The running instance took it.
    Taken,
    /// The running instance refused it, and why.
    Refused(String),
    /// No answer that came from it: nothing listens there, the token is wrong (the file is a
    /// crashed instance's, the port another program's), or it is closing.
    NoAnswer,
}

/// Sends `request` to the instance `found` names.
pub fn hand_over(found: &Found, request: &Request) -> Handoff {
    match send(found, request) {
        Ok(Some(Reply::Ok)) => Handoff::Taken,
        Ok(Some(Reply::Refused(why))) => Handoff::Refused(why),
        // An agent's answer is no answer to a start.
        Ok(Some(Reply::Closing | Reply::Agent(_)) | None) | Err(_) => Handoff::NoAnswer,
    }
}

fn send(found: &Found, request: &Request) -> std::io::Result<Option<Reply>> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, found.port));
    let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
    stream.set_write_timeout(Some(ANSWER_TIMEOUT))?;
    stream.write_all(message::encode_request(&found.token, request).as_bytes())?;
    let line = super::server::read_line(&stream, ANSWER_TIMEOUT)?;
    Ok(line.as_deref().and_then(message::decode_reply))
}

/// Hands an agent's `call` to the instance `found` names and waits up to `wait` for how it
/// ended, in short steps, stopping (and closing the connection, which the window sees) as soon
/// as `cancelled`. `Err` says why there's no answer.
pub fn call_agent(
    found: &Found,
    call: &message::AgentCall,
    wait: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<crate::mcp::wire::Reply, String> {
    let line = message::encode_request(&found.token, &Request::Agent(call.clone()));
    if line.len() as u64 > message::MAX_LINE {
        return Err("the request is too big to send (name fewer or shorter paths)".to_owned());
    }
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, found.port));
    let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
        .map_err(|_| "gitcraft's window doesn't answer".to_owned())?;
    let _ = stream.set_write_timeout(Some(ANSWER_TIMEOUT));
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("couldn't reach gitcraft's window: {e}"))?;
    let until = std::time::Instant::now() + wait;
    loop {
        if cancelled() {
            return Err(
                "the call was cancelled; gitcraft drops the request unless it had started it"
                    .to_owned(),
            );
        }
        let left = until
            .checked_duration_since(std::time::Instant::now())
            .ok_or("no answer in time; it may still be running")?;
        let _ = stream.set_read_timeout(Some(left.min(Duration::from_millis(300))));
        let mut byte = [0u8; 1];
        match stream.peek(&mut byte) {
            Ok(_) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(format!("the connection to gitcraft broke: {e}")),
        }
    }
    let answer = super::server::read_line(&stream, ANSWER_TIMEOUT)
        .ok()
        .flatten()
        .and_then(|line| message::decode_reply(&line));
    match answer {
        Some(Reply::Agent(reply)) => Ok(reply),
        Some(Reply::Refused(why)) => Err(format!("gitcraft's window refused it: {why}")),
        Some(Reply::Closing) => Err("gitcraft's window is closing".to_owned()),
        // A token that isn't this window's (a crashed one's file) is answered with nothing.
        Some(Reply::Ok) | None => Err("gitcraft's window didn't answer".to_owned()),
    }
}
