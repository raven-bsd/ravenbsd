#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S199 guarded-buffer conformance for the SIGACTION seam (x86_64).

Boots the S199 image once and asserts the in-band `sigact:` rows printed by
`/bin/sigact-guard`:

  * the kernel request is the 40-byte S160 layout (handler 0, mask 8, flags 24, restorer 32);
  * an install with a non-null request leaves the canary words before and after it unchanged;
  * a query with non-null `act` and non-null `old` round-trips handler/mask/flags and leaves every
    canary around both 40-byte objects unchanged;
  * restoring `SIG_DFL` with a non-null `old` captures the caught action and leaves every canary
    unchanged, and a following query reads `SIG_DFL` back.

`--mutation` expects a kernel built with the pre-aef6f7f58 152-byte `UserSigAction` mirror. That
kernel writes 152 bytes into the 40-byte `old` object, so the guarded `query`/`restore` rows turn
red and the harness must report MUTATION RED. The product tree is not changed by the harness.

Usage:
  python3 tests/conformance/signals/run.py --image build/conformance/signals/signals.img
  python3 tests/conformance/signals/run.py --image ... --mutation
  python3 tests/conformance/signals/run.py --selftest

Exit: 0 all rows green, 1 a row failed or the mutation did not turn the rows red, 2 setup error.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))

KV_RE = re.compile(r"([A-Za-z0-9_-]+)=([^\s]+)")

GATE_ROWS = [
    ("layout", "size", "40"),
    ("layout", "handler", "0"),
    ("layout", "mask", "8"),
    ("layout", "flags", "24"),
    ("layout", "restorer", "32"),
    ("install", "ret", "0"),
    ("install", "new-before", "ok"),
    ("install", "new-after", "ok"),
    ("query", "ret", "0"),
    ("query", "new-null", "0"),
    ("query", "old-null", "0"),
    ("query", "handler", "1"),
    ("query", "mask", "1"),
    ("query", "flags", "1"),
    ("query", "new-before", "ok"),
    ("query", "new-after", "ok"),
    ("query", "old-before", "ok"),
    ("query", "old-after", "ok"),
    ("restore", "ret", "0"),
    ("restore", "old-handler", "1"),
    ("restore", "new-before", "ok"),
    ("restore", "new-after", "ok"),
    ("restore", "old-before", "ok"),
    ("restore", "old-after", "ok"),
    ("default", "ret", "0"),
    ("default", "handler", "1"),
    ("default", "old-before", "ok"),
    ("default", "old-after", "ok"),
    ("done", "failures", "0"),
]

SERIAL_BAD_MARKERS = (
    "KERNEL PANIC",
    "EXCEPTION",
    "SegmentOutsideUserRange",
    "bad ELF",
    "double fault",
)


def parse_rows(text: str) -> dict[str, dict[str, str]]:
    """Parse the probe's `sigact: <row> key=value ...` lines."""
    rows: dict[str, dict[str, str]] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("sigact: "):
            continue
        body = line[len("sigact: ") :]
        head, _, rest = body.partition(" ")
        if not head or head == "canary-bad" or head == "FAIL":
            continue
        rows[head] = {key: value for key, value in KV_RE.findall(rest)}
    return rows


def check_rows(rows: dict[str, dict[str, str]], text: str) -> list[str]:
    """Return the failed assertions with the measured values."""
    failures: list[str] = []
    for row, key, expected in GATE_ROWS:
        fields = rows.get(row)
        if fields is None:
            failures.append(f"{row}: row missing")
            continue
        actual = fields.get(key)
        if actual != expected:
            failures.append(f"{row}: {key}={actual!r} expected {expected!r}")
    if "sigact: ok" not in text:
        failures.append("serial: no `sigact: ok` row")
    for line in text.splitlines():
        if line.strip().startswith("sigact: canary-bad"):
            failures.append(f"canary: {line.strip()}")
        elif line.strip().startswith("sigact: FAIL"):
            failures.append(f"probe: {line.strip()}")
    return failures


def mutation_tripped(rows: dict[str, dict[str, str]]) -> list[str]:
    """The rows a 152-byte kernel mirror must turn red."""
    tripped: list[str] = []
    for row in ("query", "restore", "default"):
        fields = rows.get(row, {})
        if fields.get("old-after") != "ok":
            tripped.append(f"{row}: old-after={fields.get('old-after')!r}")
        if fields.get("old-before") != "ok":
            tripped.append(f"{row}: old-before={fields.get('old-before')!r}")
    query = rows.get("query", {})
    if query.get("flags") != "1":
        tripped.append(f"query: flags={query.get('flags')!r} (152-byte kernel reads flags at 136)")
    done = rows.get("done", {})
    if done.get("failures") not in (None, "0"):
        tripped.append(f"done: failures={done.get('failures')!r}")
    return tripped


