"""Bounded stack minidumps of the watchdog's direct, still-owned Windows child.

The caller keeps the Popen process handle alive (preventing PID reuse) and checks
it has not exited. Never call this with a PID obtained from a host-wide listing.
MiniDumpNormal includes thread stacks without requesting a full memory dump.
"""
import ctypes
from ctypes import wintypes
from pathlib import Path
import subprocess
import sys


def capture(child, destination):
    if sys.platform != "win32" or child.poll() is not None:
        return {"status": "unavailable"}
    try:
        result = subprocess.run([sys.executable, str(Path(__file__).resolve()),
                                 str(child.pid), str(destination)],
                                capture_output=True, text=True, timeout=10)
        return {"status": "captured" if result.returncode == 0 else "failed",
                "path": str(destination.name), "detail": result.stderr[-1000:]}
    except (OSError, subprocess.SubprocessError) as error:
        return {"status": "failed", "detail": str(error)}


def write_dump(pid, destination):
    import msvcrt
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    dbghelp = ctypes.WinDLL("Dbghelp", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    dump = dbghelp.MiniDumpWriteDump
    dump.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.HANDLE, wintypes.DWORD,
                     ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
    dump.restype = wintypes.BOOL
    process = kernel.OpenProcess(0x0400 | 0x0010, False, pid)  # query information + read memory
    if not process:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        with open(destination, "wb") as target:
            if not dump(process, pid, msvcrt.get_osfhandle(target.fileno()), 0, None, None, None):
                raise ctypes.WinError(ctypes.get_last_error())
    finally:
        kernel.CloseHandle(process)


if __name__ == "__main__":
    write_dump(int(sys.argv[1]), sys.argv[2])
