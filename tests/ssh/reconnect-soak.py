#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K449 SSH reconnect soak.

Boots the release gate image once and runs fresh ``ssh raven@... echo <marker>`` sessions, each
bounded, recording the exit code, wall time, exact marker bytes and the client-observed server
lines (Authenticated, exit-status, channel free). A non-zero exit, a hang or a missing marker is a
FAIL with the session number, its raw bytes and the client log; the default stops the batch on the
first failure (``--keep-going`` exists only to expose the failure pattern in a bounded
measurement).

The harness is reusable unchanged for the 100/100 gate:
  python3 tests/ssh/reconnect-soak.py --sessions 100 --batch 10
and optionally adds the TTY rows:
  python3 tests/ssh/reconnect-soak.py --sessions 100 --tty-sessions 10

Every ``--resource-every N`` command sessions (default 5) the harness also samples the resource
curve without adding an SSH row: the host QEMU RSS proxy plus, when the image was built with
``k449-metrics=<seconds>`` on the kernel cmdline, the guest kernel's heap/frame bytes, live
process/thread counts, handle (OFD) lineage count, page-table frames, kernel-stack frames, and
the present user pages of init/vfsd/netd/sshd from the ``K449-METRICS`` serial line. Images
without that test-only key report ``-`` for those fields; no value is fabricated. The samples
are written to ``resources.tsv`` and printed as a table.

Evidence is written under build/ssh-reconnect-soak/<UTC>/ (copy it to managed evidence after the
run; the frozen product evidence tree is never touched). No QEMU-free shortcut, no retry.

Usage: python3 tests/ssh/reconnect-soak.py [--sessions N] [--batch N] [--session-timeout S]
                                           [--tty-sessions N] [--connect-timeout S]
                                           [--keep-going] [--image PATH]
                                           [--net tap|user] [--resource-every N] [--sshd-log]
                                           [--selftest]
Exit: 0 when every session passes, 1 when a session fails, 2 for setup problems.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
ASKPASS = ROOT / "tests" / "ssh" / "askpass.sh"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_args(argv: list[str]) -> dict:
    options = {
        "sessions": 100,
        "batch": 10,
        "session_timeout": 20.0,
        "connect_timeout": 8.0,
        "tty_sessions": 0,
        "keep_going": False,
        "image": IMAGE,
        "net": "tap",
        "resource_every": 5,
        "selftest": False,
        "sshd_log": False,
    }
    index = 0
    while index < len(argv):
        arg = argv[index]
        if arg == "--sessions" and index + 1 < len(argv):
            options["sessions"] = int(argv[index + 1])
            index += 2
        elif arg == "--batch" and index + 1 < len(argv):
            options["batch"] = int(argv[index + 1])
            index += 2
        elif arg == "--session-timeout" and index + 1 < len(argv):
            options["session_timeout"] = float(argv[index + 1])
            index += 2
        elif arg == "--connect-timeout" and index + 1 < len(argv):
            options["connect_timeout"] = float(argv[index + 1])
            index += 2
        elif arg == "--tty-sessions" and index + 1 < len(argv):
            options["tty_sessions"] = int(argv[index + 1])
            index += 2
        elif arg == "--keep-going":
            options["keep_going"] = True
            index += 1
        elif arg == "--image" and index + 1 < len(argv):
            options["image"] = Path(argv[index + 1])
            index += 2
        elif arg == "--net" and index + 1 < len(argv):
            options["net"] = argv[index + 1]
            index += 2
        elif arg == "--resource-every" and index + 1 < len(argv):
            options["resource_every"] = int(argv[index + 1])
            index += 2
        elif arg == "--selftest":
            options["selftest"] = True
            index += 1
        elif arg == "--sshd-log":
            options["sshd_log"] = True
            index += 1
        elif arg in ("-h", "--help"):
            print(__doc__)
            raise SystemExit(0)
        else:
            print(f"reconnect-soak: unknown argument {arg}", file=sys.stderr)
            raise SystemExit(2)
    if options["sessions"] < 1 or options["batch"] < 1:
        print("reconnect-soak: --sessions and --batch must be positive", file=sys.stderr)
        raise SystemExit(2)
    if options["resource_every"] < 0:
        print("reconnect-soak: --resource-every must be zero or positive", file=sys.stderr)
        raise SystemExit(2)
    return options


