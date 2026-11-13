"""Owned process trees for the CI test watchdog (issue #211).

Every child starts in a container that only it and its descendants join: a new
session/process group on POSIX, a Job Object on Windows (created suspended and
assigned before it runs). Signals target only that owned container, never PIDs
found by host-wide searches. Host-wide process listings are read-only and are
filtered to the owned container before they are recorded.
"""

import json
import os
import signal
import subprocess
import sys

WINDOWS = sys.platform == "win32"
SNAPSHOT_TIMEOUT = 60


def parse_ps(text, pgid):
    """Rows of `ps -A -o pid=,ppid=,pgid=,etime=,stat=,command=` in one process group."""
    rows = []
    for line in text.splitlines():
        fields = line.split(None, 5)
        if len(fields) < 5 or not fields[0].isdigit():
            continue
        pid, ppid, group = (int(value) for value in fields[:3])
        if group != pgid:
            continue
        rows.append({"pid": pid, "ppid": ppid, "pgid": group, "elapsed": fields[3],
                     "state": fields[4], "command": fields[5] if len(fields) > 5 else ""})
    return rows


def tree_order(rows, root):
    """Depth-first order from the owned root, then any orphaned members of the container."""
    children = {}
    for row in rows:
        children.setdefault(row["ppid"], []).append(row)
    ordered, seen = [], set()

    def visit(pid, depth):
        for row in sorted(children.get(pid, []), key=lambda item: item["pid"]):
            if row["pid"] in seen:
                continue
            seen.add(row["pid"])
            ordered.append({**row, "depth": depth})
            visit(row["pid"], depth + 1)

    for row in rows:
        if row["pid"] == root:
            seen.add(root)
            ordered.append({**row, "depth": 0})
            visit(root, 1)
    for row in sorted(rows, key=lambda item: item["pid"]):
        if row["pid"] not in seen:
            seen.add(row["pid"])
            ordered.append({**row, "depth": 0, "orphaned": True})
            visit(row["pid"], 1)
    return ordered


def parse_cim(text, owned):
    """PowerShell `Win32_Process | ConvertTo-Json` rows restricted to job members."""
    data = json.loads(text) if text.strip() else []
    if isinstance(data, dict):
        data = [data]
    rows = []
    for item in data:
        pid = int(item.get("ProcessId", -1))
        if pid not in owned:
            continue
        rows.append({"pid": pid, "ppid": int(item.get("ParentProcessId") or 0),
                     "name": item.get("Name") or "", "command": item.get("CommandLine") or "",
                     "created": str(item.get("CreationDate") or ""),
                     "threads": item.get("ThreadCount"), "working_set": item.get("WorkingSetSize")})
    return rows


class _WindowsJob:
    """A kill-on-close Job Object; breakaway stays allowed so test semantics do not change."""

    def __init__(self):
        import ctypes
        from ctypes import wintypes

        self.ctypes = ctypes
        self.k32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.k32.CreateJobObjectW.restype = wintypes.HANDLE
        self.k32.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
        for name, args in (("SetInformationJobObject", [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]),
                           ("QueryInformationJobObject", [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
                                                          wintypes.DWORD, ctypes.c_void_p]),
                           ("AssignProcessToJobObject", [wintypes.HANDLE, wintypes.HANDLE]),
                           ("TerminateJobObject", [wintypes.HANDLE, wintypes.UINT]),
                           ("CloseHandle", [wintypes.HANDLE])):
            function = getattr(self.k32, name)
            function.argtypes = args
            function.restype = wintypes.BOOL
        self.handle = self.k32.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())

        class Basic(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                        ("LimitFlags", wintypes.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                        ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wintypes.DWORD),
                        ("Affinity", ctypes.c_size_t), ("PriorityClass", wintypes.DWORD),
                        ("SchedulingClass", wintypes.DWORD)]

        class Extended(ctypes.Structure):
            _fields_ = [("Basic", Basic), ("IoInfo", ctypes.c_uint64 * 6),
                        ("ProcessMemoryLimit", ctypes.c_size_t), ("JobMemoryLimit", ctypes.c_size_t),
                        ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]

        info = Extended()
        info.Basic.LimitFlags = 0x2000 | 0x800  # KILL_ON_JOB_CLOSE | BREAKAWAY_OK
        if not self.k32.SetInformationJobObject(self.handle, 9, ctypes.byref(info), ctypes.sizeof(info)):
            raise ctypes.WinError(ctypes.get_last_error())

    def assign(self, process_handle):
        if not self.k32.AssignProcessToJobObject(self.handle, int(process_handle)):
            raise self.ctypes.WinError(self.ctypes.get_last_error())

    def pids(self):
        ctypes = self.ctypes

        class PidList(ctypes.Structure):
            _fields_ = [("Assigned", ctypes.c_uint32), ("Listed", ctypes.c_uint32),
                        ("Ids", ctypes.c_size_t * 4096)]

        listing = PidList()
        if not self.k32.QueryInformationJobObject(self.handle, 3, ctypes.byref(listing),
                                                  ctypes.sizeof(listing), None):
            raise ctypes.WinError(ctypes.get_last_error())
        return [int(listing.Ids[index]) for index in range(listing.Listed)]

    def terminate(self):
        return bool(self.k32.TerminateJobObject(self.handle, 1))

    def close(self):
        if self.handle:
            self.k32.CloseHandle(self.handle)
            self.handle = None


