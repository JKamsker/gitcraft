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
parser.add_argument('--modes', nargs='+', default=['commands', 'local-tabs', 'remote-tabs'])
parser.add_argument('--counts', nargs='+', type=int, default=[1, 10, 50, 100])
args = parser.parse_args()
if sys.platform == 'win32':
    parser.error('sampling uses Unix ps; the workload itself also runs in Windows tests')
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
metadata = dict(platform=platform.platform(), cpu_count=os.cpu_count(),
                binary_sha256=hashlib.file_digest(args.binary.open('rb'), 'sha256').hexdigest(),
                sample_interval_ms=10, counts=args.counts, modes=args.modes)
(outdir / f'{args.label}-metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
for mode in args.modes:
    for count in args.counts:
        env = dict(os.environ, GITCRAFT_BENCH_COUNT=str(count), GITCRAFT_BENCH_MODE=mode)
        log = outdir / f'{args.label}-{mode}-{count}.log'
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
        record = json.loads(next(line.removeprefix('PROCESS_BENCH ') for line in text.splitlines()
                                 if line.startswith('PROCESS_BENCH ')))
        record.update(peak_os_threads=threads, peak_rss_kib=rss, peak_descendants=processes)
        results.append(record)
        print(json.dumps(record), flush=True)
(outdir / f'{args.label}.json').write_text(json.dumps(results, indent=2) + '\n')