# Resource sampling. The image exposes no heap-free/thread/fd counters (kstat has only the
# DTrace provider; the VM/scheduler providers are the documented follow-up in docs/kstat.md),
# and its ps/kstat print to the console rather than the SSH stdout. The curve is therefore
# host-side: QEMU RSS, the serial boot heap and the panic flag. No value is fabricated:
# unavailable counters are absent and named in the summary.
RESOURCE_COLUMNS = [
    "after_session", "qemu_rss_kb", "heap_used", "heap_free", "frames_free",
    "frames_alloc", "procs", "threads", "ofds", "pt_frames", "stack_frames",
    "init_pages", "vfsd_pages", "netd_pages", "sshd_pages",
    "c3072_free", "c3072_used", "c3072_slabs", "live_threads", "reap",
    "slot_drops", "slot_denies", "stacks_fresh", "stacks_cached",
    "stacks_dropped", "sshd_procs", "sshd_threads", "dma_used", "donations",
    "serial_heap_kb", "panic",
]
RESOURCE_NOTE = (
    "guest heap/frame bytes, live processes/threads, handle (OFD) lineage, page-table frames, "
    "kernel-stack frames, per-process user pages, the 3072-byte kmem class counters, live thread "
    "slots, reap depth, kernel stack fresh/cached/dropped counts, sshd/sshd-session process/thread "
    "counts, DMA pages in use and donation-registered threads come from the test-only K449-METRICS "
    "kernel line, enabled by `k449-metrics=<seconds>` on the image cmdline; on an image without "
    "that key those fields are '-' and only the host QEMU RSS and the serial boot heap/panic "
    "remain. The K449-METRICS sampler is a diagnostic, not a product service, and the SSH rows are "
    "untouched."
)


def parse_qemu_rss(text: str) -> int | None:
    """Return QEMU's VmRSS in kB from /proc/<pid>/status, or None."""
    for line in text.splitlines():
        if line.startswith("VmRSS:"):
            fields = line.split()
            if len(fields) >= 2 and fields[1].isdigit():
                return int(fields[1])
    return None


METRICS_RE = re.compile(
    r"K449-METRICS heap_used=(\d+) heap_free=(\d+) frames_free=(\d+) "
    r"frames_alloc=(\d+) procs=(\d+) threads=(\d+) ofds=(\d+) "
    r"pt_frames=(\d+) stack_frames=(\d+) init_pages=(\d+) "
    r"vfsd_pages=(\d+) netd_pages=(\d+) sshd_pages=(\d+) "
    r"c3072_free=(\d+) c3072_used=(\d+) c3072_slabs=(\d+) "
    r"live_threads=(\d+) reap=(\d+) slot_drops=(\d+) slot_denies=(\d+) "
    r"stacks_fresh=(\d+) stacks_cached=(\d+) stacks_dropped=(\d+) "
    r"sshd_procs=(\d+) sshd_threads=(\d+) dma_used=(\d+) donations=(\d+)"
)


def parse_metrics_line(text: str) -> dict | None:
    """Parse the newest K449-METRICS line from the guest serial."""
    matches = METRICS_RE.findall(text)
    if not matches:
        return None
    keys = ["heap_used", "heap_free", "frames_free", "frames_alloc",
            "procs", "threads", "ofds", "pt_frames", "stack_frames",
            "init_pages", "vfsd_pages", "netd_pages", "sshd_pages",
            "c3072_free", "c3072_used", "c3072_slabs", "live_threads",
            "reap", "slot_drops", "slot_denies", "stacks_fresh",
            "stacks_cached", "stacks_dropped", "sshd_procs", "sshd_threads",
            "dma_used", "donations"]
    return {key: int(value) for key, value in zip(keys, matches[-1])}


