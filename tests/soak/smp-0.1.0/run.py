#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S85h 0.1.0 SMP soak: repeated foreground boots to the login marker.

Each cycle starts one QEMU process through the existing foreground serial
supervisor ``tests/smp/boot/soak_boot.py``. The supervisor stops QEMU as soon
as ``login:`` arrives. A cycle fails on a missing CPU, a missing login marker,
a panic/exception/TLB-shootdown-timeout token, a supervisor timeout, or a
non-zero QEMU exit. Every cycle's serial is kept and hashed in the run
directory so the evidence is reproducible.

Evidence is written below
``<prjmgmt>/tickets/evidence/artifacts/S85h/runs/`` and never into the product
tree. ``RAVEN_PRJMGMT`` names the prjmgmt checkout when it is not a sibling of
the product clone.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).absolute().parents[3]
SUPERVISOR = ROOT / "tests/smp/boot/soak_boot.py"

DEFAULT_IMAGES = {
    "x86_64": "build/x86_64/ravenbsd-x86_64.img",
    "aarch64": "build/aarch64/login-cold-start-release.img",
}
DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"
DEFAULT_AAVMF_CODE = "/usr/share/AAVMF/AAVMF_CODE.fd"
DEFAULT_AAVMF_VARS = "/usr/share/AAVMF/AAVMF_VARS.fd"

HARD_FAILURES = (
    ("panic", re.compile(r"panic", re.IGNORECASE)),
    ("EXCEPTION", re.compile(r"EXCEPTION")),
    ("unclaimed vector", re.compile(r"unclaimed vector", re.IGNORECASE)),
    ("CPU Reset", re.compile(r"CPU Reset", re.IGNORECASE)),
    ("triple fault", re.compile(r"triple fault", re.IGNORECASE)),
    ("randomization window", re.compile(r"randomization window", re.IGNORECASE)),
    ("tlb shootdown timeout", re.compile(r"tlb shootdown timeout", re.IGNORECASE)),
    ("login failed", re.compile(r"login: .*(?:incorrect|failed)", re.IGNORECASE)),
    ("getty failed", re.compile(r"getty failed", re.IGNORECASE)),
)
OBSERVATIONS = (
    ("crash loop", re.compile(r"svcd:.*crash loop", re.IGNORECASE)),
    ("service failed", re.compile(r"^svcd: .*failed", re.IGNORECASE)),
    ("aslr entropy fallback", re.compile(r"aslr: entropy unavailable", re.IGNORECASE)),
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


@dataclass
class Scan:
    login_line: str | None = None
    smp_line: str | None = None
    failures: list[tuple[int, str, str]] = field(default_factory=list)
    observations: list[tuple[int, str, str]] = field(default_factory=list)
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
        for name, pattern in HARD_FAILURES:
            if pattern.search(line):
                scan.failures.append((number, name, line.strip()))
        for name, pattern in OBSERVATIONS:
            if pattern.search(line):
                scan.observations.append((number, name, line.strip()))
    scan.tail = "\n".join(lines[-40:])
    return scan


def cell_error(arch: str, accel: str, smp: int) -> str | None:
    if arch not in ("x86_64", "aarch64"):
        return f"unsupported architecture {arch!r}"
    if accel not in ("kvm", "tcg"):
        return f"unsupported accelerator {accel!r}"
    if smp < 1:
        return f"invalid cpu count {smp}"
    if arch == "aarch64" and accel != "tcg":
        return "aarch64 supports only the tcg accelerator on this host"
    return None


def qemu_argv(arch: str, accel: str, smp: int, image: Path, vars_path: Path) -> list[str]:
    if arch == "aarch64":
        code = Path(os.environ.get("RAVEN_AAVMF_CODE", DEFAULT_AAVMF_CODE))
        return [
            "qemu-system-aarch64",
            "-M",
            "virt,gic-version=2",
            "-cpu",
            "cortex-a72",
            "-m",
            "512M",
            "-smp",
            str(smp),
            "-accel",
            "tcg",
            "-serial",
            "stdio",
            "-display",
            "none",
            "-monitor",
            "none",
            "-no-reboot",
            "-semihosting-config",
            "enable=on,target=native",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive",
            f"if=pflash,format=raw,file={vars_path}",
            "-drive",
            f"format=raw,file={image}",
            "-nic",
            "none",
        ]
    code = Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE))
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


def firmware_paths(arch: str) -> tuple[Path, Path]:
    if arch == "aarch64":
        return (
            Path(os.environ.get("RAVEN_AAVMF_CODE", DEFAULT_AAVMF_CODE)),
            Path(os.environ.get("RAVEN_AAVMF_VARS", DEFAULT_AAVMF_VARS)),
        )
    return (
        Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE)),
        Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS)),
    )


