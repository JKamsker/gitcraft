//! The packaged resource wrapper runs only as an already-owned subprocess, before UI
//! startup. POSIX exec keeps the same process/group; Windows uses the shared Service.
use std::ffi::{OsStr, OsString};
use std::process::Command;

pub(crate) fn program() -> Result<String, String> {
    super::command::git_cmd()
        .get_program()
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| crate::i18n::text("backend-git-path-is-not-utf-8-35d0c4").into())
}

pub(crate) fn entrypoint() -> i32 {
    let result = run(std::env::args_os().skip(2).collect());
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Contained bundle Git: {error}");
            78
        }
    }
}
fn configure(
    command: &mut Command,
    alternate: &OsStr,
    env: impl Iterator<Item = (OsString, OsString)>,
) {
    // A private quarantine must not inherit injected config, object stores, indexes,
    // replacement refs or lazy-fetch policy from an enclosing Git shell alias.
    for (key, _) in env {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    if !alternate.is_empty() {
        command.env("GIT_ALTERNATE_OBJECT_DIRECTORIES", alternate);
    }
}
fn run(args: Vec<OsString>) -> Result<i32, String> {
    let [program, alternate, separator, rest @ ..] = args.as_slice() else {
        return Err("invalid resource helper arguments".into());
    };
    if program.is_empty() || separator != "--" || rest.is_empty() {
        return Err("invalid resource helper arguments".into());
    }
    // Establish the kernel limit before reading input or starting Git. This call never
    // occurs in the GUI; failure has no fallback that opens the incoming pack.
    let _resources = gitcraft_os::BundleResources::install()?;
    if program == "--retire-owned-keep" {
        let [hash, message] = rest else {
            return Err("invalid keep retirement arguments".into());
        };
        gitcraft_os::retire_bundle_keep(
            std::path::Path::new(alternate),
            hash.to_str().ok_or("invalid keep hash")?,
            message.to_str().ok_or("invalid keep message")?,
        )
        .map_err(|e| e.to_string())?;
        return Ok(0);
    }
    let mut command = Command::new(program);
    configure(&mut command, alternate, std::env::vars_os());
    command.args(rest);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().to_string())
    }
    #[cfg(windows)]
    {
        use std::io::{Read, Write};
        let mut input = Vec::new();
        std::io::stdin()
            .take((super::bundles::BUNDLE_LIMIT + 1) as u64)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > super::bundles::BUNDLE_LIMIT {
            return Err("Input exceeds 128 MiB".into());
        }
        let result = crate::processes::run(
            command,
            crate::processes::Options {
                input: Some(input),
                output_limit: 2 << 20,
                stderr_limit: Some(2 << 20),
                timeout: Some(std::time::Duration::from_secs(120)),
                background: true,
                ..Default::default()
            },
        )
        .and_then(|o| o.complete());
        crate::processes::shutdown();
        let out = result?;
        std::io::stdout()
            .write_all(&out.stdout)
            .map_err(|e| e.to_string())?;
        std::io::stderr()
            .write_all(&out.stderr)
            .map_err(|e| e.to_string())?;
        out.status
            .code()
            .ok_or_else(|| "Git did not return an exit code".into())
    }
    #[cfg(not(any(unix, windows)))]
    Err("Unsupported resource helper platform".into())
}

/// The remote POSIX owner already owns this exact Python process and its inherited
/// group. Refuse non-Linux or missing resource support before exec, with no fallback.
pub(crate) const REMOTE: &str = r#"import os,resource,sys
if sys.platform != 'linux':
 sys.stderr.write('Bundle unpacking requires Linux hard address-space limits on this host; no pack was opened.\n'); sys.exit(78)
for kind,cap in [(resource.RLIMIT_AS,1073741824),(resource.RLIMIT_FSIZE,536870912),(resource.RLIMIT_CORE,0),(resource.RLIMIT_CPU,120)]:
 old=resource.getrlimit(kind); hard=min(cap,old[1]) if old[1]!=resource.RLIM_INFINITY else cap
 soft=min(old[0],hard) if old[0]!=resource.RLIM_INFINITY else hard
 resource.setrlimit(kind,(soft,hard))
 if resource.getrlimit(kind)!=(soft,hard): raise RuntimeError('hard resource limit not confirmed')
alternate=sys.argv[1]
env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='/dev/null',GIT_CONFIG_SYSTEM='/dev/null',GIT_NO_REPLACE_OBJECTS='1',GIT_NO_LAZY_FETCH='1',GIT_TERMINAL_PROMPT='0')
if alternate: env['GIT_ALTERNATE_OBJECT_DIRECTORIES']=alternate
os.execvpe('git',['git']+sys.argv[2:],env)
"#;

/// Narrow post-publication cleanup, run through the same actual-machine fenced owner.
/// No glob, scan, PID, or caller-chosen filename; a mismatching marker is retained.
pub(crate) const RETIRE_KEEP_REMOTE: &str = r#"import os,stat,sys
common,h,msg=sys.argv[1:]
hexes='0123456789abcdef';prefix='gitcraft-bundle-keep-v1-'
if len(h) not in (40,64) or any(c not in hexes for c in h) or not msg.startswith(prefix) or len(msg)!=len(prefix)+32 or any(c not in hexes for c in msg[len(prefix):]): raise RuntimeError('invalid keep ownership')
d=os.open(common+'/objects/pack',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
name='pack-'+h+'.keep'
f=os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=d)
s=os.fstat(f)
if not stat.S_ISREG(s.st_mode) or s.st_nlink!=1 or s.st_uid!=os.geteuid() or os.read(f,len(msg)+2)!=(msg+'\n').encode(): raise RuntimeError('keep ownership changed; retained')
current=os.stat(name,dir_fd=d,follow_symlinks=False)
if (current.st_dev,current.st_ino)!=(s.st_dev,s.st_ino): raise RuntimeError('keep identity changed; retained')
os.unlink(name,dir_fd=d)
os.close(f);os.close(d)
"#;

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn helper_removes_injected_git_environment_but_preserves_non_git_settings() {
        let mut command = Command::new("recording-git");
        configure(
            &mut command,
            OsStr::new("\"/verified/objects\""),
            [
                ("GIT_CONFIG_COUNT", "3"),
                ("GIT_DIR", "wrong"),
                ("GIT_OBJECT_DIRECTORY", "wrong"),
                ("GIT_ALTERNATE_OBJECT_DIRECTORIES", "wrong"),
                ("PATH", "test-path"),
                ("SECRET", "unrelated"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into())),
        );
        let env: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_os_string(), v.map(OsStr::to_os_string)))
            .collect();
        assert_eq!(env[OsStr::new("GIT_DIR")], None);
        assert_eq!(env[OsStr::new("GIT_CONFIG_COUNT")], None);
        assert_eq!(
            env[OsStr::new("GIT_ALTERNATE_OBJECT_DIRECTORIES")],
            Some("\"/verified/objects\"".into())
        );
        assert_eq!(env[OsStr::new("GIT_NO_LAZY_FETCH")], Some("1".into()));
        assert!(!env.contains_key(OsStr::new("PATH")) && !env.contains_key(OsStr::new("SECRET")));
        assert!(REMOTE.contains("sys.platform != 'linux'"));
        assert!(REMOTE.find("resource.setrlimit").unwrap() < REMOTE.find("os.execvpe").unwrap());
    }
}