# kernel/src/memory/heap.rs `MAX_SLABS`: the per-class slab table bound.
C3072_SLAB_CAP = 256


def class_gate(sample: dict, session: int) -> str | None:
    """Return a loud line when the 3072-byte kmem class is exhausted.

    The 3072-byte class, not the frame pool, is the release-gate resource: a 2160-byte
    request can fail while frames are free when this class is full. A zero free list is
    normal while the cache can still grow, so the gate also requires the slab table at
    its cap (kernel/src/memory/heap.rs `MAX_SLABS = 256`). Samples from an image without
    the counters carry None and do not trip the gate.
    """
    free = sample.get("c3072_free")
    slabs = sample.get("c3072_slabs")
    if free is None or slabs is None:
        return None
    if free != 0 or slabs < C3072_SLAB_CAP:
        return None
    return (
        f"CLASS-EXHAUSTED session={session} c3072_free=0 "
        f"c3072_used={sample.get('c3072_used')} "
        f"c3072_slabs={sample.get('c3072_slabs')} "
        f"frames_free={sample.get('frames_free')} "
        f"frames_alloc={sample.get('frames_alloc')} "
        f"heap_used={sample.get('heap_used')} "
        f"heap_free={sample.get('heap_free')} "
        f"pt_frames={sample.get('pt_frames')} "
        f"stack_frames={sample.get('stack_frames')}"
    )


def utc_from_epoch(epoch: float) -> str:
    """Format a Unix timestamp as a UTC ISO-8601 string with milliseconds."""
    return datetime.fromtimestamp(epoch, timezone.utc).isoformat(
        timespec="milliseconds"
    ).replace("+00:00", "Z")


def utc_now() -> str:
    """Current UTC timestamp in the harness log format."""
    return utc_from_epoch(time.time())


def is_banner_line(line: str) -> bool:
    """True for the OpenSSH client line that proves the server banner arrived."""
    return "Remote protocol version" in line or "remote software version" in line


def format_resource_table(rows: list[dict]) -> str:
    """Render the resource samples as a compact table (missing values are '-')."""
    header = ["after", "rss_kb", "heap_u", "heap_f", "fr_free", "fr_alloc",
              "procs", "thr", "ofds", "pt", "stk", "init", "vfsd", "netd",
              "sshd", "c3f", "c3u", "c3s", "live_thr", "reap", "slot_d",
              "slot_n", "stk_f", "stk_c", "stk_d", "sshd_p", "sshd_t",
              "dma", "don", "heap_kb", "panic"]
    lines = ["  ".join(header)]

    def value(item: object) -> str:
        return "-" if item is None else str(item)

    for row in rows:
        lines.append("  ".join([
            value(row.get("after_session")),
            value(row.get("qemu_rss_kb")),
            value(row.get("heap_used")),
            value(row.get("heap_free")),
            value(row.get("frames_free")),
            value(row.get("frames_alloc")),
            value(row.get("procs")),
            value(row.get("threads")),
            value(row.get("ofds")),
            value(row.get("pt_frames")),
            value(row.get("stack_frames")),
            value(row.get("init_pages")),
            value(row.get("vfsd_pages")),
            value(row.get("netd_pages")),
            value(row.get("sshd_pages")),
            value(row.get("c3072_free")),
            value(row.get("c3072_used")),
            value(row.get("c3072_slabs")),
            value(row.get("live_threads")),
            value(row.get("reap")),
            value(row.get("slot_drops")),
            value(row.get("slot_denies")),
            value(row.get("stacks_fresh")),
            value(row.get("stacks_cached")),
            value(row.get("stacks_dropped")),
            value(row.get("sshd_procs")),
            value(row.get("sshd_threads")),
            value(row.get("dma_used")),
            value(row.get("donations")),
            value(row.get("serial_heap_kb")),
            "yes" if row.get("panic") else "no",
        ]))
    return "\n".join(lines) + "\n"


