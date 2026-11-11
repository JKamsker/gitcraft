#!/usr/bin/env python3
"""Sample the opt-in production job/reload workload (Unix, no GUI required).

Builds and saves the current test executable; --binary reuses a saved executable.
"""
import argparse
import json
import hashlib
import platform
import shutil
import os
from pathlib import Path
import subprocess
import sys
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path)
parser.add_argument('--label', required=True)
parser.add_argument('--modes', nargs='+', choices=['commands', 'local-tabs', 'remote-tabs'],
                    default=['commands', 'local-tabs', 'remote-tabs'])
parser.add_argument('--counts', nargs='+', type=int, default=[1, 10, 50, 100])
parser.add_argument('--backend', choices=['auto', 'cli', 'native'], default='auto',
                    help='Select the Git read backend; cli supplies the reference baseline')
parser.add_argument('--repo', type=Path,
                    help='Clone this real repository for each tab instead of the one-commit fixture')
parser.add_argument('--repeats', type=int, default=1,
                    help='Fresh workload processes per mode/count (default: 1)')
parser.add_argument('--rounds', type=int, default=1,
                    help='Reload rounds in each process: initial then warm (default: 1)')
parser.add_argument('--ssh-host', help='Real SSH alias for remote-tabs; defaults to test loopback')
parser.add_argument('--ssh-log', type=Path,
                    help='Local disposable sshd log to count authenticated TCP connections')
parser.add_argument('--sanitize-path', action='store_true',
                    help='Drop relative PATH entries in both reference/native fixture processes')
args = parser.parse_args()
if sys.platform == 'win32':
    parser.error('sampling uses Unix ps; the workload itself also runs in Windows tests')
if any(count <= 0 for count in args.counts):
    parser.error('counts must be positive')
if args.repeats <= 0 or args.rounds <= 0:
    parser.error('repeats and rounds must be positive')
if args.repo is not None and not args.repo.is_dir():
    parser.error('--repo must name an existing repository directory')
if args.ssh_log is not None and not args.ssh_log.is_file():
    parser.error('--ssh-log must name the existing disposable sshd log')
results = []
outdir = Path('target/process-bench')
outdir.mkdir(parents=True, exist_ok=True)
if args.binary is None:
    build = subprocess.run(['cargo', 'test', '--locked', '--lib', '--no-run', '--message-format=json'],
                           check=True, stdout=subprocess.PIPE, text=True)
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binary = next(r['executable'] for r in artifacts if r.get('reason') == 'compiler-artifact'
                  and r.get('executable') and r['target']['name'] == 'gitcraft' and r['target']['kind'] == ['lib'])
    args.binary = outdir / f'{args.label}-tests'
    shutil.copy2(binary, args.binary)
digest = hashlib.sha256()
with args.binary.open('rb') as binary:
    for chunk in iter(lambda: binary.read(1024 * 1024), b''):
        digest.update(chunk)
metadata = dict(platform=platform.platform(), cpu_count=os.cpu_count(),
                binary_sha256=digest.hexdigest(),
                sample_interval_ms=10, counts=args.counts, modes=args.modes,
                backend=args.backend, repeats=args.repeats, rounds=args.rounds,
                ssh_host=args.ssh_host,
                path_sanitized=args.sanitize_path,
                repository=str(args.repo.resolve()) if args.repo else None)
if args.repo:
    metadata['repository_revision'] = subprocess.run(
        ['git', '-C', str(args.repo), 'rev-parse', 'HEAD'], check=True,
        capture_output=True, text=True).stdout.strip()
(outdir / f'{args.label}-metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
for mode in args.modes:
    for count, repeat in ((count, repeat) for count in args.counts
                          for repeat in range(args.repeats)):
        env = dict(os.environ, GITCRAFT_BENCH_COUNT=str(count), GITCRAFT_BENCH_MODE=mode,
                   GITCRAFT_GIT_BACKEND=args.backend, GITCRAFT_BENCH_ROUNDS=str(args.rounds))
        if args.sanitize_path:
            env['PATH'] = os.pathsep.join(path for path in env.get('PATH', '').split(os.pathsep)
                                         if Path(path).is_absolute())
        if args.repo:
            env['GITCRAFT_BENCH_REPO'] = str(args.repo.resolve())
        if args.ssh_host:
            env['GITCRAFT_BENCH_SSH_HOST'] = args.ssh_host
        if args.ssh_log:
            env['GITCRAFT_BENCH_SSH_LOG'] = str(args.ssh_log.resolve())
        log = outdir / f'{args.label}-{mode}-{count}-{repeat}.log'
        with log.open('w') as output:
            child = subprocess.Popen([str(args.binary.resolve()), 'repo::process_benchmarks::workload',
                                      '--exact', '--ignored', '--nocapture'], env=env, stdout=output, stderr=output)
            threads = rss = processes = 0
            while child.poll() is None:
                listing = subprocess.run(['ps', '-A', '-o', 'pid=,ppid=,rss=,comm='], capture_output=True, text=True)
                rows = [line.split(None, 3) for line in listing.stdout.splitlines()]
                descendants = {child.pid}
                while True:
                    added = {int(r[0]) for r in rows if int(r[1]) in descendants} - descendants
                    if not added:
                        break
                    descendants |= added
                # Exclude this sampler's ps process; it is not a child of the workload.
                processes = max(processes, len(descendants) - 1)
                rss = max(rss, max((int(r[2]) for r in rows if int(r[0]) == child.pid), default=0))
                if sys.platform == 'darwin':
                    listing = subprocess.run(['ps', '-M', '-p', str(child.pid)], capture_output=True, text=True)
                    threads = max(threads, max(0, len(listing.stdout.splitlines()) - 1))
                else:
                    status = Path(f'/proc/{child.pid}/status')
                    if status.exists():
                        for line in status.read_text().splitlines():
                            if line.startswith('Threads:'):
                                threads = max(threads, int(line.split()[1]))
                time.sleep(0.01)
        text = log.read_text()
        if child.returncode:
            raise SystemExit(text)
        payloads = [line.removeprefix('PROCESS_BENCH ') for line in text.splitlines()
                    if line.startswith('PROCESS_BENCH ')]
        if not payloads:
            raise SystemExit(f'Missing PROCESS_BENCH output in {log}:\n{text}')
        if len(payloads) != args.rounds:
            raise SystemExit(f'Expected {args.rounds} rounds, got {len(payloads)} in {log}; '
                             'rebuild the saved benchmark binary')
        for payload in payloads:
            record = json.loads(payload)
            record.update(peak_os_threads=threads, peak_rss_kib=rss, peak_descendants=processes,
                          backend=args.backend, repeat=repeat)
            results.append(record)
            print(json.dumps(record), flush=True)
(outdir / f'{args.label}.json').write_text(json.dumps(results, indent=2) + '\n')
