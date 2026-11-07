"""Pure, synthetic cancellation model. This module has no process or I/O capability.

The numbers below are fixture labels, NOT host PIDs or authority to signal them.
A future launcher must independently prove/retain an owned session/job capability.
These plans must never be connected to a host process-table-to-signal pipeline.
"""
from dataclasses import dataclass


class NotOwned(ValueError):
    """Ownership or registry data is insufficient/ambiguous; no plan is produced."""


@dataclass(frozen=True)
class ProcessRecord:
    handle: str
    parent_handle: str | None
    pid_label: int
    session_label: int
    group_label: int
    owner: str
    alive: bool = True


@dataclass(frozen=True)
class LaunchReceipt:
    owner: str
    root_handle: str
    launcher_handle: str
    session_label: int
    group_label: int
    created_new_session: bool


@dataclass(frozen=True)
class StopPlan:
    owner: str
    root_handle: str
    # Only opaque registered handles leave the selector. No PID or negative-PGID list.
    handles: tuple[str, ...]


def synthetic_stop_plan(
    receipt: LaunchReceipt, records: tuple[ProcessRecord, ...]
) -> StopPlan:
    """Plan only known owned descendants of a live, freshly created session root.

    parent_handle is immutable launch ancestry, not a live host PPID lookup. A missing,
    foreign, or out-of-containment link cannot establish authority. The receipt is a
    synthetic fixture premise, not a self-authenticating production capability.
    """
    if (
        type(receipt) is not LaunchReceipt
        or type(receipt.owner) is not str or not receipt.owner
        or type(receipt.root_handle) is not str or not receipt.root_handle
        or type(receipt.launcher_handle) is not str or not receipt.launcher_handle
        or type(receipt.session_label) is not int
        or type(receipt.group_label) is not int
        or receipt.root_handle == receipt.launcher_handle
        or receipt.created_new_session is not True
        or type(records) is not tuple
        or not records
        or len(records) > 4096
    ):
        raise NotOwned("An owned launcher/session receipt and bounded registry are required")
    by_handle: dict[str, ProcessRecord] = {}
    pid_labels: set[int] = set()
    for process in records:
        if (
            type(process) is not ProcessRecord
            or not isinstance(process.handle, str)
            or not process.handle
            or (process.parent_handle is not None and not isinstance(process.parent_handle, str))
            or type(process.pid_label) is not int
            or type(process.session_label) is not int
            or type(process.group_label) is not int
            or type(process.owner) is not str
            or type(process.alive) is not bool
            or process.handle in by_handle
            or process.pid_label in pid_labels
        ):
            raise NotOwned("Ambiguous process registry")
        by_handle[process.handle] = process
        pid_labels.add(process.pid_label)
    root = by_handle.get(receipt.root_handle)
    if (
        root is None
        or not root.alive
        or root.pid_label <= 1
        or root.owner != receipt.owner
        or root.parent_handle != receipt.launcher_handle
        or root.session_label != root.pid_label
        or root.group_label != root.pid_label
        or receipt.session_label != root.session_label
        or receipt.group_label != root.group_label
    ):
        raise NotOwned("The live root does not prove a distinct owned session")
    launcher = by_handle.get(receipt.launcher_handle)
    if launcher is not None and (
        launcher.session_label == root.session_label
        or launcher.group_label == root.group_label
    ):
        raise NotOwned("The launcher/ancestor must be outside the owned containment")
    eligible = {
        handle: process
        for handle, process in by_handle.items()
        if process.pid_label > 1
        and process.owner == receipt.owner
        and process.session_label == root.session_label
        and process.group_label == root.group_label
        and handle != receipt.launcher_handle
    }
    depths: dict[str, int] = {}
    for handle, process in eligible.items():
        if not process.alive:
            continue
        current: str | None = handle
        visited: set[str] = set()
        while current is not None and current != root.handle:
            if current in visited or current not in eligible:
                break
            visited.add(current)
            current = eligible[current].parent_handle
        if current == root.handle:
            depths[handle] = len(visited)
    if root.handle not in depths:
        raise NotOwned("No owned root target")
    handles = tuple(sorted(depths, key=lambda handle: (-depths[handle], handle)))
    return StopPlan(receipt.owner, receipt.root_handle, handles)


class RecordingSignalSink:
    """An inert recorder with no callback, executor, numeric-PID output, or OS adapter."""

    def __init__(self, plan: StopPlan):
        if type(plan) is not StopPlan or plan.root_handle not in plan.handles:
            raise NotOwned("An owned plan is required")
        self._allowed = frozenset(plan.handles)
        self.records: list[tuple[str, str]] = []

    def record(self, handle: str, action: str = "stop-owned-handle") -> None:
        if handle not in self._allowed or action != "stop-owned-handle":
            raise NotOwned("A foreign target or runtime signal was refused")
        self.records.append((handle, action))
