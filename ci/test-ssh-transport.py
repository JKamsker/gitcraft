#!/usr/bin/env python3
"""Exercise managed transports against a disposable, real OpenSSH server (no root).

Linux prerequisites: openssh-client, openssh-server, and /run/sshd created by CI setup.
Uses RUSTUP_TOOLCHAIN/CARGO_TARGET_DIR normally. Never reads or changes ~/.ssh.
"""
import argparse
import getpass
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
TEST = "remote::transport::unix::tests::real_openssh_reuses_concurrently_and_recovers"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmark", action="store_true", help="also measure cold/warm real SSH reloads with and without multiplexing")
    parser.add_argument("--binary", type=Path, help="compiled test harness for --benchmark")
    args = parser.parse_args()
    sshd = shutil.which("sshd") or "/usr/sbin/sshd"
    if not Path(sshd).is_file():
        raise SystemExit("OpenSSH server missing: install openssh-server before running this test")
    with tempfile.TemporaryDirectory(prefix="gitcraft-ssh-test-") as tmp:
        work = Path(tmp)
        for key in ("host", "client"):
            run("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(work / key))
        (work / "authorized_keys").write_bytes((work / "client.pub").read_bytes())
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        config = work / "sshd_config"
        config.write_text(
            f"ListenAddress 127.0.0.1\nPort {port}\n"
            f'HostKey "{work / "host"}"\n'
            f'AuthorizedKeysFile "{work / "authorized_keys"}"\n'
            f'PidFile "{work / "pid"}"\n'
            f"AllowUsers {getpass.getuser()}\n"
            "PasswordAuthentication no\nKbdInteractiveAuthentication no\n"
            "PubkeyAuthentication yes\nUsePAM no\nStrictModes no\n"
            "MaxSessions 10\nMaxStartups 100\nLogLevel VERBOSE\n"
        )
        public = (work / "host.pub").read_text().split()
        (work / "known_hosts").write_text(f"[127.0.0.1]:{port} {public[0]} {public[1]}\n")
        client_config = work / "config"
        client_config.write_text(
            f"Host gitcraft-transport-test\n HostName 127.0.0.1\n Port {port}\n"
            f" User {getpass.getuser()}\n"
            f' IdentityFile "{work / "client"}"\n'
            f' UserKnownHostsFile "{work / "known_hosts"}"\n'
            " IdentitiesOnly yes\n StrictHostKeyChecking yes\n"
        )
        log_path = work / "server.log"
        with log_path.open("wb") as log:
            server = subprocess.Popen([sshd, "-D", "-e", "-f", str(config)], stdout=log, stderr=log)
            try:
                for _ in range(100):
                    if server.poll() is not None:
                        raise RuntimeError("Disposable sshd exited before listening")
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                            break
                    except OSError:
                        time.sleep(0.05)
                else:
                    raise RuntimeError("Disposable sshd did not start within five seconds")
                env = os.environ.copy()
                env["GITCRAFT_SSH_CONFIG"] = str(client_config)
                env["GITCRAFT_SSH_TEST_HOST"] = "gitcraft-transport-test"
                run("cargo", "test", "--locked", "--lib", TEST, "--", "--ignored", "--exact", "--nocapture", cwd=ROOT, env=env, timeout=900)
                if args.benchmark:
                    if not args.binary:
                        parser.error("--benchmark requires --binary (a freshly compiled test harness)")
                    no_mux = work / "config-no-mux"
                    no_mux.write_text(client_config.read_text() + "Host *\n ControlMaster no\n ControlPath none\n")
                    for label, source in (("ssh-no-mux", no_mux), ("ssh-managed", client_config)):
                        env["GITCRAFT_SSH_CONFIG"] = str(source)
                        run(sys.executable, str(ROOT / "ci/bench-processes.py"),
                            "--binary", str(args.binary.resolve()), "--label", label,
                            "--backend", "cli", "--modes", "remote-tabs",
                            "--counts", "1", "10", "50", "--repeats", "1", "--rounds", "2",
                            "--ssh-host", "gitcraft-transport-test", "--ssh-log", str(log_path),
                            "--repo", str(ROOT), cwd=ROOT, env=env, timeout=900)
            except BaseException:
                print(log_path.read_text(errors="replace"), file=sys.stderr)
                raise
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
        accepted = sum("Accepted publickey" in line for line in log_path.read_text(errors="replace").splitlines())
        print(f"Disposable sshd accepted {accepted} authenticated TCP connections (includes explicit reconnect).")


if __name__ == "__main__":
    main()
