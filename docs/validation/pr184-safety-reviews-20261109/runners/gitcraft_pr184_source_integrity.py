"""Compare tracked checkout bytes with immutable Git blobs, independently of status flags."""
from pathlib import Path
import hashlib
import os
import stat
import subprocess


def inspect_source(repo, commit):
    repo = Path(repo)
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=repo)
    raw = git('ls-tree', '-r', '-z', '--full-tree', commit)
    algorithm = git('rev-parse', '--show-object-format').decode().strip()
    if algorithm not in ('sha1', 'sha256'):
        raise ValueError('Unsupported Git object format')
    errors = []
    observed = []
    for row in raw.split(b'\0'):
        if not row:
            continue
        metadata, name = row.split(b'\t', 1)
        mode, kind, expected = metadata.split()
        relative = Path(os.fsdecode(name))
        if kind != b'blob' or mode not in (b'100644', b'100755', b'120000'):
            errors.append('Unsupported tracked entry: ' + repr(name))
            continue
        try:
            if relative.is_absolute() or '..' in relative.parts:
                raise ValueError('unsafe tracked path')
            for parent in relative.parents:
                if parent == Path('.'):
                    continue
                parent_info = (repo / parent).lstat()
                if not stat.S_ISDIR(parent_info.st_mode):
                    raise ValueError('tracked parent is not an ordinary directory')
            path = repo / relative
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                actual_mode = b'120000'
                data = os.fsencode(os.readlink(path))
            elif stat.S_ISREG(info.st_mode):
                actual_mode = b'100755' if info.st_mode & 0o111 else b'100644'
                with path.open('rb') as stream:
                    opened = os.fstat(stream.fileno())
                    if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                        raise ValueError('entry changed while opening')
                    data = stream.read()
                    after = os.fstat(stream.fileno())
                    if (opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
                        raise ValueError('entry changed while reading')
                final = path.lstat()
                if (final.st_dev, final.st_ino, final.st_mode, final.st_mtime_ns, final.st_ctime_ns) != (info.st_dev, info.st_ino, info.st_mode, info.st_mtime_ns, info.st_ctime_ns):
                    raise ValueError('entry changed during observation')
            else:
                raise ValueError('entry is not a regular file or symlink')
            actual = hashlib.new(algorithm, b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest().encode()
            observed.append(actual_mode + b' blob ' + actual + b'\t' + name + b'\0')
            if actual_mode != mode or actual != expected:
                errors.append('Tracked bytes/mode differ: ' + repr(name))
        except (OSError, ValueError) as error:
            errors.append('Cannot establish tracked identity: ' + repr(name) + ': ' + str(error))
    flags = git('ls-files', '-v', '-z')
    for row in flags.split(b'\0'):
        if row and (row[:1].islower() or row[:1] == b'S'):
            errors.append('Hidden or sparse index flag unsupported: ' + repr(row))
    # A sparse index may contain tree entries instead of all tracked leaves.
    for row in git('ls-files', '--stage', '-z').split(b'\0'):
        if row:
            meta = row.split(b'\t', 1)[0].split()
            if meta[0] == b'040000' or meta[2] != b'0':
                errors.append('Sparse or unmerged index unsupported')
    return dict(commit=commit, expected_map_sha256=hashlib.sha256(raw).hexdigest(), observed_map_sha256=hashlib.sha256(b''.join(observed)).hexdigest(), entries=len(observed), errors=errors)
