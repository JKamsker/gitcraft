//! What the card says about each problem: its title and what is going on, in plain words.

use crate::git::errors::GitProblem;

/// Where git runs, which changes what can be done about some problems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// Git for Windows, on this machine.
    pub windows: bool,
    /// In WSL or over SSH (always Linux there).
    pub remote: bool,
}

impl Place {
    /// Git for Windows: where `core.longpaths` and `http.sslBackend=schannel` mean something.
    pub fn git_for_windows(self) -> bool {
        self.windows && !self.remote
    }
}

/// Most file names a line-ending card lists.
const LISTED: usize = 3;

pub fn title(problem: &GitProblem) -> String {
    match problem {
        GitProblem::DubiousOwnership { .. } => "git doesn't trust this folder".to_owned(),
        GitProblem::StaleLock { .. } => "Another git seems to be at work here".to_owned(),
        GitProblem::LineEndings { to_crlf: true, .. } => {
            "git changes line endings here (LF to CRLF)".to_owned()
        }
        GitProblem::LineEndings { .. } => "git changes line endings here (CRLF to LF)".to_owned(),
        GitProblem::NameTooLong => "A path is too long".to_owned(),
        GitProblem::Certificate { .. } => "The server's certificate couldn't be checked".to_owned(),
        GitProblem::Transfer { too_large: true } => {
            "The server refused a request this large (HTTP 413)".to_owned()
        }
        GitProblem::Transfer { .. } => "The connection broke during the transfer".to_owned(),
        GitProblem::UnrelatedHistories => "These histories have nothing in common".to_owned(),
        GitProblem::ShallowPush => "A shallow clone can't push here".to_owned(),
        GitProblem::SshKeyRefused { .. } => "The SSH server didn't accept your key".to_owned(),
    }
}

/// What is going on and what can be done, a paragraph each.
pub fn explanation(problem: &GitProblem, place: Place) -> Vec<String> {
    match problem {
        GitProblem::DubiousOwnership { path } => vec![
            format!(
                "{path} belongs to another user account (copied from another machine, on a drive \
                 formatted elsewhere, or on a network share). git refuses to work in such a \
                 folder, since its config could run programs as you."
            ),
            "Trust this folder adds this folder, and only this one, to safe.directory in your \
             global git config. Only do that if you know where the repository came from."
                .to_owned(),
        ],
        GitProblem::StaleLock { lock } => vec![
            format!(
                "git keeps {} while it changes the repository and removes it when it is done. \
                 It is still there: either another git is working in this repository (an editor \
                 opened by git commit, an IDE, a terminal), or one crashed and left it behind.",
                file_name(lock)
            ),
            "Remove it only when no git is working in this repository.".to_owned(),
        ],
        GitProblem::LineEndings { files, fatal, .. } => line_endings(files, *fatal),
        GitProblem::NameTooLong if place.git_for_windows() => vec![
            "Windows limits paths to 260 characters unless a program asks for longer ones. \
             core.longpaths makes git ask: Enable long paths sets it for this repository."
                .to_owned(),
        ],
        GitProblem::NameTooLong => vec![
            "A name in it is longer than this machine's file system takes (255 bytes per name on \
             most). git can't change that: rename the file from a machine that can check it \
             out, or leave its folder out with a sparse checkout."
                .to_owned(),
        ],
        GitProblem::Certificate { detail } => certificate(detail, place),
        GitProblem::Transfer { too_large: true } => vec![
            "The server, or a proxy in front of it, limits how large a request may be. Push \
             fewer commits at a time (an older commit first: git push origin <commit>:<branch> \
             in the terminal), keep large files in Git LFS, or ask the server's administrator \
             to raise the limit (client_max_body_size on nginx)."
                .to_owned(),
        ],
        GitProblem::Transfer { .. } => vec![
            "The server or the network stopped before git had everything (RPC failed, early \
             EOF). Try again. On a slow or unreliable network, a shallow clone (git clone \
             --depth 1) or the SSH address of the repository instead of HTTPS gets less at \
             once."
                .to_owned(),
        ],
        GitProblem::UnrelatedHistories => vec![
            "The branch being merged shares no commit with yours: usually a repository made \
             twice (git init here, and a new repository with a README on the forge), or the \
             wrong remote."
                .to_owned(),
            "If they are meant to be joined, run git pull --allow-unrelated-histories (or git \
             merge --allow-unrelated-histories) in the terminal; otherwise check the remote's \
             URL."
                .to_owned(),
        ],
        GitProblem::ShallowPush => vec![
            "This clone has only part of the history (it was cloned with --depth), and the \
             remote takes pushes only from clones that have all of it. Fetch the rest with git \
             fetch --unshallow in the terminal, then push again."
                .to_owned(),
        ],
        GitProblem::SshKeyRefused { login } => ssh_key(login.as_deref(), place),
    }
}

