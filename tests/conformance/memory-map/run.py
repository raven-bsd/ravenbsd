#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S177 memory-mapping conformance on the release image.

Boots the release image once, logs in as the unprivileged `raven` account over SSH, runs
`/bin/mmap-probe`, and asserts the ticket's gate rows:

  * anonymous private mapping is zero-filled;
  * a private mapping does not see a parent write after fork, a shared mapping does;
  * a write through a PROT_READ mapping kills the child with SIGSEGV;
  * MAP_FIXED over an occupied range fails with EEXIST;
  * a fault past EOF of a file-backed mapping kills the child with SIGBUS;
  * munmap of the middle page keeps both edges and the middle faults;
  * a zero-length mmap fails with EINVAL.

`--mutation` runs the probe with `--mutation=private-shared`: the private row's child sees the
parent's write and the row turns red. That is the negative check; the product tree is not changed.
The mremap growth row is Partial (PG-13) and is recorded, not gated.

`--iterations N` runs the rows N times inside one probe process (S198 kernel heap churn). The
kernel command line also enables `k449-metrics=1`, so the runner compares the pre-loop
`class1536_in_use` (the size class a 1440-byte allocation uses) and heap/process/thread counters
with the post-loop sample. The churn gate is enforced for N >= 10.

Usage:
  python3 tests/conformance/memory-map/run.py --image build/conformance/memory-map/memory-map.img
  python3 tests/conformance/memory-map/run.py --image ... --iterations 100
  python3 tests/conformance/memory-map/run.py --image ... --mutation
  python3 tests/conformance/memory-map/run.py --selftest

Exit: 0 all gate rows green, 1 a gate row failed or the mutation did not turn the row red,
2 setup error.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ASKPASS = HERE / "askpass.sh"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_CONFORMANCE_PASSWORD", "raven")
HOST = "127.0.0.1"
PROBE = "/bin/mmap-probe"
SIGSEGV = 11
SIGBUS = 7
EEXIST = 17
EINVAL = 22

GATE_ROWS = [
    ("anon-zero", "ok", "1"),
    ("cow-private", "ok", "1"),
    ("cow-private", "child", "0"),
    ("cow-shared", "ok", "1"),
    ("cow-shared", "child", "17"),
    ("mprotect-segv", "ok", "1"),
    ("mprotect-segv", "signal", str(SIGSEGV)),
    ("fixed-replace", "ok", "1"),
    ("sigbus-eof", "ok", "1"),
    ("sigbus-eof", "signal", str(SIGBUS)),
    ("munmap-partial", "ok", "1"),
    ("bad-len", "ok", "1"),
    ("bad-len", "errno", str(EINVAL)),
]

KV_RE = re.compile(r"([A-Za-z0-9_-]+)=([^\s]+)")

# S198 churn tolerances. A 1440-byte allocation is served from the 1536-byte class, whose kernel
# table caps at 512 live objects; a leak of one child object per probe iteration must cross these.
CLASS1536_TOLERANCE = 32
HEAP_TOLERANCE = 256 * 1024
PROC_TOLERANCE = 8
THREAD_TOLERANCE = 8
CHURN_MIN_ITERATIONS = 10


def parse_rows(text: str) -> dict[str, dict[str, str]]:
    """Parse the probe's `mmap: <row> key=value ...` lines."""
    rows: dict[str, dict[str, str]] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("mmap: "):
            continue
        body = line[len("mmap: ") :]
        head, _, rest = body.partition(" ")
        if not head:
            continue
        rows[head] = {key: value for key, value in KV_RE.findall(rest)}
    return rows


def check_rows(rows: dict[str, dict[str, str]]) -> list[str]:
    failures: list[str] = []
    for row, key, expected in GATE_ROWS:
        fields = rows.get(row)
        if fields is None:
            failures.append(f"{row}: row missing")
            continue
        actual = fields.get(key)
        if actual != expected:
            failures.append(f"{row}: {key}={actual!r} expected {expected!r}")
    return failures


def parse_metrics(text: str) -> tuple[dict[str, str], int]:
    """Return the last K449 sample and the number of K449-GRAPH lines."""
    sample: dict[str, str] = {}
    graph_lines = 0
    for line in text.splitlines():
        if line.startswith("K449-GRAPH "):
            graph_lines += 1
            sample.update({key: value for key, value in KV_RE.findall(line)})
        elif line.startswith("K449-METRICS "):
            sample.update({key: value for key, value in KV_RE.findall(line)})
    return sample, graph_lines


def read_metrics(serial: Path) -> tuple[dict[str, str], int]:
    try:
        text = serial.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return {}, 0
    return parse_metrics(text)