def prjmgmt_root() -> Path:
    override = os.environ.get("RAVEN_PRJMGMT")
    if override:
        return Path(override).resolve()
    for base in (ROOT, Path(os.environ.get("PWD", ROOT))):
        for parent in (base, *base.parents):
            candidate = parent / "ravenbsd-prjmgmt"
            if (candidate / "tickets").is_dir():
                return candidate
    raise SystemExit("s85h: cannot locate ravenbsd-prjmgmt; set RAVEN_PRJMGMT")


def default_out(arch: str, accel: str, smp: int) -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    label = f"{stamp}-{arch}-{accel}-smp{smp}"
    return prjmgmt_root() / "tickets/evidence/artifacts/S85h/runs" / label


def resolve_image(arch: str, requested: str | None) -> Path:
    if requested:
        image = Path(requested)
    else:
        image = Path(os.environ.get("RAVEN_SOAK_IMAGE", DEFAULT_IMAGES[arch]))
    return image if image.is_absolute() else ROOT / image


def tsv(value: object) -> str:
    return str(value).replace("\t", " ").replace("\n", " ")


def write_summary(out: Path, rows: list[dict], meta: dict) -> None:
    fields = (
        "cycle",
        "status",
        "arch",
        "accel",
        "smp",
        "wall_s",
        "login_s",
        "login_line",
        "smp_line",
        "failure_class",
        "failure_text",
        "observations",
        "serial",
        "supervisor_rc",
    )
    with (out / "summary.tsv").open("w", encoding="utf-8") as handle:
        handle.write("\t".join(fields) + "\n")
        for row in rows:
            handle.write("\t".join(tsv(row.get(name, "")) for name in fields) + "\n")
    summary = dict(meta, rows=rows)
    (out / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    hashes = []
    for path in sorted(out.rglob("*")):
        if path.is_file() and path.name != "SHA256SUMS":
            hashes.append(f"{sha256_file(path)}  {path.relative_to(out)}")
    (out / "SHA256SUMS").write_text("\n".join(hashes) + "\n", encoding="utf-8")


def print_failure(record: dict) -> None:
    print(
        f"s85h: FAIL arch={record['arch']} accel={record['accel']} smp={record['smp']} "
        f"cycle={record['cycle']} failure_class={record['failure_class']}",
        file=sys.stderr,
    )
    if record["failure_text"]:
        print(f"s85h: failure={record['failure_text']}", file=sys.stderr)
    if not record["login_line"]:
        print("s85h: no login: prompt", file=sys.stderr)
    if not record["smp_line"]:
        print(f"s85h: no 'smp: {record['smp']} cpus online' line", file=sys.stderr)
    if record["supervisor_err"]:
        print(f"s85h: supervisor: {record['supervisor_err']}", file=sys.stderr)
    print(f"s85h: serial={record['serial']}", file=sys.stderr)
    print(record["tail"], file=sys.stderr)


def run_cycle(
    arch: str,
    accel: str,
    smp: int,
    image: Path,
    cycle: int,
    timeout: float,
    serial_dir: Path,
    firmware_vars: Path,
) -> dict:
    raw = serial_dir / f"cycle-{cycle:03d}.log"
    stamped = serial_dir / f"cycle-{cycle:03d}.stamped.log"
    work = serial_dir / f".vars-cycle-{cycle:03d}.fd"
    shutil.copyfile(firmware_vars, work)
    start = time.monotonic()
    try:
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
                *qemu_argv(arch, accel, smp, image, work),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
    finally:
        work.unlink(missing_ok=True)
    wall_s = round(time.monotonic() - start, 3)
    text = raw.read_text(encoding="utf-8", errors="replace") if raw.is_file() else ""
    scan = scan_serial(text, smp)
    failure_class: list[str] = []
    if result.returncode != 0:
        failure_class.append("supervisor")
    if not scan.login_line:
        failure_class.append("login")
    if not scan.smp_line:
        failure_class.append("missing-cpu")
    if scan.failures:
        failure_class.append("serial")
    login_s = None
    match = re.search(r"login_s=([0-9.]+)", result.stdout)
    if result.returncode == 0 and match:
        login_s = float(match.group(1))
    record = {
        "cycle": cycle,
        "arch": arch,
        "accel": accel,
        "smp": smp,
        "wall_s": wall_s,
        "login_s": login_s,
        "login_line": scan.login_line or "",
        "smp_line": scan.smp_line or "",
        "failure_class": ";".join(failure_class),
        "failure_text": " | ".join(
            f"{name}@{number}: {line}" for number, name, line in scan.failures
        ),
        "observations": "; ".join(
            f"{name}@{number}" for number, name, _ in scan.observations
        ),
        "serial": str(raw.relative_to(serial_dir.parent)),
        "supervisor_rc": result.returncode,
        "supervisor_out": result.stdout.strip(),
        "supervisor_err": result.stderr.strip(),
        "tail": scan.tail,
    }
    record["status"] = "pass" if not failure_class else "fail"
    return record


def invalid_cell(
    arch: str,
    accel: str,
    smp: int,
    cycles: int,
    out: Path,
    reason: str,
    commit: str,
) -> int:
    out.mkdir(parents=True, exist_ok=True)
    record = {
        "cycle": 0,
        "arch": arch,
        "accel": accel,
        "smp": smp,
        "wall_s": 0.0,
        "login_s": None,
        "login_line": "",
        "smp_line": "",
        "failure_class": "invalid-cell",
        "failure_text": reason,
        "observations": "",
        "serial": "",
        "supervisor_rc": 126,
        "supervisor_out": "",
        "supervisor_err": reason,
        "tail": "",
        "status": "fail",
    }
    meta = {
        "arch": arch,
        "accel": accel,
        "smp": smp,
        "cycles_requested": cycles,
        "commit": commit,
        "started_utc": utc_now(),
        "finished_utc": utc_now(),
        "failed": True,
    }
    write_summary(out, [record], meta)
    print_failure(record)
    print(f"s85h: summary={out / 'summary.tsv'}", file=sys.stderr)
    return 1


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", required=True, choices=("x86_64", "aarch64"))
    parser.add_argument("--accel", required=True)
    parser.add_argument("--smp", required=True, type=int)
    parser.add_argument("--cycles", required=True, type=int)
    parser.add_argument("--image")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--timeout-kvm", type=float, default=30.0)
    parser.add_argument("--timeout-tcg", type=float, default=60.0)
    args = parser.parse_args(argv)

    if args.cycles < 1:
        parser.error("--cycles must be at least 1")

    image = resolve_image(args.arch, args.image)
    out = args.out or default_out(args.arch, args.accel, args.smp)
    commit = git_output("rev-parse", "HEAD")
    reason = cell_error(args.arch, args.accel, args.smp)
    if reason is not None:
        return invalid_cell(args.arch, args.accel, args.smp, args.cycles, out, reason, commit)

    code, firmware_vars = firmware_paths(args.arch)
    if not image.is_file():
        return invalid_cell(
            args.arch, args.accel, args.smp, args.cycles, out,
            f"image not found: {image}", commit,
        )
    if not code.is_file():
        return invalid_cell(
            args.arch, args.accel, args.smp, args.cycles, out,
            f"firmware code not found: {code}", commit,
        )
    if not firmware_vars.is_file():
        return invalid_cell(
            args.arch, args.accel, args.smp, args.cycles, out,
            f"firmware vars not found: {firmware_vars}", commit,
        )
    if not SUPERVISOR.is_file():
        return invalid_cell(
            args.arch, args.accel, args.smp, args.cycles, out,
            f"serial supervisor not found: {SUPERVISOR}", commit,
        )

    out.mkdir(parents=True, exist_ok=True)
    serial_dir = out / "serials"
    serial_dir.mkdir(parents=True, exist_ok=True)
    meta = {
        "arch": args.arch,
        "accel": args.accel,
        "smp": args.smp,
        "cycles_requested": args.cycles,
        "image": str(image),
        "image_sha256": sha256_file(image),
        "commit": commit,
        "started_utc": utc_now(),
        "finished_utc": None,
        "load1": load1(),
        "nproc": os.cpu_count(),
        "failed": False,
    }
    timeout = args.timeout_kvm if args.accel == "kvm" else args.timeout_tcg
    print(
        f"s85h: start utc={meta['started_utc']} arch={args.arch} accel={args.accel} "
        f"smp={args.smp} cycles={args.cycles} commit={commit} image={image} "
        f"sha256={meta['image_sha256']} load1={meta['load1']} nproc={meta['nproc']} "
        f"out={out}"
    )
    rows: list[dict] = []
    failed = False
    for cycle in range(1, args.cycles + 1):
        record = run_cycle(
            args.arch,
            args.accel,
            args.smp,
            image,
            cycle,
            timeout,
            serial_dir,
            firmware_vars,
        )
        rows.append(record)
        write_summary(out, rows, meta)
        if record["status"] != "pass":
            print_failure(record)
            failed = True
            break
        print(
            f"s85h: PASS cycle={cycle}/{args.cycles} accel={args.accel} "
            f"login_s={record['login_s']} wall_s={record['wall_s']} "
            f"obs={record['observations'] or '-'}"
        )
    meta["finished_utc"] = utc_now()
    meta["failed"] = failed
    write_summary(out, rows, meta)
    if failed:
        print(
            f"s85h: FAIL stopped at first failed cycle; summary={out / 'summary.tsv'}",
            file=sys.stderr,
        )
        return 1
    print(
        f"s85h: PASS arch={args.arch} accel={args.accel} smp={args.smp} "
        f"cycles={args.cycles}"
    )
    print(f"s85h: summary={out / 'summary.tsv'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
