# Fixed remote transaction program; stdin is data, never executable repository input.
# Python 3 + its standard library supply no-follow dir-fd I/O and native ACL snapshots.
import base64, ctypes, errno, hashlib, json, os, stat, subprocess, sys, tempfile, time
LIMIT = 16 * 1024 * 1024
DEADLINE = time.monotonic() + 40
payload = json.load(sys.stdin)
def decode(key): return base64.b64decode(payload[key], validate=True)
def git(args, index=None, absent=False):
    env = os.environ.copy()
    if index is not None: env['GIT_INDEX_FILE'] = index
    out = subprocess.run(['git', '--no-lazy-fetch', '--literal-pathspecs'] + args,
                         env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         timeout=max(0.1, DEADLINE-time.monotonic()))
    if out.returncode != 0 and not (absent and out.returncode == 1):
        raise RuntimeError('Git index transaction command failed')
    if len(out.stdout) > LIMIT or out.stderr:
        raise RuntimeError('Git transaction output is unsupported or exceeds limit')
    return out.stdout

def config(name): return git(['config','--get',name], absent=True)
def native_acl(fd):
    if sys.platform != 'darwin': return None
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
    lib.acl_get_fd_np.argtypes = [ctypes.c_int, ctypes.c_int]; lib.acl_get_fd_np.restype = ctypes.c_void_p
    lib.acl_to_text.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ssize_t)]; lib.acl_to_text.restype = ctypes.c_void_p
    lib.acl_free.argtypes = [ctypes.c_void_p]
    acl = lib.acl_get_fd_np(fd, 0x100)
    if not acl:
        if ctypes.get_errno() == errno.ENOENT: return b''  # established absence, not unreadable
        raise RuntimeError('Cannot establish native index ACL')
    text = None
    try:
        size = ctypes.c_ssize_t()
        text = lib.acl_to_text(acl, ctypes.byref(size))
        if not text or size.value < 0 or size.value > 1024*1024: raise RuntimeError('Native index ACL exceeds limits')
        return ctypes.string_at(text, size.value)
    finally:
        if text: lib.acl_free(text)
        lib.acl_free(acl)
def set_acl(fd, text):
    if text is None: return
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
    lib.acl_from_text.argtypes=[ctypes.c_char_p]; lib.acl_from_text.restype=ctypes.c_void_p
    lib.acl_set_fd_np.argtypes=[ctypes.c_int,ctypes.c_void_p,ctypes.c_int]; lib.acl_free.argtypes=[ctypes.c_void_p]
    lib.acl_init.argtypes=[ctypes.c_int]; lib.acl_init.restype=ctypes.c_void_p
    acl = lib.acl_from_text(text) if text else lib.acl_init(0)
    if not acl: raise RuntimeError('Cannot prepare native index ACL')
    try:
        if lib.acl_set_fd_np(fd,acl,0x100) != 0: raise RuntimeError('Cannot preserve native index ACL')
    finally: lib.acl_free(acl)
def xattrs(fd):
    if sys.platform != 'darwin':
        return [(os.fsencode(name),os.getxattr(fd,name)) for name in sorted(os.listxattr(fd))]
    # CPython on macOS does not expose os.listxattr/getxattr. Use the shipped
    # descriptor-based APIs directly, with size queries before every allocation.
    lib=ctypes.CDLL('/usr/lib/libSystem.B.dylib',use_errno=True)
    lib.flistxattr.argtypes=[ctypes.c_int,ctypes.c_void_p,ctypes.c_size_t,ctypes.c_int];lib.flistxattr.restype=ctypes.c_ssize_t
    lib.fgetxattr.argtypes=[ctypes.c_int,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_size_t,ctypes.c_uint,ctypes.c_int];lib.fgetxattr.restype=ctypes.c_ssize_t
    size=lib.flistxattr(fd,None,0,0)
    if size<0 or size>1024*1024: raise RuntimeError('Cannot bound native index attributes')
    names=ctypes.create_string_buffer(max(1,size));count=lib.flistxattr(fd,names,size,0)
    if count<0 or count>size: raise RuntimeError('Index attributes changed during listing')
    result=[];total=0
    for name in sorted(n for n in names.raw[:count].split(b'\0') if n):
        size=lib.fgetxattr(fd,name,None,0,0,0);total+=len(name)+max(0,size)
        if size<0 or total>1024*1024: raise RuntimeError('Native index attributes exceed 1 MiB')
        value=ctypes.create_string_buffer(max(1,size));count=lib.fgetxattr(fd,name,value,size,0,0)
        if count<0 or count>size: raise RuntimeError('Index attributes changed during reading')
        result.append((name,value.raw[:count]))
    return result