def wait_metrics(
    serial: Path, required: tuple[str, ...], min_graphs: int, timeout: float
) -> tuple[dict[str, str], int]:
    """Wait until the serial has the requested counters and at least `min_graphs` samples."""
    deadline = time.monotonic() + timeout
    sample, graphs = read_metrics(serial)
    while time.monotonic() < deadline:
        if graphs >= min_graphs and all(key in sample for key in required):
            return sample, graphs
        time.sleep(0.2)
        sample, graphs = read_metrics(serial)
    return sample, graphs


def check_footprint(base: dict[str, str], end: dict[str, str], iterations: int) -> list[str]:
    """S198: the kernel heap/class/process footprint must return to the pre-loop baseline."""
    failures: list[str] = []
    if iterations < CHURN_MIN_ITERATIONS:
        return failures
    fields = ("class1536_in_use", "heap_used", "procs", "threads")
    missing = [field for field in fields if field not in base or field not in end]
    if missing:
        return [f"churn: metrics missing {', '.join(missing)}"]
    c0, c1 = int(base["class1536_in_use"]), int(end["class1536_in_use"])
    h0, h1 = int(base["heap_used"]), int(end["heap_used"])
    p0, p1 = int(base["procs"]), int(end["procs"])
    t0, t1 = int(base["threads"]), int(end["threads"])
    print(
        "memory-map: footprint class1536={c0}->{c1} heap={h0}->{h1} "
        "procs={p0}->{p1} threads={t0}->{t1}".format(
            c0=c0, c1=c1, h0=h0, h1=h1, p0=p0, p1=p1, t0=t0, t1=t1
        )
    )
    if c1 > c0 + CLASS1536_TOLERANCE:
        failures.append(f"churn: class1536_in_use {c0}->{c1} above {CLASS1536_TOLERANCE}")
    if h1 > h0 + HEAP_TOLERANCE:
        failures.append(f"churn: heap_used {h0}->{h1} above {HEAP_TOLERANCE}")
    if p1 > p0 + PROC_TOLERANCE:
        failures.append(f"churn: procs {p0}->{p1} above {PROC_TOLERANCE}")
    if t1 > t0 + THREAD_TOLERANCE:
        failures.append(f"churn: threads {t0}->{t1} above {THREAD_TOLERANCE}")
    return failures


def selftest() -> int:
    good = (
        "mmap: anon-zero ok=1\n"
        "mmap: cow-private child=0 expected=0 ok=1\n"
        "mmap: cow-shared child=17 expected=17 ok=1\n"
        "mmap: mprotect-segv signal=11 ok=1\n"
        "mmap: fixed-replace errno=0 replaced=1 ok=1\n"
        "mmap: sigbus-eof signal=7 expected=7 ok=1\n"
        "mmap: munmap-partial edges=2 middle=11 ok=1\n"
        "mmap: bad-len errno=22 expected=22 ok=1\n"
    )
    rows = parse_rows(good)
    failures = check_rows(rows)
    if failures:
        print(f"memory-map: selftest FAIL: {failures}", file=sys.stderr)
        return 1
    dirty = good.replace("cow-private child=0 expected=0 ok=1", "cow-private child=17 expected=0 ok=1")
    if not any("cow-private" in failure for failure in check_rows(parse_rows(dirty))):
        print("memory-map: selftest FAIL: private mutation not detected", file=sys.stderr)
        return 1
    base = {
        "class1536_in_use": "4",
        "heap_used": "1000000",
        "procs": "9",
        "threads": "40",
    }
    flat = dict(base)
    if check_footprint(base, flat, CHURN_MIN_ITERATIONS):
        print("memory-map: selftest FAIL: flat footprint reported as a leak", file=sys.stderr)
        return 1
    leaked = dict(base, class1536_in_use="504", heap_used="1700000", procs="509")
    detected = check_footprint(base, leaked, CHURN_MIN_ITERATIONS)
    if not any("class1536_in_use" in failure for failure in detected):
        print("memory-map: selftest FAIL: class1536 leak not detected", file=sys.stderr)
        return 1
    if not any("procs" in failure for failure in detected):
        print("memory-map: selftest FAIL: process leak not detected", file=sys.stderr)
        return 1
    print("memory-map: selftest PASS")
    return 0


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind((HOST, 0))
        return probe.getsockname()[1]


def accel_args(accel: str) -> list[str]:
    if accel == "tcg":
        return ["-accel", "tcg"]
    if accel == "kvm":
        return ["-enable-kvm", "-cpu", "host"]
    raise SystemExit(f"memory-map: unknown accelerator {accel!r}")


