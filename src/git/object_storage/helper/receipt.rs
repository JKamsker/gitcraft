//! A status query never treats missing files or an unlocked store as completion.
//! Read the exact ticket's retained terminal receipt through actual directory handles.
use super::{Identity, LIMIT, PROTOCOL, Receipt, Ticket};
use rustix::fs::{self, Mode, OFlags};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
use std::sync::Arc;

struct Node {
    parent: Arc<File>,
    name: OsString,
    file: Arc<File>,
    directory: bool,
}
fn identity(file: &File) -> Result<Identity, String> {
    let stat = fs::fstat(file).map_err(|e| e.to_string())?;
    let birth = file
        .metadata()
        .map_err(|e| e.to_string())?
        .created()
        .map_err(|_| "Receipt identity requires filesystem birth-time support")?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(Identity {
        device: stat.st_dev,
        inode: stat.st_ino,
        birth_seconds: birth.as_secs(),
        birth_nanos: birth.subsec_nanos(),
    })
}
fn flags(directory: bool) -> OFlags {
    OFlags::RDONLY
        | OFlags::CLOEXEC
        | OFlags::NOFOLLOW
        | OFlags::NONBLOCK
        | if directory {
            OFlags::DIRECTORY
        } else {
            OFlags::empty()
        }
}
fn child(
    nodes: &mut Vec<Node>,
    parent: &Arc<File>,
    name: &OsStr,
    directory: bool,
) -> Result<Arc<File>, String> {
    let file = Arc::new(File::from(
        fs::openat(parent.as_ref(), name, flags(directory), Mode::empty())
            .map_err(|e| e.to_string())?,
    ));
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if meta.is_dir() != directory || (!directory && !meta.is_file()) {
        return Err("Receipt metadata is not a plain file/directory".into());
    }
    nodes.push(Node {
        parent: parent.clone(),
        name: name.into(),
        file: file.clone(),
        directory,
    });
    Ok(file)
}
fn current(nodes: &[Node]) -> Result<(), String> {
    for node in nodes {
        let named = File::from(
            fs::openat(
                node.parent.as_ref(),
                node.name.as_os_str(),
                flags(node.directory),
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        );
        if identity(&named)? != identity(&node.file)? {
            return Err("Receipt ancestor/store/file identity changed".into());
        }
    }
    Ok(())
}
pub(super) fn read(ticket: &Ticket) -> Result<Result<String, String>, String> {
    if ticket.transaction.is_empty()
        || ticket.transaction.len() > 64
        || !ticket.transaction.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid receipt transaction".into());
    }
    let repo = Path::new(&ticket.repo);
    if !repo.is_absolute() || Path::new(&ticket.primary) != repo.join(".git/objects") {
        return Err("Receipt requires the prepared conventional object directory".into());
    }
    let root = Arc::new(File::from(
        fs::open("/", flags(true), Mode::empty()).map_err(|e| e.to_string())?,
    ));
    let mut nodes = Vec::new();
    let mut directory = root.clone();
    for component in repo.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(name) => directory = child(&mut nodes, &directory, name, true)?,
            _ => return Err("Receipt repository path cannot contain . or ..".into()),
        }
    }
    let git = child(&mut nodes, &directory, OsStr::new(".git"), true)?;
    let objects = child(&mut nodes, &git, OsStr::new("objects"), true)?;
    let _info = child(&mut nodes, &objects, OsStr::new("info"), true)?;
    let _pack = child(&mut nodes, &objects, OsStr::new("pack"), true)?;
    let mut actual = vec![identity(&root)?];
    for node in &nodes {
        actual.push(identity(&node.file)?);
    }
    // The original alternate may now live at its backup name. Directory identities
    // must still match; the terminal receipt contains and binds the complete ticket.
    if ticket.identities.len() != actual.len() + 1 || ticket.identities[..actual.len()] != actual {
        return Err(
            "Prepared ancestor/object-store identities changed; receipt not accepted".into(),
        );
    }
    current(&nodes)?;
    let stage_name = format!("gitcraft-independent-stage-{}", ticket.transaction);
    let stage = child(&mut nodes, &objects, OsStr::new(&stage_name), true)?;
    let file = child(
        &mut nodes,
        &stage,
        OsStr::new("gitcraft-outcome.json"),
        false,
    )?;
    let mut bytes = Vec::new();
    file.as_ref()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Terminal receipt exceeds 4 MiB".into());
    }
    let outcome: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| "Terminal receipt is incomplete or malformed")?;
    current(&nodes)?;
    if outcome.protocol != PROTOCOL || outcome.ticket != *ticket {
        return Err("Terminal receipt does not match the prepared ownership ticket".into());
    }
    Ok(outcome.result)
}
