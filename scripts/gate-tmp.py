"""The gate's own temporary folder (v0.0.13 scope, C7).

Both gates (scripts/prepublish.sh, _prepublish.bat) point TMPDIR (TMP and TEMP
on Windows) at a folder of their own for the run, so the test stores the
harnesses leave behind stop piling up in the system's temporary folder.
Removing them must not hide that they are left (a cleanup that silences a
repetition silences the failure too), so the end of a run prints them first,
grouped by name.

    python -I scripts/gate-tmp.py start  <gate_tmp> <system_tmp>
        Removes the previous run's folder, creates a fresh one, and records
        what <system_tmp> holds now.
    python -I scripts/gate-tmp.py report <gate_tmp> <system_tmp> [--remove]
        Prints what the run left in <gate_tmp>, grouped by name, and what
        appeared in <system_tmp> since `start`. With --remove (a gate that
        passed) the folder goes; without it (a gate that failed) it stays for
        diagnosis until the next start.

The report is a measure: it never changes the gate's exit code.
"""

import collections
import os
import re
import shutil
import stat
import sys


def long_path(path):
    """Windows refuses paths past 260 characters unless they are extended."""
    path = os.path.abspath(path)
    if os.name == "nt" and not path.startswith("\\\\?\\"):
        return "\\\\?\\" + path
    return path


def entries(folder):
    try:
        return sorted(os.listdir(long_path(folder)))
    except FileNotFoundError:
        return []


def remove_tree(folder):
    def clear_read_only(func, path, _exc):
        os.chmod(path, stat.S_IWRITE)
        func(path)

    if os.path.exists(long_path(folder)):
        shutil.rmtree(long_path(folder), onerror=clear_read_only)


def snapshot_path(gate_tmp):
    return os.path.abspath(gate_tmp) + ".before"


def group(name):
    """A name up to its first digit: "docs-host-4711-...db" -> "docs-host-"."""
    return re.sub(r"[0-9].*$", "", name) or name


def start(gate_tmp, system_tmp):
    remove_tree(gate_tmp)
    os.makedirs(long_path(gate_tmp))
    with open(snapshot_path(gate_tmp), "w", encoding="utf-8") as f:
        f.write("\n".join(entries(system_tmp)))
    print(f"[gate-tmp] temporary folder for this run: {os.path.abspath(gate_tmp)}")


def report(gate_tmp, system_tmp, remove):
    left = entries(gate_tmp)
    print()
    print(f"=== temporary files and folders the run left in its own folder: {len(left)}")
    for name, count in collections.Counter(group(n) for n in left).most_common():
        print(f"{count:8}  {name}")

    try:
        with open(snapshot_path(gate_tmp), encoding="utf-8") as f:
            before = set(f.read().splitlines())
    except FileNotFoundError:
        before = None
    if before is None:
        print(f"=== new entries in {system_tmp}: not measured (no record from the start)")
    else:
        own = os.path.normcase(os.path.abspath(gate_tmp))
        new = [
            n for n in entries(system_tmp)
            if n not in before and os.path.normcase(os.path.abspath(os.path.join(system_tmp, n))) != own
        ]
        print(f"=== new entries in {system_tmp} since the gate started: {len(new)}"
              " (any process may write there; the names say whose)")
        for n in new[:20]:
            print(f"          {n}")
        if len(new) > 20:
            print(f"          ... and {len(new) - 20} more")

    if remove:
        remove_tree(gate_tmp)
        if os.path.exists(snapshot_path(gate_tmp)):
            os.remove(snapshot_path(gate_tmp))
        print(f"[gate-tmp] removed {os.path.abspath(gate_tmp)}")
    else:
        print(f"[gate-tmp] kept for diagnosis: {os.path.abspath(gate_tmp)} (removed at the next run's start)")


def main(argv):
    if len(argv) < 3 or argv[0] not in ("start", "report"):
        print(__doc__, file=sys.stderr)
        return 2
    command, gate_tmp, system_tmp = argv[0], argv[1], argv[2]
    if command == "start":
        start(gate_tmp, system_tmp)
        return 0
    try:
        report(gate_tmp, system_tmp, "--remove" in argv[3:])
    except OSError as e:
        print(f"[gate-tmp] the report could not be completed: {e}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
