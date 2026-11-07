# gitcraft owned execution protocol v1. Python 3 POSIX backend.
# No process discovery; group authority is a live, directly owned anchor child.
import base64, errno, json, os, re, selectors, signal, socket, sys, time

MAX = 64 << 20
ROOT = '/tmp/gitcraft-owned-'
HEX = re.compile(r'^[0-9a-f]{64}$')
NONCE = re.compile(r'^[0-9a-f]{1,96}$')

def endpoint(value):
    if not isinstance(value, dict) or set(value) != {'v', 'nonce', 'path', 'token'}:
        raise ValueError('invalid owner fields')
    if value['v'] != 1 or not NONCE.fullmatch(value['nonce']) or not HEX.fullmatch(value['token']):
        raise ValueError('invalid owner identity')
    if not re.fullmatch(ROOT + r'[0-9a-f]{32}', value['path']):
        raise ValueError('invalid owner path')
    return value

def encode(value):
    return json.dumps(value, separators=(',', ':')).encode() + b'\n'

def receipt(owner, state, code):
    return dict(owner, state=state, code=code)

def atomic(path, data, exclusive=False):
    name = path if exclusive else path + '.new'
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, 'wb') as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        if not exclusive:
            os.rename(name, path)
    except BaseException:
        try: os.unlink(name)
        except OSError: pass
        raise