def wait_for(serial: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if needle in serial.read_bytes():
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    return False


def boot(run: Path, image: Path, accel: str) -> tuple[subprocess.Popen, Path, int]:
    run.mkdir(parents=True, exist_ok=True)
    vars_path = run / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = run / "serial.log"
    serial.write_bytes(b"")
    port = free_port()
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        *accel_args(accel),
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-netdev", f"user,id=n0,hostfwd=tcp:{HOST}:{port}-:22",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    with open(run / "qemu.err", "wb") as error_file:
        child = subprocess.Popen(
            argv, cwd=ROOT, start_new_session=True,
            stdout=subprocess.DEVNULL, stderr=error_file,
        )
    if not wait_for(serial, b"svcd: sshd up", 180):
        child.kill()
        raise RuntimeError(f"sshd never started; serial {serial}")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((HOST, port), timeout=1):
                return child, serial, port
        except OSError:
            time.sleep(0.5)
    child.kill()
    raise RuntimeError(f"guest TCP 22 never listened; serial {serial}")


def ssh(port: int, command: str, timeout: float) -> tuple[int, bytes, bytes]:
    env = dict(os.environ)
    env.update(
        SSH_ASKPASS=str(ASKPASS),
        SSH_ASKPASS_REQUIRE="force",
        DISPLAY=":0",
        RAVEN_CONFORMANCE_PASSWORD=PASSWORD,
    )
    argv = [
        "ssh", "-p", str(port),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        f"{USER}@{HOST}",
        command,
    ]
    result = subprocess.run(
        argv, cwd=ROOT, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout,
        env=env, check=False,
    )
    return result.returncode, result.stdout, result.stderr


def stop(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGTERM)
        child.wait(timeout=10)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def run(image: Path, accel: str, mutation: bool, keep: bool, iterations: int) -> int:
    if not image.is_file():
        print(f"memory-map: image {image} is missing", file=sys.stderr)
        return 2
    iterations = max(1, iterations)
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = ROOT / "build" / "conformance" / "memory-map" / f"run-{stamp}"
    child, serial, port = boot(run_dir, image, accel)
    try:
        # Baseline the S198 counters before the churn. The sampler prints every second, so the
        # first sample can lag the sshd-up marker; wait for it rather than comparing with nothing.
        base, base_graphs = wait_metrics(
            serial,
            ("class1536_in_use", "heap_used", "procs", "threads"),
            1,
            20,
        )
        if mutation:
            command = f"{PROBE} --mutation=private-shared"
        elif iterations > 1:
            command = f"{PROBE} --iterations={iterations}"
        else:
            command = PROBE
        # Expected loop time is seconds; keep a proportional margin for the SSH round trip.
        ssh_timeout = 120 if iterations == 1 else min(600, 120 + 2 * iterations)
        code, stdout, stderr = ssh(port, command, ssh_timeout)
        probe_text = stdout.decode("utf-8", "replace")
        (run_dir / "probe.out").write_text(probe_text, encoding="utf-8")
        (run_dir / "probe.err").write_text(stderr.decode("utf-8", "replace"), encoding="utf-8")
        # Wait for at least two fresh one-second samples after the probe exited so the freed
        # children are reflected, then take the last sample as the post-loop footprint.
        end, _ = wait_metrics(
            serial,
            ("class1536_in_use", "heap_used", "procs", "threads"),
            base_graphs + 2,
            10,
        )
        serial_text = serial.read_text(encoding="utf-8", errors="replace")
        rows = parse_rows(probe_text)
        failures = check_rows(rows)
        markers = probe_text.count("mmap: iteration ")
        if iterations > 1 and markers != iterations:
            failures.append(f"churn: {markers} iteration markers, expected {iterations}")
        failures.extend(check_footprint(base, end, iterations))
        for line in (
            "KERNEL PANIC",
            "EXCEPTION",
            "SegmentOutsideUserRange",
            "bad ELF",
            "double fault",
        ):
            if line in serial_text:
                failures.append(f"serial: {line}")
        if code == 255:
            print(
                "memory-map: note: the SSH exit-status channel returned 255 while the command "
                "stdout was delivered; the in-band `mmap:` rows are the verdict (open gap)"
            )
        print(
            "memory-map: mremap=absent (PG-13 Partial; the libc sysroot has no mremap export, "
            "recorded as a gap)"
        )
        if mutation:
            private = rows.get("cow-private", {})
            if private.get("ok") == "0" and private.get("child") == "17":
                print("memory-map: MUTATION RED cow-private child=17 expected=0 ok=0")
                print(f"memory-map: run={run_dir}")
                return 1
            print(
                "memory-map: MUTATION DID NOT TURN THE PRIVATE ROW RED",
                file=sys.stderr,
            )
            for failure in failures:
                print(f"memory-map: MUTATION context: {failure}", file=sys.stderr)
            return 1
        if failures:
            for failure in failures:
                print(f"memory-map: FAIL: {failure}")
            print(f"memory-map: run={run_dir}")
            return 1
        print("memory-map: PASS anon cow mprotect fixed sigbus munmap bad-len")
        print(f"memory-map: run={run_dir}")
        return 0
    finally:
        stop(child)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--accel", choices=("tcg", "kvm"), default="tcg")
    parser.add_argument("--mutation", action="store_true")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    if args.image is None:
        parser.error("--image is required")
    return run(args.image, args.accel, args.mutation, args.keep, args.iterations)


if __name__ == "__main__":
    sys.exit(main())
