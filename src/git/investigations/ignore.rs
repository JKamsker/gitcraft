//! Deliberate append-only ignore-revision edits. Preparation is read-only; no real index is used.
use super::*;
use crate::git::recovery::{self, Identity};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::Arc;
const NAME: &str = ".git-blame-ignore-revs";
const LIMIT: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Snapshot {
    bytes: Option<String>,
    identity: Option<String>,
    modified: Option<String>,
}
impl Snapshot {
    fn missing() -> Self {
        Self {
            bytes: None,
            identity: None,
            modified: None,
        }
    }
    fn decoded(&self) -> Result<Option<Vec<u8>>, String> {
        match (&self.bytes, &self.identity, &self.modified) {
            (None, None, None) => Ok(None),
            (Some(bytes), Some(id), Some(time)) if id.len() <= 256 && time.len() <= 128 => {
                let bytes = B64.decode(bytes).map_err(|_| {
                    crate::i18n::text("backend-invalid-ignore-file-snapshot-f7dbde")
                })?;
                if bytes.len() > LIMIT || bytes.contains(&0) || std::str::from_utf8(&bytes).is_err()
                {
                    return Err(crate::i18n::text(
                        "backend-ignore-file-must-be-utf-8-text-within64kib-75c96a",
                    )
                    .into());
                }
                Ok(Some(bytes))
            }
            _ => Err(crate::i18n::text("backend-incomplete-ignore-file-identity-1bffaf").into()),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Preview {
    pub commit: String,
    pub patch: String,
    pub existing: String,
    repo: std::path::PathBuf,
    identity: Identity,
    expected: Snapshot,
    _held: Option<Arc<std::fs::File>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Appended {
    Changed,
    AlreadyPresent,
}
fn contains(bytes: &[u8], commit: &str) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| {
        text.lines()
            .any(|line| line.split('#').next().unwrap_or("").trim() == commit)
    })
}
fn payload(commit: &str) -> Vec<u8> {
    format!("\n# gitcraft: inspected whitespace-only commit\n{commit}\n").into_bytes()
}
/// Git -w is a whitespace heuristic, including inside strings. The UI must expose the
/// complete patch for deliberate human inspection; it must not call this semantic proof.
fn whitespace_patch(reader: &Reader<'_>, commit: &str) -> Result<String, String> {
    if !oid(commit) {
        return Err(
            crate::i18n::text("backend-choose-an-immutable-formatting-commit-0a1ec4").into(),
        );
    }
    let parents = reader.run(
        &[
            "show",
            "-s",
            "--no-show-signature",
            "--format=%P",
            commit,
            "--",
        ],
        vec![],
        &[0],
    )?;
    let parents = std::str::from_utf8(&parents)
        .map_err(|_| crate::i18n::text("backend-invalid-commit-parents-8430cf"))?;
    let parents: Vec<_> = parents.split_whitespace().collect();
    let [parent] = parents.as_slice() else {
        return Err(crate::i18n::text(
            "backend-only-single-parent-whitespace-commits-can-be-added-here-747852",
        )
        .into());
    };
    if !oid(parent) {
        return Err(crate::i18n::text("backend-invalid-formatting-parent-1a34d8").into());
    }
    let raw = reader.run(
        &[
            "diff-tree",
            "-r",
            "--raw",
            "-z",
            "--no-commit-id",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            parent,
            commit,
            "--",
        ],
        vec![],
        &[0],
    )?;
    let mut fields = raw.split(|b| *b == 0).filter(|s| !s.is_empty());
    let mut files = 0;
    let mut total = 0usize;
    while let Some(header) = fields.next() {
        files += 1;
        if files > 16 {
            return Err(crate::i18n::text(
                "backend-formatting-preview-is-limited-to16changed-files-417caf",
            )
            .into());
        }
        let _path = fields.next().ok_or(crate::i18n::text(
            "backend-truncated-formatting-path-4a462e",
        ))?;
        let header = std::str::from_utf8(header)
            .map_err(|_| crate::i18n::text("backend-invalid-formatting-metadata-0e00ea"))?;
        let f: Vec<_> = header.trim_start_matches(':').split_whitespace().collect();
        let [old_mode, new_mode, old, new, status] = f.as_slice() else {
            return Err(crate::i18n::text("backend-malformed-formatting-change-c905a3").into());
        };
        if old_mode != new_mode
            || !matches!(*old_mode, "100644" | "100755")
            || *status != "M"
            || !oid(old)
            || !oid(new)
        {
            return Err(crate::i18n::text(
                "backend-formatting-candidates-cannot-add-delete-rename-files-change-modes-482a3d",
            )
            .into());
        }
        let mut versions = Vec::new();
        for id in [*old, *new] {
            let size = reader.run(&["cat-file", "-s", id], vec![], &[0])?;
            let size = std::str::from_utf8(&size)
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .ok_or(crate::i18n::text(
                    "backend-invalid-formatting-blob-size-1cf4e4",
                ))?;
            total = total.saturating_add(size);
            if size > 2 * 1024 * 1024 || total > 8 * 1024 * 1024 {
                return Err(crate::i18n::text(
                    "backend-formatting-preview-exceeds2mib-blob-or8mib-total-f805cb",
                )
                .into());
            }
            let bytes = reader.run(&["cat-file", "blob", id], vec![], &[0])?;
            if bytes.len() != size || bytes.contains(&0) {
                return Err(crate::i18n::text(
                    "backend-binary-incomplete-versions-cannot-be-classified-as-formatting-4d9fbb",
                )
                .into());
            }
            let actual = reader.run(
                &["hash-object", "--no-filters", "--stdin"],
                bytes.clone(),
                &[0],
            )?;
            if actual.trim_ascii() != id.as_bytes() {
                return Err(
                    crate::i18n::text("backend-formatting-object-identity-changed-21c4eb").into(),
                );
            }
            versions.push(
                String::from_utf8(bytes).map_err(|_| {
                    crate::i18n::text("backend-non-utf-8-formatting-version-f771ba")
                })?,
            );
        }
        let normalized = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        if normalized(&versions[0]) != normalized(&versions[1]) {
            return Err(
                crate::i18n::text("backend-commit-contains-non-whitespace-changes-5b4389").into(),
            );
        }
    }
    if files == 0 {
        return Err(
            crate::i18n::text("backend-commit-has-no-material-whitespace-change-e918f2").into(),
        );
    }
    reader
        .run(
            &[
                "diff",
                "--quiet",
                "-w",
                "--no-ext-diff",
                "--no-textconv",
                parent,
                commit,
                "--",
            ],
            vec![],
            &[0],
        )
        .map_err(|_| {
            crate::i18n::text(
                "backend-git-w-finds-additional-changes-inspect-this-commit-manually-242aca",
            )
        })?;
    let patch = reader.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            parent,
            commit,
            "--",
        ],
        vec![],
        &[0],
    )?;
    if patch.len() > 1024 * 1024 {
        return Err(crate::i18n::text(
            "backend-full-formatting-patch-exceeds1mib-inspect-it-manually-ea3911",
        )
        .into());
    }
    String::from_utf8(patch)
        .map_err(|_| crate::i18n::text("backend-formatting-patch-is-not-utf-8-55702b").into())
}
pub(crate) fn prepare(
    repo: &Path,
    commit: &str,
    active: &dyn Fn() -> bool,
) -> Result<Preview, String> {
    let patch = whitespace_patch(&Reader::new(repo, active), commit)?;
    let identity = recovery::pinned_worktree(repo, active)?;
    let (expected, held) = snapshot(&identity, active)?;
    let bytes = expected.decoded()?.unwrap_or_default();
    let existing = String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("backend-ignore-file-is-not-utf-8-f2fc41"))?;
    Ok(Preview {
        commit: commit.into(),
        patch,
        existing,
        repo: repo.into(),
        identity,
        expected,
        _held: held,
    })
}
pub(crate) fn perform(preview: &Preview, active: &dyn Fn() -> bool) -> Result<Appended, String> {
    whitespace_patch(&Reader::new(&preview.repo, active), &preview.commit)?;
    if !active() {
        return Err("Ignore edit cancelled before publication".into());
    }
    if let Some(host) = &preview.identity.host {
        let value = remote_call(
            &preview.identity,
            host,
            "append",
            &preview.expected,
            Some(&payload(&preview.commit)),
            active,
        )?;
        return match value.get("result").and_then(serde_json::Value::as_str) {
            Some("changed") => Ok(Appended::Changed),
            Some("present") => Ok(Appended::AlreadyPresent),
            _ => Err(crate::i18n::text(
                "backend-ignore-append-did-not-return-a-complete-acknowledgement-inspect-t-62f845",
            )
            .into()),
        };
    }
    local_append(
        &preview.identity,
        &preview.expected,
        &preview.commit,
        active,
    )
}
fn snapshot(
    identity: &Identity,
    active: &dyn Fn() -> bool,
) -> Result<(Snapshot, Option<Arc<std::fs::File>>), String> {
    if !active() {
        return Err("Ignore preparation cancelled".into());
    }
    if let Some(host) = &identity.host {
        let value = remote_call(identity, host, "read", &Snapshot::missing(), None, active)?;
        let state: Snapshot = serde_json::from_value(value)
            .map_err(|_| crate::i18n::text("backend-invalid-remote-ignore-snapshot-b7ddaf"))?;
        state.decoded()?;
        return Ok((state, None));
    }
    local_snapshot(identity)
}
fn remote_call(
    identity: &Identity,
    host: &crate::remote::Host,
    op: &str,
    expected: &Snapshot,
    bytes: Option<&[u8]>,
    active: &dyn Fn() -> bool,
) -> Result<serde_json::Value, String> {
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-pin-9ed8c4"))?;
    let request =
        serde_json::json!({"op":op,"expected":expected,"payload":bytes.map(|b|B64.encode(b))});
    let script = format!(
        "umask 077\ncd -P -- {} || exit 1\n{}exec python3 -c {}\n",
        crate::remote::sh_quote(&pin.physical),
        pin.remote_guard(),
        crate::remote::sh_quote(REMOTE)
    );
    let out = crate::remote::exec(
        host,
        &crate::remote::Request {
            script,
            stdin: Some(request.to_string().into_bytes()),
            timeout: Some(std::time::Duration::from_secs(30)),
            long: true,
            ..Default::default()
        },
    )?;
    // Once deliberate append succeeds, late cancellation must not conceal its outcome.
    if out.code != 0 || !out.stderr.is_empty() || out.stdout.len() > 256 * 1024 {
        return Err(crate::i18n::text(
            "backend-remote-ignore-edit-refused-or-was-interrupted-python3-and-stable-9bad3b",
        )
        .into());
    }
    if op == "read" && !active() {
        return Err("Ignore preview cancelled".into());
    }
    serde_json::from_slice(&out.stdout).map_err(|_| {
        crate::i18n::text("backend-malformed-remote-ignore-file-acknowledgement-6fc30e").into()
    })
}