def read_receipt(owner):
    # The endpoint is a capability, not a PID or an arbitrary script/path.
    fd = os.open(owner['path'] + '/terminal', os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as f:
        value = json.loads(f.read(2049))
    if set(value) != {'v', 'nonce', 'path', 'token', 'state', 'code'}:
        raise ValueError('invalid terminal receipt')
    if any(value[k] != owner[k] for k in owner) or value['state'] != 'stopped' or type(value['code']) is not int or not 0 <= value['code'] <= 255:
        raise ValueError('unconfirmed cleanup')
    return value

def control(owner):
    owner = endpoint(owner)
    try: return read_receipt(owner)
    except FileNotFoundError: pass
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(5)
    try:
        s.connect(owner['path'] + '/control')
        s.sendall(encode(dict(owner, op='stop')))
        value = bytearray()
        while not value.endswith(b'\n') and len(value) <= 2048:
            chunk = s.recv(2049 - len(value))
            if not chunk: break
            value.extend(chunk)
        # Never accept a socket-only response instead of the atomic terminal receipt.
        if json.loads(value) != read_receipt(owner): raise ValueError('incomplete acknowledgement')
        return read_receipt(owner)
    finally: s.close()

def finish_owned(anchor, established, ops=os, pause=time.sleep):
    failure = None
    try:
        if established:
            ops.killpg(anchor, signal.SIGTERM)
            pause(.15)
            ops.killpg(anchor, signal.SIGKILL)
        else:
            # User code is still behind the gate; no group has been acknowledged.
            ops.kill(anchor, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except BaseException as error:
        failure = error
        try: ops.kill(anchor, signal.SIGKILL)
        except ProcessLookupError: pass
    finally:
        ops.waitpid(anchor, 0)
    if failure: raise failure

def run(req, controller, stream=False):
    nonce = req['nonce']
    if not NONCE.fullmatch(nonce): raise ValueError('invalid request nonce')
    duration = req['milliseconds'] / 1000
    if not 0 < duration <= 7 * 86400: raise ValueError('invalid request deadline')
    owner = endpoint(dict(v=1, nonce=nonce, path=ROOT + os.urandom(16).hex(), token=os.urandom(32).hex()))
    os.mkdir(owner['path'], 0o700)
    sock = socket.socket(socket.AF_UNIX)
    sock.bind(owner['path'] + '/control')
    sock.listen(4)
    sock.setblocking(False)
    # All private descriptors are non-inheritable; anchor and target explicitly close
    # the controller, server and protocol fds before executing repository code.
    source = os.open(owner['path'] + '/input', os.O_CREAT | os.O_EXCL | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    data = base64.b64decode(req['input'], validate=True)
    with os.fdopen(source, 'w+b') as f:
        f.write(data)
        f.seek(0)
        out_r, out_w = os.pipe()
        err_r, err_w = os.pipe()
        status_r, status_w = os.pipe()
        gate_r, gate_w = os.pipe()
        anchor = os.fork()
        if anchor == 0:
            try:
                sock.close()
                controller.close()
                os.close(out_r); os.close(err_r); os.close(status_r); os.close(gate_w)
                os.setsid()
                signal.signal(signal.SIGTERM, signal.SIG_IGN)
                os.write(status_w, b'ready\n')
                if os.read(gate_r, 1) != b'G': os._exit(125)
                os.close(gate_r)
                child = os.fork()
                if child == 0:
                    try:
                        signal.signal(signal.SIGTERM, signal.SIG_DFL)
                        signal.signal(signal.SIGHUP, signal.SIG_DFL)
                        os.dup2(f.fileno(), 0); os.dup2(out_w, 1); os.dup2(err_w, 2)
                        os.close(status_w); os.close(out_w); os.close(err_w)
                        os.execv('/bin/sh', ['sh', '-c', req['script']])
                    except BaseException: os._exit(125)
                os.close(out_w); os.close(err_w)
                _, status = os.waitpid(child, 0)
                code = os.waitstatus_to_exitcode(status)
                os.write(status_w, (str(code if code >= 0 else 128 - code) + '\n').encode())
                os.close(status_w)
                # The anchor remains alive until the owner finishes every group signal.
                while True: signal.pause()
            except BaseException: os._exit(125)
        os.close(out_w); os.close(err_w); os.close(status_w); os.close(gate_r)
        established = False
        stopped = False
        code = 125
        clients = []
        expiries = {}
        selector = selectors.DefaultSelector()
        output = [bytearray(), bytearray()]
        deadline = time.monotonic() + duration
        # Ignore hangup in the supervisor; EOF/deadline uses the same owned stop/reap path.
        signal.signal(signal.SIGHUP, signal.SIG_IGN)
        try:
            # Anchor readiness is bounded and never grants authority to an arbitrary PID.
            ready_selector = selectors.DefaultSelector()
            ready_selector.register(status_r, selectors.EVENT_READ)
            if not ready_selector.select(5) or os.read(status_r, 6) != b'ready\n':
                raise RuntimeError('anchor did not establish ownership')
            ready_selector.close()
            established = True
            selector.register(controller, selectors.EVENT_READ, 'controller')
            selector.register(sock, selectors.EVENT_READ, 'socket')
            selector.register(status_r, selectors.EVENT_READ, 'status')
            selector.register(out_r, selectors.EVENT_READ, 0)
            selector.register(err_r, selectors.EVENT_READ, 1)
            if req.get('record'):
                atomic(req['record'], encode(owner), exclusive=True)
            hello = ('O1 {nonce} {path} {token}\n'.format(**owner)).encode()
            if stream:
                sys.stderr.buffer.write(b'__GITCRAFT_OWNER__ ' + hello)
                sys.stderr.buffer.flush()
            else:
                sys.stdout.buffer.write(hello); sys.stdout.buffer.flush()
            os.write(gate_w, b'G'); os.close(gate_w); gate_w = None
            status_bytes = bytearray()
            while not stopped:
                if time.monotonic() >= deadline: break
                for client in list(clients):
                    if expiries.get(client, 0) < time.monotonic():
                        try: selector.unregister(client)
                        except KeyError: pass
                        clients.remove(client); client.close()
                for key, _ in selector.select(min(.05, max(0, deadline - time.monotonic()))):
                    which = key.data
                    if which == 'controller':
                        # No user stdin shares this channel. EOF is a controller loss.
                        if not os.read(controller.fileno(), 4096): stopped = True
                    elif which == 'socket':
                        client, _ = sock.accept()
                        client.setblocking(False)
                        if len(clients) >= 4: client.close(); continue
                        clients.append(client)
                        expiries[client] = time.monotonic() + .25
                        selector.register(client, selectors.EVENT_READ, bytearray())
                    elif isinstance(which, bytearray):
                        try: chunk = key.fileobj.recv(2049 - len(which))
                        except BlockingIOError: continue
                        which.extend(chunk)
                        if not chunk or len(which) > 2048 or b'\n' in which:
                            selector.unregister(key.fileobj)
                            try:
                                message = json.loads(which)
                                if message != dict(owner, op='stop'): raise ValueError('unauthenticated')
                                stopped = True
                                expiries[key.fileobj] = time.monotonic() + 10
                            except (ValueError, TypeError):
                                clients.remove(key.fileobj); key.fileobj.close()
                    elif which == 'status':
                        chunk = os.read(status_r, 64)
                        status_bytes.extend(chunk)
                        if b'\n' in status_bytes:
                            code = int(status_bytes.strip()); stopped = True
                        elif not chunk: raise RuntimeError('anchor lost before status')
                    else:
                        chunk = os.read(key.fd, 8192)
                        if not chunk: selector.unregister(key.fd); continue
                        if len(output[which]) + len(chunk) > MAX: stopped = True
                        output[which].extend(chunk[:max(0, MAX - len(output[which]))])
                        if stream:
                            dest = sys.stdout.buffer if which == 0 else sys.stderr.buffer
                            dest.write(chunk); dest.flush()
        finally:
            # The owned anchor has never been waited on. Even premature anchor death
            # retains its identity as our unreaped direct child until these signals finish.
            finish_owned(anchor, established)
            if gate_w is not None: os.close(gate_w)
            # Freeze captured output after group termination, bounded even if a deliberate
            # session escape holds a pipe. Such escapes need stronger kernel containment.
            end = time.monotonic() + .3
            for fd, index in ((out_r, 0), (err_r, 1)):
                os.set_blocking(fd, False)
                while time.monotonic() < end:
                    try: chunk = os.read(fd, 8192)
                    except BlockingIOError: break
                    if not chunk: break
                    output[index].extend(chunk[:max(0, MAX - len(output[index]))])
                    if stream:
                        dest = sys.stdout.buffer if index == 0 else sys.stderr.buffer
                        try: dest.write(chunk); dest.flush()
                        except BrokenPipeError: pass
                os.close(fd)
            os.close(status_r)
            final = receipt(owner, 'stopped', code)
            atomic(owner['path'] + '/terminal', encode(final))
            for client in clients:
                try: client.settimeout(.1); client.sendall(encode(final))
                except OSError: pass
                client.close()
            selector.close(); sock.close()
        return code, output, owner

def serve():
    # Unbuffered reads prevent input for the next request from being stolen by buffering.
    controller = os.fdopen(os.dup(0), 'rb', buffering=0)
    print('__GITCRAFT_READY__', flush=True)
    while True:
        line = bytearray()
        while len(line) <= 96 << 20:
            chunk = os.read(controller.fileno(), 65536)
            if not chunk: return
            line.extend(chunk)
            if b'\n' in chunk: break
        if len(line) > 96 << 20 or line.count(b'\n') != 1 or not line.endswith(b'\n'):
            raise ValueError('oversized or pipelined request')
        line = bytes(line)
        if line.startswith(b'P '):
            sys.stdout.buffer.write(line); sys.stdout.buffer.flush(); continue
        req = json.loads(line)
        print('S ' + req['nonce'], flush=True)
        code, output, owner = run(req, controller)
        sys.stdout.buffer.write(b'C1 ' + encode(receipt(owner, 'stopped', code))); sys.stdout.buffer.flush()
        print('R {} {} {} {}'.format(req['nonce'], code, len(output[0]), len(output[1])), flush=True)
        sys.stdout.buffer.write(output[0]); sys.stdout.buffer.write(output[1]); sys.stdout.buffer.flush()

def main():
    mode = sys.argv[1]
    if mode == 'probe':
        if not all(hasattr(os, n) for n in ('fork', 'setsid', 'killpg')): raise RuntimeError('POSIX ownership unavailable')
    elif mode == 'serve': serve()
    elif mode == 'control':
        print(json.dumps(control(json.loads(sys.stdin.buffer.readline(2049)))), flush=True)
    elif mode == 'stream':
        print('\n__GITCRAFT_READY__', flush=True)
        fd = os.open(sys.argv[2], os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(fd, 'rb') as f: req = json.load(f)
        os.unlink(sys.argv[2])
        os.rmdir(os.path.dirname(sys.argv[2]))
        control_input = os.fdopen(os.dup(0), 'rb', buffering=0)
        code, _, _ = run(req, control_input, True)
        sys.exit(code)
    else: raise ValueError('unknown owner operation')

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] in ('serve', 'stream'):
        # Detach the supervisor from the local SSH/WSL/loopback transport group. It
        # survives controller/transport death, notices EOF, and publishes the receipt.
        child = os.fork()
        if child:
            _, status = os.waitpid(child, 0)
            code = os.waitstatus_to_exitcode(status)
            sys.exit(code if code >= 0 else 128 - code)
        os.setsid()
        signal.signal(signal.SIGHUP, signal.SIG_IGN)
    main()
