#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S181 process and exec conformance.

Runs the process contract as a table: the host ``raven-spawn`` layout tests, the existing
on-target C qtests (argv/envp/auxv, wait/exit statuses, signals, spawn attributes), the
``process-lifecycle`` terminate/wait rows, and one release-image SSH row that runs an external
command and checks the exact exit status the SSH gate quotes. Missing release-image rows are
reported as BLOCKED with the exact build command, never silently skipped.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMAGE = Path(os.environ.get("RAVEN_S181_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
BUILD_IMAGE = [
    "cargo", "xtask", "image", "--arch", "x86_64", "--profile", "release",
    "--output", str(IMAGE),
]

HOST_ROWS = (
    ("start-layout-argv-auxv", ["cargo", "nextest", "run", "-p", "raven-spawn"], 120),
)

QTEST_ROWS = (
    (
        "argv-env-auxv",
        "libc-hello",
        ("^libc-hello: argc ok$", "^libc-hello: environ ok$", "^libc-hello: auxv ok$", "^libc-hello: done$"),
        240,
    ),
    (
        "exit-status-wait-echild",
        "libc-spawn",
        None,
        240,
    ),
    (
        "signal-handler",
        "libc-signals",
        None,
        240,
    ),
    (
        "spawn-attrs",
        "libc-spawn-attr",
        None,
        240,
    ),
    (
        "spawn-cmd",
        "libc-spawn-cmd",
        None,
        240,
    ),
    (
        "spawn-std",
        "libc-spawn-std",
        None,
        240,
    ),
    (
        "terminate-wait",
        "process-lifecycle",
        (
            "^s36: unstarted reaped$",
            "^s36: wait race ok$",
            "^s36: wait eintr ok$",
            "^s36: admission ok$",
            "^s36: terminate ok$",
        ),
        300,
    ),
)

SSH_ROWS = (
    ("exit-0", ["/bin/brush", "-c", "exit 0"], 0, None, 20),
    ("exit-1", ["/bin/brush", "-c", "exit 1"], 1, None, 20),
    ("exit-42", ["/bin/brush", "-c", "exit 42"], 42, None, 20),
    # Same external status the SSH gate quotes for the failing child.
    ("exit-134", ["/bin/brush", "-c", "exit 134"], 134, None, 20),
    ("sigkill-external", ["/bin/brush", "-c", "kill -9 $$"], 137, None, 20),
    (
        "argv-env-exact",
        ["/bin/brush", "-c", 'printf "%s|%s|%s\\n" "$0" "$1" "$2"', "zero", "one", "two"],
        0,
        b"zero|one|two\n",
        20,
    ),
    (
        "fd-inherit",
        ["/bin/brush", "-c", 'exec 3</etc/ssh-matrix-marker; /bin/brush -c "cat <&3"'],
        0,
        b"K434-SSH-MARKER-2026-11-22\n",
        20,
    ),
    (
        "handler-reset",
        [
            "/bin/brush", "-c",
            'trap "echo caught" USR1; exec /bin/brush -c "kill -USR1 $$; sleep 1; echo survived"',
        ],
        138,
        None,
        20,
    ),
)

BLOCKED_ROWS = (
    (
        "wait-nohang",
        "S169 kernel wait unit test is not on origin/master (report Result WORKING); "
        "the release shell's wait builtin does not implement waitpid(WNOHANG)",
    ),
    (
        "fd-cloexec",
        "S172 descriptor conformance harness/tests/conformance/harness is absent "
        "(ticket kevin/S172 status todo); no guest fcntl(F_SETFD) row exists",
    ),
    (
        "zombie-reparent",
        "no guest process-census helper in this tree; the release shell's ps row cannot "
        "observe a stable zombie/reparent transition without S172's harness",
    ),
)


def run_command(argv: list[str], timeout: int) -> tuple[int, str]:
    try:
        done = subprocess.run(
            argv,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        return 124, f"timeout after {timeout}s: {error}"
    return done.returncode, done.stdout.decode("utf-8", "replace")


def qtest_result(case: str) -> dict:
    path = ROOT / "build" / "qtest" / f"{case}.json"
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return {}


def run_qtest(case: str, required: tuple[str, ...] | None, timeout: int) -> tuple[bool, str]:
    argv = [
        "cargo", "xtask", "qtest", "--case", case, "--arch", "x86_64",
    ]
    code, output = run_command(argv, timeout)
    report = qtest_result(case)
    matched = report.get("matched", [])
    if required is None:
        ok = code == 0 and report.get("result") == "pass"
        detail = f"exit={code} result={report.get('result')} matched={len(matched)}"
    else:
        missing = [pattern for pattern in required if pattern not in matched]
        ok = not missing
        detail = f"exit={code} required missing={missing!r}"
    if not ok:
        detail += " | " + output.strip().splitlines()[-1] if output.strip() else ""
    return ok, detail


def load_command_acceptance():
    path = ROOT / "tests" / "ssh" / "command-acceptance.py"
    spec = importlib.util.spec_from_file_location("s181_command_acceptance", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_ssh_rows(image: Path, timeout: int, keep: bool) -> list[tuple[str, bool, str]]:
    rows: list[tuple[str, bool, str]] = []
    if not image.is_file():
        reason = f"missing image {image}; build with: {' '.join(BUILD_IMAGE)}"
        return [(name, False, f"BLOCKED {reason}") for name, *_ in SSH_ROWS]
    module = load_command_acceptance()
    run_dir = ROOT / "build" / "conformance" / "process" / time.strftime(
        "%Y%m%dT%H%M%SZ", time.gmtime()
    )
    port = module.free_port()
    child = None
    try:
        child, serial = module.boot(run_dir, image, port)
        for name, argv, expected_exit, expected_stdout, row_timeout in SSH_ROWS:
            row = module.run_row(serial, port, argv, min(timeout, row_timeout))
            ok = (
                not row["timed_out"]
                and row["exit"] == expected_exit
                and (
                    expected_stdout is None
                    or row["stdout"] == expected_stdout
                )
            )
            detail = (
                f"exit={row['exit']} timed_out={int(row['timed_out'])} "
                f"stdout_hex={row['stdout'].hex()} expected_exit={expected_exit} "
                f"expected_stdout={None if expected_stdout is None else expected_stdout.hex()} "
                f"fault={row['fault']!r}"
            )
            rows.append((name, ok, detail))
    except Exception as error:  # boot/setup failure is a row failure with the cause
        rows = [(name, False, f"boot/setup failed: {error}") for name, *_ in SSH_ROWS]
    finally:
        if child is not None and not keep:
            try:
                os.killpg(child.pid, 9)
            except ProcessLookupError:
                pass
            child.wait()
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=IMAGE)
    parser.add_argument("--build-image", action="store_true")
    parser.add_argument("--host-only", action="store_true")
    parser.add_argument("--timeout", type=int, default=20)
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--only", action="append", default=None)
    args = parser.parse_args()
    image = args.image if args.image.is_absolute() else ROOT / args.image

    if args.build_image and not image.is_file():
        command = BUILD_IMAGE.copy()
        command[-1] = str(image)
        print(f"ROW BUILD image command={' '.join(command)}", flush=True)
        code, output = run_command(command, 420)
        if code != 0:
            print(f"ROW FAIL image-build exit={code} output={output[-2000:]!r}")
            return 1

    rows: list[tuple[str, bool, str]] = []
    selected = set(args.only) if args.only else None
    for name, argv, timeout in HOST_ROWS:
        if selected is not None and name not in selected:
            continue
        print(f"ROW START name={name}", flush=True)
        code, output = run_command(argv, timeout)
        rows.append((name, code == 0, f"exit={code} tail={output.strip().splitlines()[-1:]!r}"))
    for name, case, required, timeout in QTEST_ROWS:
        if selected is not None and name not in selected:
            continue
        print(f"ROW START name={name} case={case}", flush=True)
        ok, detail = run_qtest(case, required, timeout)
        rows.append((name, ok, detail))
    if not args.host_only and (selected is None or any(name in selected for name, *_ in SSH_ROWS)):
        rows.extend(run_ssh_rows(image, args.timeout, args.keep))
    for name, reason in BLOCKED_ROWS:
        if selected is not None and name not in selected:
            continue
        rows.append((name, False, f"BLOCKED {reason}"))

    failures = 0
    for name, ok, detail in rows:
        if not ok:
            failures += 1
        print(f"ROW {'PASS' if ok else 'FAIL'} name={name} {detail}")
    print(f"S181-PROCESS {'PASS' if failures == 0 else 'FAIL'} failures={failures}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
