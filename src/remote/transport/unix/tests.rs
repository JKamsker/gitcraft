use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn fixture() -> Result<Manager, Box<dyn std::error::Error>> {
    Ok(Manager::new()?)
}

fn effective(config: &Path, host: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("ssh")
        .args(["-G", "-F"])
        .arg(config)
        .args(["--", host])
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8(out.stdout)?)
}

#[test]
fn sockets_and_configuration_are_private_and_instance_local()
-> Result<(), Box<dyn std::error::Error>> {
    let a = fixture()?;
    let b = fixture()?;
    assert_ne!(a.dir, b.dir);
    assert_eq!(
        std::fs::metadata(&a.dir)?.permissions().mode() & 0o777,
        0o700
    );
    let config = a.config_with("identity", &[])?;
    assert_eq!(
        std::fs::metadata(&config.path)?.permissions().mode() & 0o777,
        0o600
    );
    let text = std::fs::read_to_string(&config.path)?;
    assert!(text.contains("ControlMaster auto"));
    assert!(text.contains("ControlPersist 300"));
    let socket = a
        .dir
        .join("1234567890123456-1234567890123456789012345678901234567890");
    assert!(
        socket.as_os_str().len() + 17 < 104,
        "OpenSSH's temporary socket must fit too"
    );
    std::fs::remove_dir_all(a.dir)?;
    std::fs::remove_dir_all(b.dir)?;
    Ok(())
}