fn line_endings(files: &[String], fatal: bool) -> Vec<String> {
    let mut paragraphs = Vec::new();
    if !files.is_empty() {
        let shown: Vec<&str> = files.iter().take(LISTED).map(String::as_str).collect();
        let more = files.len().saturating_sub(LISTED);
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        paragraphs.push(format!("Files: {}{more}.", shown.join(", ")));
    }
    paragraphs.push(
        "core.autocrlf, in your own config, applies to every repository of this machine: true \
         turns LF into CRLF when files are checked out and back into LF when they are committed \
         (the usual choice on Windows); input turns CRLF into LF when committing and changes \
         nothing on checkout; false leaves line endings alone."
            .to_owned(),
    );
    paragraphs.push(
        ".gitattributes, committed with the repository, is the same for everyone who clones \
         it: * text=auto stores text files with LF and checks them out with each machine's \
         ending; eol=lf or eol=crlf on a pattern gives those files that ending everywhere. A \
         .gitattributes rule wins over core.autocrlf."
            .to_owned(),
    );
    if fatal {
        paragraphs.push(
            "core.safecrlf is set, so git refused rather than warned: change one of the two, or \
             unset core.safecrlf."
                .to_owned(),
        );
    }
    paragraphs
}

fn certificate(detail: &str, place: Place) -> Vec<String> {
    let mut paragraphs = vec![format!(
        "TLS said: {detail}. Usually a company network or proxy that signs HTTPS with its own \
         certificate authority, or a server with a self-signed certificate."
    )];
    paragraphs.push(if place.git_for_windows() {
        "Git for Windows checks certificates against its own list by default. With \
         http.sslBackend = schannel it uses Windows' certificate store instead, where your IT \
         department adds its certificate: Use Windows' certificates sets that in your global \
         git config."
            .to_owned()
    } else {
        "Add the certificate authority to this machine's trusted ones, or point git at it with \
         http.sslCAInfo. Turning http.sslVerify off would accept any server, an impostor too."
            .to_owned()
    });
    paragraphs
}

fn ssh_key(login: Option<&str>, place: Place) -> Vec<String> {
    let who = login.unwrap_or("The server");
    let test = login.map_or_else(
        || "ssh -T <user>@<host>".to_owned(),
        |login| format!("ssh -T {login}"),
    );
    let mut paragraphs = vec![format!(
        "{who} took none of the keys ssh offered. See what it says to {test} in the terminal; \
         load your key into the agent (ssh-add), or add its public key (~/.ssh/id_ed25519.pub) \
         to your account on the forge."
    )];
    if place.remote {
        paragraphs.push(
            "The repository is on another machine, so it is that machine's keys and agent that \
             count."
                .to_owned(),
        );
    }
    paragraphs
}

/// The last name of `path`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// "3 minutes", "2 days": how long `secs` is.
pub fn age(secs: u64) -> String {
    let ago = crate::git::relative_time(0, secs.min(i64::MAX as u64) as i64);
    match ago.as_str() {
        "just now" => "less than a minute".to_owned(),
        _ => ago.trim_end_matches(" ago").to_owned(),
    }
}