def selftest() -> int:
    """Exercise the resource parsers/formatter with fixed fixtures (no QEMU)."""
    failures: list[str] = []

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("rss", parse_qemu_rss("Name:\tqemu\nVmRSS:\t12345 kB\n"), 12345)
    check("rss-missing", parse_qemu_rss("Name:\tqemu\n"), None)
    metrics = parse_metrics_line(
        "K449-METRICS: sampler every 5s\n"
        "K449-METRICS heap_used=10 heap_free=20 frames_free=30 frames_alloc=40 "
        "procs=5 threads=6 ofds=7 pt_frames=8 stack_frames=9 init_pages=10 "
        "vfsd_pages=11 netd_pages=12 sshd_pages=13 "
        "c3072_free=14 c3072_used=15 c3072_slabs=16 "
        "live_threads=17 reap=18 slot_drops=19 slot_denies=20 "
        "stacks_fresh=21 stacks_cached=22 stacks_dropped=23 "
        "sshd_procs=24 sshd_threads=25 dma_used=26 donations=27\n"
    )
    check("metrics-heap-used", metrics and metrics["heap_used"], 10)
    check("metrics-frames-free", metrics and metrics["frames_free"], 30)
    check("metrics-procs", metrics and metrics["procs"], 5)
    check("metrics-threads", metrics and metrics["threads"], 6)
    check("metrics-ofds", metrics and metrics["ofds"], 7)
    check("metrics-pt-frames", metrics and metrics["pt_frames"], 8)
    check("metrics-stack-frames", metrics and metrics["stack_frames"], 9)
    check("metrics-init-pages", metrics and metrics["init_pages"], 10)
    check("metrics-vfsd-pages", metrics and metrics["vfsd_pages"], 11)
    check("metrics-netd-pages", metrics and metrics["netd_pages"], 12)
    check("metrics-sshd-pages", metrics and metrics["sshd_pages"], 13)
    check("metrics-c3072-free", metrics and metrics["c3072_free"], 14)
    check("metrics-c3072-used", metrics and metrics["c3072_used"], 15)
    check("metrics-c3072-slabs", metrics and metrics["c3072_slabs"], 16)
    check("metrics-live-threads", metrics and metrics["live_threads"], 17)
    check("metrics-reap", metrics and metrics["reap"], 18)
    check("metrics-stacks-cached", metrics and metrics["stacks_cached"], 22)
    check("metrics-sshd-procs", metrics and metrics["sshd_procs"], 24)
    check("metrics-sshd-threads", metrics and metrics["sshd_threads"], 25)
    check("metrics-donations", metrics and metrics["donations"], 27)
    check("metrics-missing", parse_metrics_line("no metrics here\n"), None)
    exhausted = class_gate({
        "c3072_free": 0, "c3072_used": 256, "c3072_slabs": 256,
        "frames_free": 377507840, "frames_alloc": 38944768,
        "heap_used": 6344848, "heap_free": 995184,
        "pt_frames": 1, "stack_frames": 2,
    }, 57)
    check("class-gate-zero", bool(exhausted) and "session=57" in exhausted, True)
    check("class-gate-used", "c3072_used=256" in (exhausted or ""), True)
    check("class-gate-frames", "frames_free=377507840" in (exhausted or ""), True)
    check("class-gate-one", class_gate({"c3072_free": 1}, 57), None)
    check("class-gate-absent", class_gate({"c3072_free": None}, 57), None)
    check("class-gate-not-capped", class_gate({
        "c3072_free": 0, "c3072_used": 37, "c3072_slabs": 37,
        "frames_free": 370499584, "frames_alloc": 45936640,
    }, 1), None)
    check("class-gate-capped-one", class_gate({
        "c3072_free": 1, "c3072_used": 255, "c3072_slabs": 256,
    }, 56), None)
    check(
        "banner-line-remote",
        is_banner_line(
            "debug1: Remote protocol version 2.0, remote software version OpenSSH_10.6"
        ),
        True,
    )
    check(
        "banner-line-local",
        is_banner_line("debug1: Local version string SSH-2.0-OpenSSH_8.9p1"),
        False,
    )
    check("utc-format", utc_from_epoch(0).endswith("Z"), True)
    check("connect-default", parse_args(["--sessions", "1"])["connect_timeout"], 8.0)
    check(
        "connect-set",
        parse_args(["--sessions", "1", "--connect-timeout", "30"])["connect_timeout"],
        30.0,
    )
    table = format_resource_table([
        {"after_session": 0, "qemu_rss_kb": 1000, "serial_heap_kb": 4096,
         "panic": False},
        {"after_session": 5, "qemu_rss_kb": 1200, "serial_heap_kb": 4096,
         "panic": True},
    ])
    header = table.splitlines()[0]
    check("table-header", "after" in header and "rss_kb" in header, True)
    check("table-row", "1000" in table and "4096" in table, True)
    check("table-panic", "yes" in table, True)
    if failures:
        for failure in failures:
            print(f"reconnect-soak: selftest FAIL: {failure}", file=sys.stderr)
        return 1
    print("reconnect-soak: selftest PASS")
    return 0


