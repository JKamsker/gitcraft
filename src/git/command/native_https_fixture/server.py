"""Test-only, finite dumb-Git HTTPS endpoint; no external sockets or subprocesses.

Started exclusively by native_https_fixture.rs through the owned process Service.
Certificates are generated at fixture runtime, never downloaded or checked in.
"""
import json
import pathlib
import select
import socket
import ssl
import sys
import time

root = pathlib.Path(sys.argv[1])
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.minimum_version = ssl.TLSVersion.TLSv1_2
context.load_cert_chain(root / "server.pem", root / "server.key")
deadline = time.monotonic() + 100
listeners = []
requests = 0
try:
    for _ in range(2):
        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        listeners.append(listener)
        listener.bind(("127.0.0.1", 0))
        listener.listen(4)
        listener.setblocking(False)
    ports = [s.getsockname()[1] for s in listeners]
    print(json.dumps(ports), flush=True)
    with (root / "requests.jsonl").open("x", encoding="utf-8") as log:
        while time.monotonic() < deadline and requests < 64:
            if (root / "stop").exists():
                break
            ready, _, _ = select.select(listeners, [], [], 0.1)
            for listener in ready:
                raw, peer = listener.accept()
                with raw:
                    raw.settimeout(1)
                    if peer[0] != "127.0.0.1":
                        raise RuntimeError("non-loopback peer")
                    with context.wrap_socket(raw, server_side=True) as conn:
                        # One absolute read deadline, including slow partial headers.
                        read_deadline = min(deadline, time.monotonic() + 2)
                        data = b""
                        while b"\r\n\r\n" not in data:
                            remaining = read_deadline - time.monotonic()
                            if remaining <= 0 or len(data) >= 8192:
                                raise RuntimeError("bounded header read exhausted")
                            conn.settimeout(remaining)
                            chunk = conn.recv(min(1024, 8192 - len(data)))
                            if not chunk:
                                raise RuntimeError("incomplete request")
                            data += chunk
                        lines = data.decode("ascii").split("\r\n")
                        method, path, _ = lines[0].split(" ")
                        headers = {}
                        for line in lines[1:]:
                            if not line:
                                break
                            key, value = line.split(":", 1)
                            key = key.lower()
                            if key in headers:
                                raise RuntimeError("duplicate request header")
                            headers[key] = value.strip()
                        requests += 1
                        log.write(json.dumps({"port": listener.getsockname()[1],
                                              "path": path,
                                              "authorization": headers.get("authorization", "")}) + "\n")
                        log.flush()
                        if method != "GET":
                            raise RuntimeError("fixture permits GET only")
                        extra = ""
                        if path.startswith("/redirect-path.git/"):
                            status, body = "302 Found", b""
                            extra = "Location: /sink-path.git/info/refs\r\n"
                        elif path.startswith("/redirect-origin.git/"):
                            status, body = "302 Found", b""
                            extra = f"Location: https://127.0.0.1:{ports[1]}/sink-origin.git/info/refs\r\n"
                        elif path == "/repo.git/info/refs?service=git-upload-pack":
                            status = "200 OK"
                            body = b"1111111111111111111111111111111111111111\trefs/heads/main\n"
                        elif path == "/repo.git/HEAD":
                            status, body = "200 OK", b"ref: refs/heads/main\n"
                        else:
                            status, body = "404 Not Found", b""
                        response = (f"HTTP/1.1 {status}\r\nContent-Type: text/plain\r\n"
                                    f"Content-Length: {len(body)}\r\nConnection: close\r\n{extra}\r\n").encode("ascii") + body
                        conn.settimeout(1)
                        conn.sendall(response)
        if not (root / "stop").exists():
            raise RuntimeError("finite server budget exhausted")
    print("CLOSED", flush=True)
finally:
    for listener in listeners:
        listener.close()