def set_xattr(fd,name,value):
    if sys.platform != 'darwin': return os.setxattr(fd,name,value)
    lib=ctypes.CDLL('/usr/lib/libSystem.B.dylib',use_errno=True)
    lib.fsetxattr.argtypes=[ctypes.c_int,ctypes.c_char_p,ctypes.c_void_p,ctypes.c_size_t,ctypes.c_uint,ctypes.c_int]
    data=ctypes.create_string_buffer(value)
    if lib.fsetxattr(fd,name,data,len(value),0,0)!=0: raise RuntimeError('Cannot preserve native index attributes')

def access(fd):
    if not sys.platform.startswith(('linux','darwin')):
        raise RuntimeError('Full remote index policy requires Linux/WSL or macOS')
    values=xattrs(fd)
    if len(values)>1024 or sum(len(name)+len(value) for name,value in values)>1024*1024: raise RuntimeError('Index access metadata exceeds 1 MiB')
    return (values,native_acl(fd))
def readfd(fd, limit):
    result = bytearray()
    while len(result) <= limit:
        chunk = os.read(fd,min(65536,limit+1-len(result)))
        if not chunk: return bytes(result)
        result.extend(chunk)
    raise RuntimeError('File exceeds transaction size limit')
def snapshot(directory, name):
    try: fd = os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=directory)
    except FileNotFoundError: return None
    try:
        s = os.fstat(fd)
        if not stat.S_ISREG(s.st_mode) or s.st_size > LIMIT: raise RuntimeError('Index is not a bounded no-follow regular file')
        before = access(fd); data = readfd(fd,LIMIT); after = access(fd); end = os.fstat(fd)
        key = lambda s:(s.st_dev,s.st_ino,s.st_mode,s.st_uid,s.st_gid)
        if before != after or key(s) != key(end): raise RuntimeError('Index metadata changed during read')
        return (key(s),data,before)
    finally: os.close(fd)
def restore(parent, recovery, name):
    try: os.link('previous',name,src_dir_fd=recovery,dst_dir_fd=parent,follow_symlinks=False)
    except OSError: pass  # create-only: never overwrite a concurrent writer

