//! Keep access permissions when Git apply replaces a file. Never chmod a path by name.
use super::*;
#[cfg(unix)]
use std::io::Read;
pub(super) struct Pin {
    identity: crate::git::recovery::Identity,
    mode: u32,
}
impl Pin {
    pub(super) fn bits(&self) -> u32 {
        self.mode
    }
    pub fn capture(repo: &Path, path: &str, active: &dyn Fn() -> bool) -> Result<Self, String> {
        let identity = crate::git::recovery::pinned_worktree(repo, active)?;
        let mode = if identity.host.is_some() {
            remote_mode(&identity, path, None, None, active)?
        } else {
            native_mode(&identity, path, None, None)?
        };
        Ok(Self { identity, mode })
    }
    pub fn restore(
        self,
        path: &str,
        bytes: &[u8],
        active: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if self.identity.host.is_some() {
            remote_mode(&self.identity, path, Some(self.mode), Some(bytes), active)?;
        } else {
            native_mode(&self.identity, path, Some(self.mode), Some(bytes))?;
        }
        Ok(())
    }
}
#[cfg(unix)]
fn native_mode(
    identity: &crate::git::recovery::Identity,
    path: &str,
    mode: Option<u32>,
    expected: Option<&[u8]>,
) -> Result<u32, String> {
    use rustix::fs::{Mode, OFlags, openat};
    use std::os::unix::fs::PermissionsExt;
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-working-root-pin-117323"))?;
    let (dir, name) = crate::git::recovery::pinned_parent(
        Path::new(&pin.physical),
        Path::new(&identity.common),
        path,
        pin,
    )?;
    let mut file = std::fs::File::from(
        openat(
            &dir,
            name.as_str(),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(crate::i18n::text(
            "backend-permission-restoration-requires-a-regular-file-0bc5a9",
        )
        .into());
    }
    let before = meta.permissions().mode() & 0o777;
    if let (Some(mode), Some(expected)) = (mode, expected) {
        let mut bytes = Vec::new();
        file.by_ref()
            .take(FILE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes != expected {
            return Err(crate::i18n::text(
                "backend-recovered-file-changed-before-permission-restoration-fd07d2",
            )
            .into());
        }
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())?;
    }
    Ok(before)
}
#[cfg(not(unix))]
fn native_mode(
    _: &crate::git::recovery::Identity,
    _: &str,
    _: Option<u32>,
    _: Option<&[u8]>,
) -> Result<u32, String> {
    Ok(0)
}
fn remote_mode(
    identity: &crate::git::recovery::Identity,
    path: &str,
    mode: Option<u32>,
    expected: Option<&[u8]>,
    active: &dyn Fn() -> bool,
) -> Result<u32, String> {
    use base64::Engine;
    let pin = identity.root_pin.as_ref().ok_or(crate::i18n::text(
        "backend-missing-remote-root-identity-18406b",
    ))?;
    let script = r#"import os,sys,json,stat,base64
q=json.load(sys.stdin)
fd=os.open('.',os.O_RDONLY|os.O_DIRECTORY)
root=os.fstat(fd)
if os.getcwd()!=q['root'] or str(root.st_dev)!=q['device'] or str(root.st_ino)!=q['inode']:raise ValueError('working root changed')
try:
 for part in q['path'].split('/')[:-1]:
  new=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd);os.close(fd);fd=new
 file=os.open(q['path'].split('/')[-1],os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=fd)
 try:
  meta=os.fstat(file)
  if not stat.S_ISREG(meta.st_mode):raise ValueError('regular file required')
  if q['mode'] is not None:
   expected=base64.b64decode(q['expected'],validate=True)
   data=b''
   while len(data)<=len(expected):
    chunk=os.read(file,min(65536,len(expected)+1-len(data)))
    if not chunk:break
    data+=chunk
   if data!=expected:raise ValueError('recovered file changed')
   os.fchmod(file,q['mode'])
  print(stat.S_IMODE(meta.st_mode)&511)
 finally:os.close(file)
finally:os.close(fd)
"#;
    if !active() {
        return Err("Draft permissions cancelled".into());
    }
    let output=git::forecast_remote(identity.host.as_ref().ok_or(crate::i18n::text("backend-missing-remote-host-584e40"))?,remote::Request {
        cwd:Some(pin.physical.clone()),script:format!("exec python3 -c {}",remote::sh_quote(script)),
        stdin:Some(serde_json::json!({"root":pin.physical,"device":pin.device_inode().map(|p|p.0),"inode":pin.device_inode().map(|p|p.1),"path":path,"mode":mode,"expected":expected.map(|b|base64::engine::general_purpose::STANDARD.encode(b))}).to_string().into_bytes()),..Default::default()
    },active)?;
    if output.code != 0 {
        return Err(crate::i18n::text(
            "backend-remote-permission-preservation-requires-python-3-and-stable-regul-c8957f",
        )
        .into());
    }
    std::str::from_utf8(&output.stdout)
        .map_err(|_| crate::i18n::text("backend-invalid-remote-permissions-c99226"))?
        .trim()
        .parse::<u32>()
        .map_err(|_| crate::i18n::text("backend-invalid-remote-permissions-c99226").into())
}
