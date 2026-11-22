#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K492 boot-loop matrix: repeated boots to login in TCG/KVM modes.

Each boot is a foreground QEMU process supervised by the existing
``tests/smp/boot/soak_boot.py`` helper: it reads the serial pty, timestamps
every line, and terminates QEMU as soon as ``login:`` arrives. This wrapper
owns the matrix, the optional second heavy job, per-boot host telemetry, the
failure-token scan and the evidence summary. It never retries a failed boot.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SUPERVISOR = ROOT / "tests/smp/boot/soak_boot.py"
DEFAULT_IMAGE = ROOT / "build/x86_64/ravenbsd-x86_64.img"
DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"

MODES = {
    "tcg": ("tcg", 4, "tcg4"),
    "kvm2": ("kvm", 2, "kvm2"),
    "kvm4": ("kvm", 4, "kvm4"),
}

# The ticket names these exact conditions. The extra reset markers are the
# existing runner's stop conditions, kept so a reset cannot hide a login.
FORBIDDEN = (
    ("panic", re.compile(r"panic", re.IGNORECASE)),
    ("EXCEPTION", re.compile(r"EXCEPTION")),
    ("crash loop", re.compile(r"svcd:.*crash loop", re.IGNORECASE)),
    ("restart", re.compile(r"restart", re.IGNORECASE)),
    ("unclaimed vector", re.compile(r"unclaimed vector", re.IGNORECASE)),
    ("CPU Reset", re.compile(r"CPU Reset", re.IGNORECASE)),
    ("triple fault", re.compile(r"triple fault", re.IGNORECASE)),
)


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_output(*args: str) -> str:
    try:
        return subprocess.run(
            ["git", *args], cwd=ROOT, check=True, text=True, capture_output=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def load1() -> float:
    try:
        return round(os.getloadavg()[0], 2)
    except OSError:
        return 0.0


def process_counts() -> tuple[int, int]:
    """Other QEMU and xtask/cargo/rustc jobs (zombies/stopped ignored)."""
    qemu = jobs = 0
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            comm = (entry / "comm").read_text(encoding="utf-8", errors="replace").strip()
            stat = (entry / "stat").read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        state = stat.rsplit(")", 1)[-1].strip().split(" ", 1)[0] if ")" in stat else ""
        if state in {"Z", "X", "T", "t"}:
            continue
        if comm.startswith("qemu-system-"):
            qemu += 1
        elif comm in {"xtask", "cargo", "rustc", "clippy-driver", "rustdoc"}:
            jobs += 1
    return qemu, jobs


@dataclass
class Scan:
    login_line: str | None = None
    smp_line: str | None = None
    forbidden: list[tuple[int, str, str]] = field(default_factory=list)
    tail: str = ""


def scan_serial(text: str, smp: int) -> Scan:
    lines = text.splitlines()
    scan = Scan()
    for line in lines:
        stripped = line.strip()
        if scan.login_line is None and re.search(r"(?i)login:\s*$", stripped):
            scan.login_line = stripped
        if scan.smp_line is None and f"smp: {smp} cpus online" in stripped:
            scan.smp_line = stripped
    for number, line in enumerate(lines, 1):
        for name, pattern in FORBIDDEN:
            if pattern.search(line):
                scan.forbidden.append((number, name, line.strip()))
    scan.tail = "\n".join(lines[-40:])
    return scan


class HeavyJob:
    """A bounded second CPU job for the requested loaded boots."""

    def __init__(self, processes: int) -> None:
        self.processes = processes
        self.children: list[subprocess.Popen[bytes]] = []

    def start(self) -> None:
        for _ in range(self.processes):
            self.children.append(
                subprocess.Popen(
                    ["yes"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            )

    def stop(self) -> None:
        for child in self.children:
            if child.poll() is None:
                child.terminate()
        for child in self.children:
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        self.children.clear()


def qemu_argv(mode: tuple[str, int, str], image: Path, vars_path: Path) -> list[str]:
    accel, smp, _ = mode
    code = Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE))
    if not code.is_file():
        raise SystemExit(f"k492: missing OVMF code {code}")
    argv = [
        "qemu-system-x86_64",
        "-machine",
        "q35",
        "-m",
        "512M",
        "-smp",
        str(smp),
    ]
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    else:
        argv.extend(["-accel", "tcg"])
    argv.extend(
        [
            "-serial",
            "stdio",
            "-display",
            "none",
            "-monitor",
            "none",
            "-no-reboot",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive",
            f"if=pflash,format=raw,file={vars_path}",
            "-drive",
            f"format=raw,file={image}",
            "-nic",
            "none",
            "-netdev",
            "user,id=n0",
            "-device",
            "virtio-net-pci,netdev=n0,disable-legacy=on",
            "-device",
            "isa-debug-exit,iobase=0xf4,iosize=0x04",
        ]
    )
    return argv


def run_boot(
    image: Path,
    mode_label: str,
    mode: tuple[str, int, str],
    boot: int,
    timeout: float,
    serial_dir: Path,
) -> dict:
    accel, smp, _ = mode
    raw = serial_dir / f"{mode_label}-boot-{boot:02d}.log"
    stamped = serial_dir / f"{mode_label}-boot-{boot:02d}.stamped.log"
    work = serial_dir / f".vars-{mode_label}-{boot:02d}.fd"
    shutil.copyfile(Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS)), work)
    start = time.monotonic()
    result = subprocess.run(
        [
            sys.executable,
            str(SUPERVISOR),
            "--timeout",
            str(timeout),
            "--raw",
            str(raw),
            "--stamped",
            str(stamped),
            "--",
            *qemu_argv(mode, image, work),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    wall_s = round(time.monotonic() - start, 3)
    work.unlink(missing_ok=True)
    text = raw.read_text(encoding="utf-8", errors="replace") if raw.is_file() else ""
    scan = scan_serial(text, smp)
    record = {
        "mode": mode_label,
        "boot": boot,
        "accel": accel,
        "smp": smp,
        "wall_s": wall_s,
        "login_line": scan.login_line or "",
        "smp_line": scan.smp_line or "",
        "forbidden": "; ".join(f"{name}@{line}" for line, name, _ in scan.forbidden),
        "forbidden_text": " | ".join(text for _, _, text in scan.forbidden),
        "serial": str(raw.relative_to(serial_dir.parent)),
        "supervisor_rc": result.returncode,
        "supervisor_out": result.stdout.strip(),
        "supervisor_err": result.stderr.strip(),
        "tail": scan.tail,
    }
    if result.returncode == 0:
        match = re.search(r"login_s=([0-9.]+)", result.stdout)
        record["login_s"] = float(match.group(1)) if match else None
    else:
        record["login_s"] = None
    record["status"] = "pass" if (
        result.returncode == 0
        and scan.login_line
        and scan.smp_line
        and not scan.forbidden
    ) else "fail"
    return record


def print_failure(record: dict) -> None:
    print(
        f"k492: FAIL mode={record['mode']} boot={record['boot']} "
        f"status_rc={record['supervisor_rc']}",
        file=sys.stderr,
    )
    if record["forbidden"]:
        print(f"k492: forbidden={record['forbidden_text']}", file=sys.stderr)
    if not record["login_line"]:
        print("k492: no login: prompt", file=sys.stderr)
    if not record["smp_line"]:
        print(f"k492: no 'smp: {record['smp']} cpus online' line", file=sys.stderr)
    if record["supervisor_err"]:
        print(f"k492: supervisor: {record['supervisor_err']}", file=sys.stderr)
    print(f"k492: serial={record['serial']}", file=sys.stderr)
    print(record["tail"], file=sys.stderr)


def selftest() -> int:
    clean = (
        "loader: RavenBSD loader 0.1 (x86_64)\n"
        "RavenBSD kernel 0.1 (x86_64)\n"
        "smp: 4 cpus online\n"
        "svcd: getty up\n"
        "login: \n"
    )
    scan = scan_serial(clean, 4)
    assert scan.login_line == "login:", scan
    assert scan.smp_line == "smp: 4 cpus online", scan
    assert not scan.forbidden, scan
    for token in (
        "KERNEL PANIC",
        "EXCEPTION: bad ELF",
        "svcd: netd crash loop",
        "kern: restart requested",
        "UNCLAIMED VECTOR 0x20",
        "CPU Reset",
        "triple fault",
    ):
        bad = scan_serial(clean + token + "\n", 4)
        assert bad.forbidden, token
    print("k492: selftest PASS (clean login accepted, all failure tokens detected)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=("tcg", "kvm2", "kvm4", "all"),
        default="all",
        help="boot mode (default all three)",
    )
    parser.add_argument("--boots", type=int, default=20, help="boots per mode")
    parser.add_argument(
        "--heavy",
        type=int,
        default=5,
        help="last N boots per mode run with the second heavy job",
    )
    parser.add_argument(
        "--load-processes",
        type=int,
        default=max(2, (os.cpu_count() or 4) // 4),
        help="CPU burner processes for the heavy boots",
    )
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--timeout-kvm", type=float, default=30.0)
    parser.add_argument("--timeout-tcg", type=float, default=45.0)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.boots < 1:
        parser.error("--boots must be at least 1")
    if args.heavy < 0 or args.heavy > args.boots:
        parser.error("--heavy must be between 0 and --boots")
    if args.load_processes < 1:
        parser.error("--load-processes must be at least 1")
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        raise SystemExit(f"k492: image not found: {image} (build it first)")
    if not Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS)).is_file():
        raise SystemExit("k492: OVMF vars not found")

    labels = ["tcg", "kvm2", "kvm4"] if args.mode == "all" else [args.mode]
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = (args.out or (ROOT / "build/k492")) / f"run-{stamp}"
    out.mkdir(parents=True, exist_ok=True)
    serial_dir = out / "serials"
    serial_dir.mkdir(parents=True, exist_ok=True)
    image_hash = sha256_file(image)
    commit = git_output("rev-parse", "HEAD")
    boot_log = out / "summary.tsv"
    rows: list[dict] = []
    failed = False
    started = utc_now()
    print(
        f"k492: start utc={started} commit={commit} image={image} sha256={image_hash} "
        f"load1={load1()} nproc={os.cpu_count()} boots={args.boots} heavy={args.heavy}"
    )
    with boot_log.open("w", encoding="utf-8") as handle:
        handle.write(
            "mode\tboot\tstatus\tlogin_line\tlogin_s\twall_s\tload1_before\tload1_after\t"
            "qemu_before\tjobs_before\theavy\tforbidden\tserial\n"
        )
        for label in labels:
            mode = MODES[label]
            timeout = args.timeout_tcg if mode[0] == "tcg" else args.timeout_kvm
            heavy = HeavyJob(args.load_processes)
            heavy_started = False
            try:
                for boot in range(1, args.boots + 1):
                    loaded = boot > args.boots - args.heavy
                    if loaded and not heavy_started:
                        heavy.start()
                        heavy_started = True
                        # Give load average a moment to reflect the second job.
                        time.sleep(2)
                    before = load1()
                    qemu_before, jobs_before = process_counts()
                    record = run_boot(
                        image, label, mode, boot, timeout, serial_dir
                    )
                    after = load1()
                    record["load1_before"] = before
                    record["load1_after"] = after
                    record["qemu_before"] = qemu_before
                    record["jobs_before"] = jobs_before
                    record["heavy"] = "yes" if loaded else "no"
                    rows.append(record)
                    handle.write(
                        f"{record['mode']}\t{record['boot']}\t{record['status']}\t"
                        f"{record['login_line']}\t{record['login_s']}\t{record['wall_s']}\t"
                        f"{before}\t{after}\t{qemu_before}\t{jobs_before}\t"
                        f"{record['heavy']}\t{record['forbidden']}\t{record['serial']}\n"
                    )
                    handle.flush()
                    if record["status"] != "pass":
                        print_failure(record)
                        failed = True
                        break
                    print(
                        f"k492: PASS mode={label} boot={boot}/{args.boots} "
                        f"login_s={record['login_s']} wall_s={record['wall_s']} "
                        f"heavy={record['heavy']} load1={before}->{after}"
                    )
            finally:
                heavy.stop()
            if failed:
                break
    summary = {
        "commit": commit,
        "image": str(image),
        "image_sha256": image_hash,
        "started_utc": started,
        "finished_utc": utc_now(),
        "boots_requested": args.boots,
        "heavy_boots": args.heavy,
        "load_processes": args.load_processes,
        "modes": labels,
        "failed": failed,
        "rows": rows,
    }
    (out / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    hashes = []
    for path in sorted(out.rglob("*")):
        if path.is_file() and path.name != "SHA256SUMS":
            hashes.append(f"{sha256_file(path)}  {path.relative_to(out)}")
    (out / "SHA256SUMS").write_text("\n".join(hashes) + "\n", encoding="utf-8")
    if failed:
        print(f"k492: FAIL stopped at first failed boot; summary={boot_log}", file=sys.stderr)
        return 1
    print(f"k492: PASS all runs requested={args.boots} modes={','.join(labels)}")
    print(f"k492: summary={boot_log}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