// No shell redirection opens an existing path. Python opens exactly once relative to
// the already pinned cwd, with NOFOLLOW/NONBLOCK and fstat checks before any I/O.
const REMOTE: &str = r#"
import os,sys,json,stat,base64
LIMIT=65536
NAME='.git-blame-ignore-revs'
os.umask(0o077)
data=sys.stdin.buffer.read(262145)
if len(data)>262144: raise ValueError('input budget')
r=json.loads(data)
if r['op'] not in ('read','append'): raise ValueError('operation')
root=os.open('.',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
def snapshot(fd):
    before=os.fstat(fd)
    if not stat.S_ISREG(before.st_mode) or before.st_nlink!=1: raise ValueError('not a plain private file')
    chunks=[];remaining=LIMIT+1
    while remaining:
        block=os.read(fd,min(8192,remaining))
        if not block: break
        chunks.append(block);remaining-=len(block)
    b=b''.join(chunks)
    after=os.fstat(fd)
    if len(b)>LIMIT or b'\x00' in b: raise ValueError('file budget/type')
    b.decode('utf-8')
    if (before.st_dev,before.st_ino,before.st_mtime_ns,before.st_size)!=(after.st_dev,after.st_ino,after.st_mtime_ns,after.st_size): raise ValueError('changed while reading')
    return {'bytes':base64.b64encode(b).decode('ascii'),'identity':str(after.st_dev)+':'+str(after.st_ino),'modified':str(after.st_mtime_ns)}
flags=os.O_NOFOLLOW|os.O_NONBLOCK
if r['op']=='read':
    try: fd=os.open(NAME,flags|os.O_RDONLY,dir_fd=root)
    except FileNotFoundError: answer={'bytes':None,'identity':None,'modified':None}
    else:
        try: answer=snapshot(fd)
        finally: os.close(fd)
else:
    expected=r['expected']
    if set(expected)!=set(('bytes','identity','modified')): raise ValueError('snapshot fields')
    existing=expected['bytes'] is not None
    if existing:
        if not isinstance(expected['identity'],str) or not isinstance(expected['modified'],str): raise ValueError('snapshot identity')
        if len(base64.b64decode(expected['bytes'],validate=True))>LIMIT: raise ValueError('snapshot budget')
    elif expected!={'bytes':None,'identity':None,'modified':None}: raise ValueError('missing snapshot')
    b=base64.b64decode(r['payload'],validate=True)
    if len(b)>256 or not b.startswith(b'\n# gitcraft: inspected whitespace-only commit\n') or not b.endswith(b'\n'): raise ValueError('payload')
    cid=b.splitlines()[-1]
    if len(cid) not in (40,64) or any(c not in b'0123456789abcdefABCDEF' for c in cid): raise ValueError('object id')
    flags|=os.O_APPEND|(os.O_RDWR if existing else os.O_WRONLY|os.O_CREAT|os.O_EXCL)
    fd=os.open(NAME,flags,0o600,dir_fd=root)
    try:
        current=snapshot(fd) if existing else None
        if existing and current!=expected: raise ValueError('stale preview')
        old=base64.b64decode(current['bytes']) if existing else b''
        present=any(line.split(b'#',1)[0].strip()==cid for line in old.splitlines())
        if present: answer={'result':'present'}
        else:
            if os.write(fd,b)!=len(b): raise ValueError('partial append; inspect file')
            os.fsync(fd)
            answer={'result':'changed'}
    finally: os.close(fd)
os.close(root)
sys.stdout.write(json.dumps(answer,separators=(',',':')))
"#;

#[cfg(test)]
mod tests;

mod native;
use native::local_append;

use native::local_snapshot;