def _resume(process_handle):
    import ctypes

    ntdll = ctypes.WinDLL("ntdll")
    ntdll.NtResumeProcess.argtypes = [ctypes.c_void_p]
    status = ntdll.NtResumeProcess(ctypes.c_void_p(int(process_handle)))
    if status:
        raise OSError(f"NtResumeProcess failed: 0x{status & 0xFFFFFFFF:08x}")


class OwnedProcess:
    """A child whose whole descendant tree can be listed and stopped without touching others."""

    def __init__(self, command, cwd=None, env=None):
        self.warnings = []
        self.job = None
        common = dict(cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                      stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if WINDOWS:
            try:
                self.job = _WindowsJob()
            except OSError as error:
                self.warnings.append(f"no Job Object, only the direct child can be stopped: {error}")
            flags = 0x4 if self.job else 0  # CREATE_SUSPENDED until the child is in the job
            try:
                self.popen = subprocess.Popen(command, creationflags=flags, **common)
            except OSError:
                self.close_job()
                raise
            if self.job:
                try:
                    self.job.assign(self.popen._handle)
                except OSError as error:
                    self.warnings.append(f"Job Object assignment failed: {error}")
                    self.close_job()
                try:
                    _resume(self.popen._handle)
                except OSError:
                    self.popen.kill()  # never leave a suspended child behind
                    self.popen.wait()
                    self.close_job()
                    raise
        else:
            self.popen = subprocess.Popen(command, start_new_session=True, **common)
        self.pid = self.popen.pid

    @property
    def stdout(self):
        return self.popen.stdout

    def poll(self):
        return self.popen.poll()

    def wait(self, timeout=None):
        return self.popen.wait(timeout)

    def snapshot(self):
        """Read-only listing of the owned tree, for diagnostics."""
        try:
            if WINDOWS:
                return self._windows_snapshot()
            listing = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,pgid=,etime=,stat=,command="],
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                     errors="replace", timeout=SNAPSHOT_TIMEOUT)
            return {"method": "ps process group", "processes": tree_order(parse_ps(listing.stdout, self.pid), self.pid)}
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            return {"method": "unavailable", "error": str(error), "processes": []}

    def _windows_snapshot(self):
        owned = set(self.job.pids()) if self.job else ({self.pid} if self.poll() is None else set())
        if not owned:
            return {"method": "job object", "processes": []}
        ids = ",".join(str(pid) for pid in sorted(owned))
        script = ("$ids = @(" + ids + "); Get-CimInstance Win32_Process | Where-Object { $ids -contains $_.ProcessId } | "
                  "Select-Object ProcessId,ParentProcessId,Name,CommandLine,CreationDate,ThreadCount,WorkingSetSize | "
                  "ConvertTo-Json -Depth 2 -Compress")
        listing = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                 errors="replace", timeout=SNAPSHOT_TIMEOUT)
        rows = parse_cim(listing.stdout, owned)
        missing = sorted(owned - {row["pid"] for row in rows})
        return {"method": "job object" if self.job else "direct child", "job_pids": sorted(owned),
                "unlisted_pids": missing, "processes": tree_order(rows, self.pid)}

    def kill_tree(self):
        """Stop the owned tree; returns a description of what was signalled."""
        if WINDOWS:
            if self.job and self.job.terminate():
                return "terminated Job Object"
            self.popen.kill()
            return "killed direct child only"
        try:
            os.killpg(self.pid, signal.SIGKILL)
            return f"SIGKILL to owned process group {self.pid}"
        except ProcessLookupError:
            return "process group already gone"

    def close_job(self):
        if self.job:
            self.job.close()
            self.job = None

    def close(self):
        if self.popen.stdout:
            self.popen.stdout.close()
        self.close_job()