#[test]
fn explicit_user_choices_and_aliases_precede_managed_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let source = manager.dir.join("user config");
    std::fs::write(
        &source,
        "Host named\n HostName example.invalid\n User alice\n Port 2222\n ControlMaster no\n ControlPath none\n ControlPersist 17\n ProxyJump bastion\nHost unrelated\n User wrong\n",
    )?;
    let config = manager.config_with("explicit", &[source])?;
    let out = effective(&config.path, "named")?;
    for expected in [
        "hostname example.invalid\n",
        "user alice\n",
        "port 2222\n",
        "controlmaster false\n",
        "controlpersist 17\n",
        "proxyjump bastion\n",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in {out}");
    }
    assert!(
        !out.contains(".sock"),
        "ControlPath none must disable the managed path"
    );
    let out = effective(&config.path, "unconfigured")?;
    assert!(out.contains("controlmaster auto\n"));
    assert!(out.contains("controlpersist 300\n"));
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn final_pass_choices_precede_managed_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let source = manager.dir.join("final-config");
    std::fs::write(
        &source,
        "Host named\n HostName example.invalid\nMatch final\n ControlMaster no\n ControlPath none\n ControlPersist 0\n",
    )?;
    let original = effective(&source, "named")?;
    let config = manager.config_with("final", &[source])?;
    let wrapped = effective(&config.path, "named")?;
    for prefix in ["controlmaster ", "controlpath ", "controlpersist "] {
        assert_eq!(
            original.lines().find(|l| l.starts_with(prefix)),
            wrapped.lines().find(|l| l.starts_with(prefix))
        );
    }
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn separate_config_roots_do_not_inherit_the_previous_host_condition()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let user = manager.dir.join("user");
    let system = manager.dir.join("system");
    std::fs::write(&user, "Host unrelated\n User wrong\n")?;
    std::fs::write(&system, "Host named\n User system-policy\n Port 2022\n")?;
    let config = manager.config_with("roots", &[user, system])?;
    let out = effective(&config.path, "named")?;
    assert!(out.contains("user system-policy\n"));
    assert!(out.contains("port 2022\n"));
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn context_changes_for_config_includes_keys_and_agent_but_not_repository()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let home = manager.dir.join("home");
    let ssh = home.join(".ssh");
    std::fs::create_dir_all(ssh.join("config.d"))?;
    std::fs::write(
        ssh.join("config"),
        "Include config.d/*\nHost *\n IdentityFile ~/.ssh/key\n",
    )?;
    std::fs::write(ssh.join("key"), "key-a")?;
    let mut env = BTreeMap::from([(OsString::from("HOME"), home.into_os_string())]);
    let key =
        |env: &BTreeMap<OsString, OsString>| security_context_with("alice@server:2222", env).0;
    let first = key(&env);
    assert_eq!(first, key(&env));
    assert_ne!(first, security_context_with("bob@server:2222", &env).0);
    assert_ne!(first, security_context_with("alice@server:2223", &env).0);
    std::fs::write(ssh.join("config.d/added"), "Host *\n IdentitiesOnly yes\n")?;
    let included = key(&env);
    assert_ne!(first, included);
    std::fs::write(ssh.join("key"), "key-b-is-different")?;
    let changed_key = key(&env);
    assert_ne!(included, changed_key);
    let mut previous = changed_key.clone();
    for (file, contents) in [
        ("key.pub", "public-identity"),
        ("key-cert", "certificate-no-suffix"),
        ("key-cert.pub", "implicit-certificate"),
    ] {
        std::fs::write(ssh.join(file), contents)?;
        let changed = key(&env);
        assert_ne!(
            previous, changed,
            "{file} must invalidate authentication context"
        );
        previous = changed;
    }
    env.insert(
        OsString::from("SSH_AUTH_SOCK"),
        OsString::from("/another/agent"),
    );
    assert_ne!(changed_key, key(&env));
    env.insert(
        OsString::from("GITCRAFT_SSH_CONFIG"),
        ssh.join("other-config").into_os_string(),
    );
    assert_ne!(changed_key, key(&env));
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn relative_identity_files_are_fingerprinted_relative_to_process_cwd()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let relative = PathBuf::from("target").join(format!(
        "ssh-key-{}",
        super::super::super::askpass::random_token()
    ));
    std::fs::create_dir_all("target")?;
    std::fs::write(&relative, "key-before")?;
    let source = manager.dir.join("config");
    std::fs::write(&source, format!("IdentityFile {}\n", relative.display()))?;
    let env = BTreeMap::from([(
        OsString::from("GITCRAFT_SSH_CONFIG"),
        source.into_os_string(),
    )]);
    let before = security_context_with("named", &env).0;
    std::fs::write(&relative, "different-key-after")?;
    assert_ne!(before, security_context_with("named", &env).0);
    std::fs::remove_file(relative)?;
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn passwd_home_is_used_even_when_home_environment_is_overridden()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let env = BTreeMap::from([(OsString::from("HOME"), manager.dir.clone().into_os_string())]);
    let real_home = account_home().ok_or("no passwd home")?;
    let (_, roots) = security_context_with_home("named", &env, Some(real_home));
    assert_eq!(roots.first(), Some(&real_home.join(".ssh/config")));
    assert!(!roots.contains(&manager.dir.join(".ssh/config")));
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn channel_start_gate_serializes_starts_and_failure_releases_waiters()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let config = manager.config_with("gate", &[])?;
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let config = Arc::clone(&config);
            let active = &active;
            let peak = &peak;
            scope.spawn(move || {
                let _opening = config.take();
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(count, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                active.fetch_sub(1, Ordering::SeqCst);
                // Dropping on success, authentication failure, or spawn failure is identical.
            });
        }
    });
    assert_eq!(peak.load(Ordering::SeqCst), 1);
    assert!(config.take().is_some());
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn dynamic_conditions_and_unresolved_security_tokens_disable_reuse()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let source = manager.dir.join("dynamic");
    for text in [
        "Host *\n IdentityFile ~/.ssh/key-%h\n",
        "Host *\n UserKnownHostsFile ~/.ssh/known-%r\n",
        "Host *\n IdentityAgent /tmp/agent-%u\n",
        "Host *\n IdentityFile ~another/.ssh/key\n",
        "Include config.d/[ab].conf\n",
        "Include ~another/.ssh/config\n",
        "Match exec true\n User alice\n",
        "Match !localnetwork 192.0.2.0/24\n User alice\n",
    ] {
        std::fs::write(&source, text)?;
        assert!(
            dynamic_file(
                &source,
                &ConfigSource {
                    include_base: &manager.dir,
                    home: Some(&manager.dir),
                    env: &BTreeMap::new()
                },
                0
            ),
            "{text}"
        );
    }
    std::fs::write(
        &source,
        "Host *\n IdentityFile ~/.ssh/fixed-key\nMatch final\n ControlMaster no\n",
    )?;
    assert!(!dynamic_file(
        &source,
        &ConfigSource {
            include_base: &manager.dir,
            home: Some(&manager.dir),
            env: &BTreeMap::new()
        },
        0
    ));
    std::fs::write(&source, "Include relative/*.conf\n")?;
    assert!(!portable_system_includes(&source, 0));
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn nested_include_paths_keep_the_original_user_or_system_base()
-> Result<(), Box<dyn std::error::Error>> {
    let manager = fixture()?;
    let home = manager.dir.join("home");
    let user_base = home.join(".ssh");
    let system_base = manager.dir.join("system");
    let outside = manager.dir.join("outside");
    for directory in [&user_base, &system_base, &outside] {
        std::fs::create_dir_all(directory)?;
    }
    let root = manager.dir.join("root-config");
    let nested = outside.join("nested-config");
    std::fs::write(&root, format!("Include \"{}\"\n", nested.display()))?;
    std::fs::write(&nested, "Include next.conf\n")?;
    let user_next = user_base.join("next.conf");
    let system_next = system_base.join("next.conf");
    std::fs::write(&user_next, "Host *\n User original-user\n")?;
    std::fs::write(&system_next, "Host *\n User system-policy\n")?;
    let env = BTreeMap::new();
    let user = ConfigSource {
        include_base: &user_base,
        home: Some(&home),
        env: &env,
    };
    let system = ConfigSource {
        include_base: &system_base,
        home: Some(&home),
        env: &env,
    };
    let fingerprint = |source: &ConfigSource<'_>| {
        let mut digest = Sha256::new();
        fingerprint_config(&root, source, &mut Vec::new(), &mut digest, 0);
        digest.finalize().to_vec()
    };
    let user_before = fingerprint(&user);
    let system_before = fingerprint(&system);
    std::fs::write(&user_next, "Match exec true\n User changed-user\n")?;
    assert_ne!(user_before, fingerprint(&user));
    assert_eq!(system_before, fingerprint(&system));
    assert!(dynamic_file(&root, &user, 0));
    assert!(!dynamic_file(&root, &system, 0));
    let user_after = fingerprint(&user);
    std::fs::write(&system_next, "Host *\n User changed-system-policy\n")?;
    assert_ne!(system_before, fingerprint(&system));
    assert_eq!(user_after, fingerprint(&user));
    assert_eq!(
        source_include_base(Path::new("/etc/ssh/ssh_config"), Some(&home), false),
        Path::new("/etc/ssh")
    );
    assert_eq!(
        source_include_base(Path::new("/etc/ssh/ssh_config"), Some(&home), true),
        user_base
    );
    assert_eq!(
        source_include_base(Path::new("/etc/ssh/custom"), Some(&home), true),
        user_base
    );
    std::fs::remove_dir_all(manager.dir)?;
    Ok(())
}