def selftest() -> int:
    """Exercise the parser and every assertion with a clean and dirty fixture."""
    failures: list[str] = []
    clean = "\n".join(
        [
            "sigact: layout size=40 handler=0 mask=8 flags=24 restorer=32",
            "sigact: install ret=0 new-before=ok new-after=ok",
            "sigact: query ret=0 new-null=0 old-null=0 handler=1 mask=1 flags=1 "
            "flags-value=0x10000000 new-before=ok new-after=ok old-before=ok old-after=ok",
            "sigact: restore ret=0 old-handler=1 new-before=ok new-after=ok "
            "old-before=ok old-after=ok",
            "sigact: default ret=0 handler=1 old-before=ok old-after=ok",
            "sigact: done failures=0",
            "sigact: ok",
        ]
    )

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("clean", check_rows(parse_rows(clean), clean), [])

    dirty_canary = clean.replace("old-after=ok", "old-after=bad")
    check(
        "dirty-canary",
        any("query: old-after" in failure or "restore: old-after" in failure
            for failure in check_rows(parse_rows(dirty_canary), dirty_canary)),
        True,
    )
    dirty_flags = clean.replace("flags=1", "flags=0")
    check(
        "dirty-flags",
        any("query: flags" in failure for failure in check_rows(parse_rows(dirty_flags), dirty_flags)),
        True,
    )
    dirty_done = clean.replace("sigact: done failures=0", "sigact: done failures=2")
    check(
        "dirty-done",
        any("done: failures" in failure for failure in check_rows(parse_rows(dirty_done), dirty_done)),
        True,
    )
    dirty_line = clean.replace(
        "sigact: ok",
        "sigact: canary-bad side=query-old-after word=0 expected=0x1 got=0x0\nsigact: ok",
    )
    check(
        "dirty-canary-line",
        any("canary:" in failure for failure in check_rows(parse_rows(dirty_line), dirty_line)),
        True,
    )
    check("mutation-clean", mutation_tripped(parse_rows(clean)), [])
    check(
        "mutation-dirty",
        [row for row in mutation_tripped(parse_rows(dirty_canary)) if "old-after" in row] != [],
        True,
    )
    if failures:
        for failure in failures:
            print(f"signals-conformance: selftest FAIL: {failure}", file=sys.stderr)
        return 1
    print("signals-conformance: selftest PASS")
    return 0


def accel_args(accel: str) -> list[str]:
    if accel == "tcg":
        return ["-accel", "tcg"]
    if accel == "kvm":
        return ["-enable-kvm", "-cpu", "host"]
    raise SystemExit(f"signals-conformance: unknown accelerator {accel!r}")


def boot(run: Path, image: Path, accel: str) -> tuple[subprocess.Popen, Path]:
    """Boot the image without SSH; the staged one-shot service owns the lifetime."""
    run.mkdir(parents=True, exist_ok=True)
    vars_path = run / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = run / "serial.log"
    serial.write_bytes(b"")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        *accel_args(accel),
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-netdev", "user,id=n0",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    with open(run / "qemu.err", "wb") as error_file:
        child = subprocess.Popen(
            argv, cwd=ROOT, start_new_session=True,
            stdout=subprocess.DEVNULL, stderr=error_file,
        )
    return child, serial


def stop(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGTERM)
        child.wait(timeout=10)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def run(image: Path, accel: str, mutation: bool, keep: bool) -> int:
    if not image.is_file():
        print(f"signals-conformance: image {image} is missing", file=sys.stderr)
        return 2
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = ROOT / "build" / "conformance" / "signals" / f"run-{stamp}"
    child, serial = boot(run_dir, image, accel)
    try:
        # The one-shot probe runs early; 180 s covers firmware, initfs and svcd on TCG.
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            try:
                text = serial.read_text(encoding="utf-8", errors="replace")
            except FileNotFoundError:
                text = ""
            if "sigact: done" in text or "init exited (" in text or child.poll() is not None:
                break
            time.sleep(0.2)
        serial_text = serial.read_text(encoding="utf-8", errors="replace")
        (run_dir / "serial.out").write_text(serial_text, encoding="utf-8")
        rows = parse_rows(serial_text)
        failures = check_rows(rows, serial_text)
        for marker in SERIAL_BAD_MARKERS:
            if marker in serial_text:
                failures.append(f"serial: {marker}")
        if mutation:
            tripped = mutation_tripped(rows)
            if tripped:
                print("signals-conformance: MUTATION RED " + "; ".join(tripped))
                print(f"signals-conformance: run={run_dir}")
                return 1
            print(
                "signals-conformance: MUTATION DID NOT TURN THE GUARDED ROWS RED",
                file=sys.stderr,
            )
            for failure in failures:
                print(f"signals-conformance: MUTATION context: {failure}", file=sys.stderr)
            return 1
        if failures:
            for failure in failures:
                print(f"signals-conformance: FAIL: {failure}")
            print(f"signals-conformance: run={run_dir}")
            return 1
        print(
            "signals-conformance: PASS layout guarded-install guarded-query guarded-restore "
            "guarded-default"
        )
        print(f"signals-conformance: run={run_dir}")
        return 0
    finally:
        stop(child)
        if not keep:
            (run_dir / "vars.fd").unlink(missing_ok=True)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--accel", choices=("tcg", "kvm"), default="tcg")
    parser.add_argument("--mutation", action="store_true")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--selftest", action="store_true")
    options = parser.parse_args(argv)
    if options.selftest:
        return selftest()
    if options.image is None:
        print("signals-conformance: --image is required (or --selftest)", file=sys.stderr)
        return 2
    image = options.image if options.image.is_absolute() else ROOT / options.image
    return run(image, options.accel, options.mutation, options.keep)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