def working(repo, path):
    parts = os.fsencode(path).split(b'/'); fd = os.open(repo,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:
        for part in parts[:-1]:
            next_fd = os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
            os.close(fd); fd = next_fd
        try: s = os.stat(parts[-1],dir_fd=fd,follow_symlinks=False)
        except FileNotFoundError: return b'missing\0'
        if stat.S_ISLNK(s.st_mode): return b'120000\0'+os.readlink(parts[-1],dir_fd=fd)
        if not stat.S_ISREG(s.st_mode): raise RuntimeError('Working source is unsupported')
        f = os.open(parts[-1],os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=fd)
        try: return (b'100755\0' if s.st_mode & 0o111 else b'100644\0')+readfd(f,2*1024*1024)
        finally: os.close(f)
    finally: os.close(fd)

def main():
    repo = os.getcwd()
    index = os.fsdecode(git(['rev-parse','--path-format=absolute','--git-path','index']).rstrip(b'\n'))
    parent_path = os.path.dirname(index); name = os.path.basename(index); lock_name = name+'.lock'
    parent = os.open(parent_path,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    directory = tempfile.mkdtemp(prefix='gitcraft-index-recovery-',dir=parent_path)
    recovery = os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    held = None; owned = None; success = False
    def parent_current():
        try:
            s = os.stat(parent_path,follow_symlinks=False); p = os.fstat(parent)
            return stat.S_ISDIR(s.st_mode) and (s.st_dev,s.st_ino)==(p.st_dev,p.st_ino)
        except OSError: return False
    def lock_owned():
        try:
            s = os.stat(lock_name,dir_fd=parent,follow_symlinks=False)
            return parent_current() and stat.S_ISREG(s.st_mode) and (s.st_dev,s.st_ino)==owned
        except OSError: return False
    def release():
        if held is None: return
        if not lock_owned(): raise RuntimeError('Foreign index lock preserved')
        os.rename(lock_name,'released-lock',src_dir_fd=parent,dst_dir_fd=recovery)
        s = os.stat('released-lock',dir_fd=recovery,follow_symlinks=False)
        if not stat.S_ISREG(s.st_mode) or (s.st_dev,s.st_ino)!=owned:
            try: os.link('released-lock',lock_name,src_dir_fd=recovery,dst_dir_fd=parent,follow_symlinks=False)
            except OSError: pass
            raise RuntimeError('Late foreign lock retained in recovery')
    try:
        held = os.open(lock_name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=parent)
        h = os.fstat(held); owned=(h.st_dev,h.st_ino)
        original = snapshot(parent,name); sharing = config('core.sharedRepository')
        def validate():
            if not lock_owned() or config('core.sharedRepository') != sharing: raise RuntimeError('Index ownership/policy changed')
            if git(['rev-parse','--verify','-q','HEAD'],absent=True).strip() != payload['head'].encode(): raise RuntimeError('HEAD changed')
            current = snapshot(parent,name)
            fingerprint = 'missing' if current is None else hashlib.md5(current[1]).hexdigest()
            if fingerprint != payload['index_stamp'] or current != original: raise RuntimeError('Index identity/bytes/policy changed')
            if git(['ls-files','--stage','-z']) != decode('index'): raise RuntimeError('Index entries changed')
            attributes = git(['check-attr','-z','filter','working-tree-encoding','eol','text','--',payload['path']])
            attributes += b'autocrlf\0'+config('core.autocrlf')
            if hashlib.md5(attributes).hexdigest() != payload['attributes_stamp']: raise RuntimeError('Attributes changed')
            for key in ['filemode','symlinks']:
                actual=git(['config','--bool','--get','core.'+key],absent=True).strip().decode()
                if actual != payload[key]: raise RuntimeError('Git file policy changed')
            if working(repo,payload['path']) != decode('work'): raise RuntimeError('Working source changed')
        validate()
        candidate = os.path.join(directory,'candidate')
        if original is not None:
            fd = os.open('candidate',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=recovery)
            with os.fdopen(fd,'wb') as f: f.write(original[1]); f.flush(); os.fsync(f.fileno())
        else: git(['read-tree','--empty'],candidate)
        if payload['intent']: git(['update-index','--force-remove','--',payload['path']],candidate)
        fd = os.open('patch',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=recovery)
        with os.fdopen(fd,'wb') as f: f.write(decode('patch'))
        git(['apply','--cached','--whitespace=nowarn']+(['--reverse'] if payload['reverse'] else [])+['--',os.path.join(directory,'patch')],candidate)
        fd = os.open('candidate',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=recovery)
        try:
            mode = os.fstat(fd).st_mode & 0o777
            if original is not None:
                key,_,policy = original
                now=os.fstat(fd)
                if (now.st_uid,now.st_gid)!=(key[3],key[4]): os.fchown(fd,key[3],key[4])
                for attr,value in policy[0]: set_xattr(fd,attr,value)
                set_acl(fd,policy[1]); os.fchmod(fd,mode)
                if access(fd) != policy: raise RuntimeError('Configured mode conflicts with full captured access policy')
            os.fsync(fd)
        finally: os.close(fd)
        prepared=snapshot(recovery,'candidate'); validate()
        if original is not None:
            os.rename(name,'previous',src_dir_fd=parent,dst_dir_fd=recovery)
            if snapshot(recovery,'previous') != original or not parent_current(): raise RuntimeError('Index changed during preservation')
        if not lock_owned() or config('core.sharedRepository')!=sharing or snapshot(recovery,'previous')!=original: raise RuntimeError('Policy changed before installation')
        os.link('candidate',name,src_dir_fd=recovery,dst_dir_fd=parent,follow_symlinks=False)
        if snapshot(parent,name)!=prepared or snapshot(recovery,'previous')!=original or not lock_owned() or config('core.sharedRepository')!=sharing: raise RuntimeError('Index changed after installation')
        release(); held_to_close=held; held=None; os.close(held_to_close)
        if snapshot(parent,name)!=prepared or config('core.sharedRepository')!=sharing: raise RuntimeError('Index changed at unlock')
        success=True
        print('Staging updated. Previous index and transaction evidence: '+directory)
    except Exception as e:
        raise RuntimeError(str(e)+'. Recovery: '+directory) from None
    finally:
        if not success: restore(parent,recovery,name)
        if held is not None:
            try: release()
            except Exception: pass
            os.close(held)
        os.close(recovery); os.close(parent)
try: main()
except Exception as e:
    print('Remote index transaction refused: '+str(e),file=sys.stderr); sys.exit(1)