#[test]
fn root_config_filenames_are_never_reinterpreted_as_include_tokens() {
    for root in ["/tmp/config-%h", "/tmp/${HOME}/config"] {
        assert!(configuration(&[PathBuf::from(root)], Path::new("/tmp/socket-%C")).is_err());
    }
    let generated = configuration(
        &[PathBuf::from("/tmp/literal config")],
        Path::new("/tmp/socket-%C"),
    );
    assert!(generated.is_ok_and(|text| text.contains("socket-%C")));
}

#[test]
fn config_path_newlines_are_rejected() {
    assert!(
        configuration(
            &[PathBuf::from("/tmp/config\nProxyCommand false")],
            Path::new("/tmp/sock")
        )
        .is_err()
    );
}

/// Actual OpenSSH, not a fake multiplexer: disposable server/config supplied by the harness.
/// GITCRAFT_SSH_TEST_HOST must differ from the special in-process `gitcraft-loopback` host.
#[test]
#[ignore = "requires a disposable real sshd; run ci/test-ssh-transport.py"]
fn real_openssh_reuses_concurrently_and_recovers() -> Result<(), Box<dyn std::error::Error>> {
    use super::super::super::{Host, Request};
    let dest = std::env::var("GITCRAFT_SSH_TEST_HOST")?;
    assert_ne!(dest, super::super::super::session::LOOPBACK);
    let host = Host::Ssh(dest.clone());
    let request = |script: &str| Request {
        script: script.to_owned(),
        ..Request::default()
    };
    let connection = |script: &str| -> Result<String, String> {
        let output = super::super::super::exec(&host, &request(script))?;
        if output.code != 0 {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let first = connection("printf '%s' \"$SSH_CONNECTION\"")?;
    assert!(!first.is_empty());
    let since = Instant::now();
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| connection("sleep 0.2; printf '%s' \"$SSH_CONNECTION\"")))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().map_err(|_| "worker panicked".to_owned())?)
            .collect::<Result<Vec<_>, String>>()
    })?;
    assert!(
        results.iter().all(|id| id == &first),
        "all shells must share a TCP connection: {results:?}"
    );
    assert!(
        since.elapsed() < Duration::from_millis(1100),
        "independent commands must run concurrently"
    );
    let stream = super::super::super::spawn(&host, &request("printf '%s' \"$SSH_CONNECTION\""))?
        .wait_with_output()?;
    assert!(stream.status.success());
    assert_eq!(String::from_utf8_lossy(&stream.stdout).trim(), first);
    // Preparing a streaming request must not reserve the pooled-shell startup gate
    // while it waits in the process service queue: cancellation cleanup needs that gate.
    let queued = super::super::super::streaming_command(&host, &request("true"))?;
    let same_config = config(&dest).ok_or("no managed configuration")?;
    let (sent, received) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let opening = same_config.take();
        let _ = sent.send(opening.is_some());
    });
    let available = received.recv_timeout(Duration::from_secs(1));
    drop(queued);
    waiter.join().map_err(|_| "gate waiter panicked")?;
    assert_eq!(
        available,
        Ok(true),
        "queued streaming preparation must leave cleanup free to start"
    );
    // The server allows ten sessions. Six idle pooled shells plus twelve streaming
    // clients exceed that limit: OpenSSH auto must fall back to independent connections
    // for refused mux channels without retrying a command that already started.
    let overflow = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..12)
            .map(|_| {
                scope.spawn(|| {
                    super::super::super::spawn(
                        &host,
                        &request("sleep 0.3; printf '%s' \"$SSH_CONNECTION\""),
                    )
                    .map_err(|e| e.to_string())?
                    .wait_with_output()
                    .map_err(|e| e.to_string())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| "overflow worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    assert!(
        overflow
            .iter()
            .all(|o| o.status.success() && !o.stdout.is_empty()),
        "mux overflow must fall back cleanly"
    );
    assert!(
        overflow
            .iter()
            .any(|o| String::from_utf8_lossy(&o.stdout).trim() != first),
        "test must actually exceed MaxSessions"
    );
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, first);
    // Killing one streaming client closes only its channel, not the authenticated master.
    let mut stream = super::super::super::spawn(&host, &request("sleep 30"))?;
    stream.kill()?;
    stream.wait()?;
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, first);
    let config = config(&dest).ok_or("no managed configuration")?;
    let effective = effective(&config.path, &dest)?;
    let socket = effective
        .lines()
        .find_map(|line| line.strip_prefix("controlpath "))
        .ok_or("no controlpath")?;
    assert!(
        socket.starts_with("/tmp/gc-"),
        "test config must allow managed multiplexing"
    );
    let status = Command::new("ssh")
        .args(["-S", socket, "-O", "exit", "--", &dest])
        .status()?;
    assert!(status.success());
    for _ in 0..100 {
        if !Path::new(socket).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Leave a stale socket, as after a crashed master. OpenSSH auto removes/replaces it.
    let stale = std::os::unix::net::UnixListener::bind(socket)?;
    drop(stale);
    let reconnected = connection("printf '%s' \"$SSH_CONNECTION\"")?;
    assert_ne!(reconnected, first);
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, reconnected);
    super::super::super::pool::shutdown();
    Ok(())
}
