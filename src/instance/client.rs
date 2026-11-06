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
        Ok(Some(Reply::Closing) | None) | Err(_) => Handoff::NoAnswer,
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