def hygiene() -> str:
    try:
        load = Path("/proc/loadavg").read_text(encoding="ascii").split()[0]
    except OSError:
        load = "unknown"
    count = subprocess.run(
        ["sh", "-c", "pgrep -c qemu-system-x86_64 || true"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    ).stdout.strip() or "0"
    return f"load1={load} qemu={count}"


def ensure_tap() -> None:
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "down"],
        cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "up"],
        cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )


def wait_serial(serial: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if needle in serial.read_bytes():
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    return False


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def boot(run: Path, image: Path, net: str) -> tuple[
        subprocess.Popen, Path, str, int, str, str]:
    host, port = HOST, 22
    if net == "tap":
        ensure_tap()
        netdev = f"tap,id=n0,ifname={TAP},script=no,downscript=no"
    elif net == "user":
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        host = "127.0.0.1"
        netdev = f"user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22"
    else:
        raise RuntimeError(f"unknown --net {net} (tap or user)")
    boot_dir = run / "boot"
    boot_dir.mkdir(parents=True, exist_ok=True)
    vars_path = boot_dir / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = boot_dir / "serial.log"
    serial.write_bytes(b"")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none",
        "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", netdev,
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    qemu_err = boot_dir / "qemu.err"
    with open(qemu_err, "wb") as err:
        child = subprocess.Popen(
            argv, cwd=ROOT, start_new_session=True,
            stdout=subprocess.DEVNULL, stderr=err,
        )
    if not wait_serial(serial, b"svcd: sshd up", 180):
        kill(child)
        raise RuntimeError(f"sshd never started; serial {serial}")
    ready_utc = utc_now()
    if not wait_serial(serial, b"netd: interface eth0 10.0.2.15/24", 30):
        kill(child)
        raise RuntimeError("netd never published 10.0.2.15")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return child, serial, host, port, ready_utc, utc_now()
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError("guest TCP 22 never listened")


def ssh_argv(marker: str, tty: bool, host: str, port: int,
             connect_timeout: float = 8.0) -> list[str]:
    command = "tty" if tty else f"echo {marker}"
    argv = ["ssh"]
    if tty:
        argv.append("-tt")
    argv += [
        "-v",
        "-p", str(port),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", f"ConnectTimeout={connect_timeout:g}",
        f"{USER}@{host}",
        command,
    ]
    return argv


def remote_argv(user: str, command: str, host: str, port: int) -> list[str]:
    """One non-interactive command connection for a diagnostic probe."""
    return [
        "ssh",
        "-p", str(port),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=8",
        f"{user}@{host}",
        command,
    ]


def collect_sshd_log(run: Path, host: str, port: int, timeout: float = 10.0) -> dict:
    """Fetch the guest `/tmp/sshd.log` (0600 root) through a root session."""
    out = run / "sshd.log"
    err = run / "sshd-log-probe.ssh.log"
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["K434_SSH_PASSWORD"] = PASSWORD
    timed_out = False
    with open(out, "wb") as out_file, open(err, "wb") as err_file:
        probe = subprocess.Popen(
            remote_argv("root", "cat /tmp/sshd.log", host, port), cwd=ROOT,
            stdin=subprocess.DEVNULL, stdout=out_file, stderr=err_file,
            env=env, start_new_session=True,
        )
        try:
            probe.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(probe.pid, signal.SIGKILL)
            probe.wait()
    text = out.read_text(encoding="utf-8", errors="replace")
    chown = [line for line in text.splitlines() if "chown" in line or "/dev/pts" in line]
    return {
        "path": str(out),
        "ok": not timed_out and probe.returncode == 0,
        "chown_lines": chown,
        "tail": text.splitlines()[-10:],
    }


def run_session(run: Path, number: int, marker: str, timeout: float, tty: bool,
                host: str, port: int, connect_timeout: float = 8.0) -> dict:
    log = run / f"session-{number}.ssh.log"
    out_path = run / f"session-{number}.stdout"
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["K434_SSH_PASSWORD"] = PASSWORD
    started = time.monotonic()
    start_epoch = time.time()
    banner_mono: float | None = None
    captured: list[str] = []

    def reader(stream) -> None:
        nonlocal banner_mono
        for raw in iter(stream.readline, b""):
            line = raw.decode("utf-8", errors="replace")
            captured.append(line)
            if banner_mono is None and is_banner_line(line):
                banner_mono = time.monotonic()

    with open(out_path, "wb") as out_file:
        child = subprocess.Popen(
            ssh_argv(marker, tty, host, port, connect_timeout), cwd=ROOT, stdin=subprocess.DEVNULL,
            stdout=out_file, stderr=subprocess.PIPE, env=env, start_new_session=True,
        )
        thread = threading.Thread(target=reader, args=(child.stderr,), daemon=True)
        thread.start()
        timed_out = False
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
        thread.join(timeout=5)
        child.stderr.close()
    text = "".join(captured)
    log.write_text(text, encoding="utf-8")
    elapsed = (time.monotonic() - started) * 1000.0
    banner_ms = round((banner_mono - started) * 1000.0, 1) if banner_mono is not None else ""
    banner_utc = (
        utc_from_epoch(start_epoch + (banner_mono - started))
        if banner_mono is not None else ""
    )
    stdout = out_path.read_bytes()
    auth = next((line for line in text.splitlines() if 'Authenticated to ' in line), "")
    exit_line = next(
        (line for line in text.splitlines() if "rtype exit-status" in line), ""
    )
    free_line = next(
        (line for line in text.splitlines() if "channel 0: free:" in line), ""
    )
    expected = (f"/dev/pts/" if tty else marker + "\n").encode()
    marker_ok = (expected in stdout) if tty else (stdout == expected)
    return {
        "session": number,
        "batch": (number - 1) // 10 + 1,
        "exit": None if timed_out else child.returncode,
        "timed_out": timed_out,
        "wall_ms": round(elapsed, 1),
        "start_utc": utc_from_epoch(start_epoch),
        "banner_utc": banner_utc,
        "banner_ms": banner_ms,
        "end_utc": utc_now(),
        "marker_expected": expected.decode(errors="replace").rstrip("\n"),
        "marker_ok": marker_ok,
        "stdout_hex": stdout.hex(),
        "auth_line": auth,
        "exit_status_line": exit_line,
        "free_line": free_line,
        "result": (
            "PASS"
            if not timed_out and child.returncode == 0 and marker_ok
            else "FAIL"
        ),
    }


def sample_host(child: subprocess.Popen, serial: Path, after_session: int) -> dict:
    """Sample the resource curve the harness can read without touching the SSH rows.

    The guest has no heap-free/thread/fd interface yet and its `ps`/`kstat` tools print to the
    guest console, not the SSH stdout (a `ps` probe also wedged the listener in the smoke), so
    the curve uses the host QEMU RSS proxy plus the serial boot heap and panic flag. Unavailable
    values stay None; nothing is invented.
    """
    sample: dict = {column: None for column in RESOURCE_COLUMNS}
    sample["after_session"] = after_session
    try:
        sample["qemu_rss_kb"] = parse_qemu_rss(
            Path(f"/proc/{child.pid}/status").read_text(encoding="ascii", errors="replace")
        )
    except OSError:
        sample["qemu_rss_kb"] = None
    try:
        serial_bytes = serial.read_bytes()
    except OSError:
        serial_bytes = b""
    metrics = parse_metrics_line(serial_bytes.decode("utf-8", errors="replace"))
    if metrics:
        sample.update(metrics)
    heap = re.search(rb"heap:\s+(\d+)\s+KiB", serial_bytes)
    sample["serial_heap_kb"] = int(heap.group(1)) if heap else None
    sample["panic"] = b"KERNEL PANIC" in serial_bytes
    return sample


def main(argv: list[str]) -> int:
    options = parse_args(argv)
    if options.get("selftest"):
        return selftest()
    image = Path(options["image"])
    if not image.is_file():
        print(f"reconnect-soak: missing image {image}", file=sys.stderr)
        return 2
    if not ASKPASS.is_file():
        print(f"reconnect-soak: missing {ASKPASS}", file=sys.stderr)
        return 2
    run = ROOT / "build" / "ssh-reconnect-soak" / datetime.now(timezone.utc).strftime(
        "%Y%m%dT%H%M%SZ"
    )
    run.mkdir(parents=True, exist_ok=True)
    hygiene_before = hygiene()
    print(f"reconnect-soak: run={run} image_sha256={sha256(image)} commit={git_head()}")
    print(f"reconnect-soak: hygiene before boot: {hygiene_before}")
    rows: list[dict] = []
    resource_rows: list[dict] = []
    sshd_log_info: dict | None = None
    last_session = 0
    failures = 0
    child = None
    try:
        child, serial, host, port, ready_utc, listening_utc = boot(run, image, options["net"])
        print(
            f"reconnect-soak: booted net={options['net']} target={host}:{port}; "
            f"sshd ready at={ready_utc} listening at={listening_utc}; "
            f"hygiene after boot: {hygiene()}"
        )
        if options["resource_every"] > 0:
            resource_rows.append(sample_host(child, serial, 0))
        total = options["sessions"]
        for number in range(1, total + 1):
            marker = f"K449-OK-{number}"
            row = run_session(
                run, number, marker, options["session_timeout"], tty=False,
                host=host, port=port, connect_timeout=options["connect_timeout"],
            )
            rows.append(row)
            last_session = number
            print(
                f"session {number}/{total}: exit={row['exit']} timed_out={row['timed_out']} "
                f"wall_ms={row['wall_ms']} start={row['start_utc']} "
                f"banner_ms={row['banner_ms']} banner={row['banner_utc']} "
                f"end={row['end_utc']} marker_ok={row['marker_ok']} "
                f"auth={bool(row['auth_line'])} exit_status={bool(row['exit_status_line'])} "
                f"free={bool(row['free_line'])} {row['result']}"
            )
            gate_sample = sample_host(child, serial, number)
            alarm = class_gate(gate_sample, number)
            if alarm:
                (run / "class-gate.txt").write_text(alarm + "\n", encoding="utf-8")
                print(f"reconnect-soak: {alarm}")
                failures += 1
                break
            if options["resource_every"] > 0 and number % options["resource_every"] == 0:
                resource_rows.append(gate_sample)
            if row["result"] != "PASS":
                failures += 1
                if not options["keep_going"]:
                    break
        if options["tty_sessions"] > 0 and failures == 0:
            for number in range(1, options["tty_sessions"] + 1):
                row = run_session(
                    run, 1000 + number, f"K449-TTY-{number}",
                    options["session_timeout"], tty=True, host=host, port=port,
                    connect_timeout=options["connect_timeout"],
                )
                rows.append(row)
                print(
                    f"tty {number}/{options['tty_sessions']}: exit={row['exit']} "
                    f"timed_out={row['timed_out']} wall_ms={row['wall_ms']} "
                    f"start={row['start_utc']} banner_ms={row['banner_ms']} "
                    f"banner={row['banner_utc']} end={row['end_utc']} "
                    f"marker_ok={row['marker_ok']} {row['result']}"
                )
                if row["result"] != "PASS":
                    failures += 1
                    if not options["keep_going"]:
                        break
        if (options["resource_every"] > 0 and last_session > 0
                and (not resource_rows
                     or resource_rows[-1]["after_session"] != last_session)):
            resource_rows.append(sample_host(child, serial, last_session))
        if options["sshd_log"]:
            sshd_log_info = collect_sshd_log(run, host, port)
            print(
                f"reconnect-soak: sshd log {sshd_log_info['path']} "
                f"ok={sshd_log_info['ok']} "
                f"chown_lines={len(sshd_log_info['chown_lines'])}"
            )
            for line in sshd_log_info["chown_lines"][:10]:
                print(f"reconnect-soak: sshd: {line}")
    finally:
        if child is not None:
            kill(child)
        if options["net"] == "tap":
            subprocess.run(
                ["sudo", "-n", "sh", "scripts/qemu-net.sh", "down"],
                cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
    columns = [
        "session", "batch", "exit", "timed_out", "wall_ms", "start_utc",
        "banner_utc", "banner_ms", "end_utc", "marker_expected",
        "marker_ok", "stdout_hex", "auth_line", "exit_status_line", "free_line",
        "result",
    ]
    with open(run / "sessions.tsv", "w", encoding="utf-8") as tsv:
        tsv.write("\t".join(columns) + "\n")
        for row in rows:
            tsv.write("\t".join(str(row[column]) for column in columns) + "\n")
    with open(run / "resources.tsv", "w", encoding="utf-8") as tsv:
        tsv.write("\t".join(RESOURCE_COLUMNS) + "\n")
        for row in resource_rows:
            tsv.write("\t".join(
                "" if row.get(column) is None else str(row.get(column))
                for column in RESOURCE_COLUMNS
            ) + "\n")
    summary = {
        "sessions": len([row for row in rows if row["session"] < 1000]),
        "tty_sessions": len([row for row in rows if row["session"] >= 1000]),
        "failed": failures,
        "hangs": len([row for row in rows if row["timed_out"]]),
        "image": str(image),
        "image_sha256": sha256(image),
        "commit": git_head(),
        "hygiene_before": hygiene_before,
        "hygiene_after": hygiene(),
        "session_timeout_s": options["session_timeout"],
        "connect_timeout_s": options["connect_timeout"],
        "net": options["net"],
        "keep_going": options["keep_going"],
        "resource_every": options["resource_every"],
        "resource_note": RESOURCE_NOTE,
        "resource_curve": resource_rows,
        "sshd_ready_utc": ready_utc,
        "listening_utc": listening_utc,
        "sshd_log": sshd_log_info,
    }
    (run / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    with open(run / "SHA256SUMS", "w", encoding="utf-8") as sums:
        for path in sorted(run.rglob("*")):
            if path.is_file() and path.name != "SHA256SUMS":
                sums.write(f"{sha256(path)}  {path.relative_to(run)}\n")
    if resource_rows:
        print("reconnect-soak: resource curve "
              "(guest K449-METRICS + host RSS; '-' = metrics image not enabled):")
        print(format_resource_table(resource_rows).rstrip("\n"))
    print(f"reconnect-soak: done sessions={summary['sessions']} failures={failures} run={run}")
    return 0 if failures == 0 else 1


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=False
    )
    return result.stdout.strip()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
