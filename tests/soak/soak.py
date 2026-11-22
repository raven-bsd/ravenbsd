#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K509 two-hour boot/SSH soak with a resumable state and stall detector.

Each iteration is a fresh foreground QEMU boot. The runner waits for `login:`,
logs in over the serial console, carries command traffic (and host-to-guest SSH
when `--ssh` is requested and the guest sshd is reachable), records guest frame
and process counts, then kills QEMU and starts the next iteration. A missing
heartbeat or a boot that never reaches login is recorded and recovered, never
left to hang the run. `state.json` and an append-only `iterations.tsv` make the
run resumable after a driver kill.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pty
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_IMAGE = ROOT / "build/k509/ravenbsd-soak.img"
DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"
FIXTURE = ROOT / "tests/fixtures/soak"
if str(FIXTURE) not in sys.path:
    sys.path.insert(0, str(FIXTURE))
from soak_driver import Monitor, Serial  # noqa: E402  (existing first-party stall reader)

MODES = {
    "tcg": ("tcg", 4),
    "kvm2": ("kvm", 2),
    "kvm4": ("kvm", 4),
}
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


def meminfo_kb(key: str) -> int | None:
    try:
        for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
            if line.startswith(key + ":"):
                parts = line.split()
                return int(parts[1]) if len(parts) > 1 else None
    except (OSError, ValueError):
        pass
    return None


def process_counts() -> tuple[int, int]:
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


def qemu_rss_kb(pid: int | None) -> int | None:
    if pid is None:
        return None
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    except (OSError, ValueError):
        pass
    return None


def host_sample() -> dict:
    qemu, jobs = process_counts()
    try:
        disk = shutil.disk_usage(ROOT)
        disk_free_kb = disk.free // 1024
    except OSError:
        disk_free_kb = None
    return {
        "load1": load1(),
        "mem_available_kb": meminfo_kb("MemAvailable"),
        "mem_free_kb": meminfo_kb("MemFree"),
        "disk_free_kb": disk_free_kb,
        "qemu": qemu,
        "jobs": jobs,
    }


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def qemu_argv(mode: tuple[str, int], image: Path, vars_path: Path, port: int | None, hang: bool) -> list[str]:
    accel, smp = mode
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
    if hang:
        argv.append("-S")
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    else:
        argv.extend(["-accel", "tcg"])
    netdev = "user,id=n0"
    if port is not None:
        netdev += f",hostfwd=tcp:127.0.0.1:{port}-:22"
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
            netdev,
            "-device",
            "virtio-net-pci,netdev=n0,disable-legacy=on",
            "-device",
            "isa-debug-exit,iobase=0xf4,iosize=0x04",
        ]
    )
    return argv


def scan_tokens(text: str) -> list[tuple[str, str]]:
    found = []
    for line in text.splitlines():
        for name, pattern in FORBIDDEN:
            if pattern.search(line):
                found.append((name, line.strip()))
    return found


def wait_any(serial: Serial, needles: tuple[str, ...], timeout: float, proc: subprocess.Popen) -> str | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        with serial.lock:
            text = serial.text
        for needle in needles:
            if needle in text:
                return needle
        if proc.poll() is not None:
            return None
        time.sleep(0.05)
    return None


def ssh_run(port: int, command: str, log_path: Path, timeout: float) -> tuple[bool, str, float]:
    try:
        import pexpect
    except ImportError:
        return False, "pexpect-missing", 0.0
    started = time.monotonic()
    args = [
        "ssh",
        "-p",
        str(port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "PreferredAuthentications=password",
        "-o",
        "PubkeyAuthentication=no",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "NumberOfPasswordPrompts=1",
        "raven@127.0.0.1",
        command,
    ]
    child = None
    detail = ""
    try:
        with log_path.open("w", encoding="utf-8") as logfile:
            child = pexpect.spawn(
                args[0], args[1:], cwd=str(ROOT), encoding="utf-8",
                codec_errors="replace", timeout=timeout, logfile=logfile,
            )
            step = child.expect(
                [r"(?i)password:\s*", r"(?i)permission denied", r"(?i)connection refused",
                 r"(?i)connection timed out", pexpect.EOF, pexpect.TIMEOUT],
                timeout=timeout,
            )
            if step != 0:
                detail = {
                    1: "auth-refused", 2: "connection-refused", 3: "connection-timeout",
                    4: "eof-before-password", 5: "timeout-before-password",
                }.get(step, "banner")
                return False, detail, round(time.monotonic() - started, 3)
            child.sendline("raven")
            step = child.expect(
                [r"K509_SSH_OK", r"(?i)permission denied", pexpect.EOF, pexpect.TIMEOUT],
                timeout=timeout,
            )
            if step == 0:
                return True, "ok", round(time.monotonic() - started, 3)
            detail = {1: "auth-refused", 2: "session-eof", 3: "session-timeout"}.get(step, "session")
    except Exception as error:  # pexpect spawn/permission failures are recorded, not fatal
        detail = str(error).splitlines()[0][:200]
    finally:
        if child is not None:
            child.close(force=True)
    return False, detail or "session", round(time.monotonic() - started, 3)


def strip_ansi(line: str) -> str:
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", line).replace("\r", "")


def count_posix_rows(text: str) -> int | None:
    lines = [strip_ansi(line).strip() for line in text.splitlines()]
    starts = [
        index for index, line in enumerate(lines)
        if re.match(r"^PID\s+PPID\b", line) or (line.startswith("PID") and "CMD" in line)
    ]
    if not starts:
        return None
    count = 0
    for line in lines[starts[-1] + 1:]:
        if not line:
            continue
        if re.match(r"^\d+\s+", line):
            count += 1
        elif count:
            break
    return count if count else None


def frame_values(text: str) -> tuple[int | None, int | None, int | None]:
    boot = re.search(r"frame allocator:\s*(\d+)\s+free bytes", text)
    values = [int(value) for value in re.findall(r"s24: frames free=(\d+)", text)]
    return (
        int(boot.group(1)) if boot else None,
        values[0] if values else None,
        values[-1] if values else None,
    )


def handle_value(text: str) -> int | None:
    values = [int(value) for value in re.findall(r"s25: exit handles=(\d+)", text)]
    return values[-1] if values else None


def write_state(path: Path, state: dict) -> None:
    temp = path.with_suffix(".tmp")
    temp.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temp, path)


def archive_partial(iter_dir: Path) -> None:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    for name in ("serial.log", "serial.stamped.log"):
        path = iter_dir / name
        if path.is_file():
            path.rename(iter_dir / f"{name}.aborted-{stamp}")


def kill_stale_qemu(out: Path, iteration: int, image: Path) -> None:
    """Kill the QEMU left by a driver that was killed mid-iteration."""
    pid_path = out / "iterations" / f"iter-{iteration:04d}" / "qemu.pid"
    if not pid_path.is_file():
        return
    try:
        pid = int(pid_path.read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        pid_path.unlink(missing_ok=True)
        return
    try:
        cmdline = Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ").decode(
            errors="replace"
        )
    except OSError:
        cmdline = ""
    if str(image) in cmdline:
        try:
            os.kill(pid, signal.SIGKILL)
            time.sleep(0.2)
        except ProcessLookupError:
            pass
    pid_path.unlink(missing_ok=True)


def run_iteration(
    image: Path,
    mode_name: str,
    iteration: int,
    out: Path,
    boot_timeout: float,
    login_timeout: float,
    command_timeout: float,
    stall: float,
    ssh: bool,
    ssh_timeout: float,
    hang: bool,
) -> dict:
    mode = MODES[mode_name]
    accel, smp = mode
    iter_dir = out / "iterations" / f"iter-{iteration:04d}"
    iter_dir.mkdir(parents=True, exist_ok=True)
    archive_partial(iter_dir)
    vars_copy = iter_dir / "OVMF_VARS.fd"
    shutil.copyfile(Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS)), vars_copy)
    port = free_port() if ssh else None
    before = host_sample()
    started_utc = utc_now()
    start = time.monotonic()
    master, slave = pty.openpty()
    proc = subprocess.Popen(
        qemu_argv(mode, image, vars_copy, port, hang),
        stdin=slave,
        stdout=slave,
        stderr=slave,
        close_fds=True,
        start_new_session=True,
    )
    os.close(slave)
    (iter_dir / "qemu.pid").write_text(str(proc.pid), encoding="utf-8")
    serial = Serial(master, iter_dir / "serial.log", iter_dir / "serial.stamped.log")
    serial.start_reader()
    monitor = Monitor(stall)
    record = {
        "iteration": iteration,
        "mode": mode_name,
        "start_utc": started_utc,
        "end_utc": "",
        "status": "running",
        "login_s": None,
        "wall_s": None,
        "ssh": "not-requested",
        "ssh_detail": "",
        "ssh_s": None,
        "frames_boot": None,
        "frames_first": None,
        "frames_last": None,
        "handles": None,
        "processes": None,
        "serial": str((iter_dir / "serial.log").relative_to(out)),
        "forbidden": "",
        "tail": "",
        "host_before": before,
        "host_after": {},
        "qemu_rss_kb": None,
    }
    try:
        if hang:
            # A deliberately stopped CPU must be caught by the boot timeout, not
            # by the command stall monitor (there is no login yet).
            time.sleep(boot_timeout)
            record["status"] = "boot-timeout"
            record["tail"] = "hung boot: QEMU -S, no login within boot timeout"
            return record
        if not serial.wait_text("login:", boot_timeout, proc):
            record["status"] = "boot-timeout" if proc.poll() is None else "qemu-exit"
            record["tail"] = "\n".join(serial.text.splitlines()[-40:])
            return record
        monitor.progress()
        serial.send("root\n")
        if not serial.wait_text("Password:", login_timeout, proc):
            record["status"] = "login-timeout"
            record["tail"] = "\n".join(serial.text.splitlines()[-40:])
            return record
        serial.send("raven\n")
        if not wait_any(serial, ("raven# ", "# "), login_timeout, proc):
            record["status"] = "login-timeout"
            record["tail"] = "\n".join(serial.text.splitlines()[-40:])
            return record
        record["login_s"] = round(time.monotonic() - start, 3)
        monitor.progress()

        serial.send(f"echo K509_ITER_{iteration}\n")
        if not serial.wait_text(f"K509_ITER_{iteration}", command_timeout, proc):
            record["status"] = "stall" if monitor.stalled() else "command-timeout"
            return record
        monitor.progress()

        ssh_ok = False
        processes = None
        if ssh and port is not None:
            if not serial.wait_text("svcd: sshd up", ssh_timeout, proc):
                record["ssh"] = "unavailable"
                record["ssh_detail"] = "sshd not up before ssh timeout"
            else:
                detail = "unavailable"
                for attempt in range(2):
                    ssh_ok, detail, ssh_s = ssh_run(
                        port,
                        "/bin/ps -e; /bin/uname -a; /bin/echo K509_SSH_OK",
                        iter_dir / f"ssh-{attempt + 1}.log",
                        command_timeout,
                    )
                    record["ssh_s"] = ssh_s
                    if ssh_ok:
                        ssh_log = iter_dir / f"ssh-{attempt + 1}.log"
                        ssh_text = ssh_log.read_text(errors="replace") if ssh_log.is_file() else ""
                        processes = count_posix_rows(ssh_text)
                        break
                    time.sleep(1)
                record["ssh"] = "ok" if ssh_ok else "unavailable"
                record["ssh_detail"] = detail

        if not ssh_ok:
            # Serial fallback. Only short commands: the release shell's line
            # editor drops characters from long pastes under load. Wait a fixed
            # slice for the short command to finish instead of matching the
            # prompt, which the line editor redraws before execution.
            for command in ("ps -e\n", "/bin/uname -a\n"):
                serial.send(command)
                time.sleep(2.0)
                monitor.progress()
            processes = count_posix_rows(serial.text)

        # The kernel prints `s24: frames free=` on each guest child exit; the
        # SSH or serial command above has just created one.
        time.sleep(0.5)
        record["processes"] = processes
        record["status"] = "pass"
    finally:
        text = serial.text
        record["frames_boot"], record["frames_first"], record["frames_last"] = frame_values(text)
        record["handles"] = handle_value(text)
        if record["processes"] is None:
            record["processes"] = count_posix_rows(text)
        record["forbidden"] = "; ".join(f"{name}: {line}" for name, line in scan_tokens(text))
        if record["status"] == "pass" and record["forbidden"]:
            record["status"] = "forbidden"
        record["tail"] = "\n".join(text.splitlines()[-40:])
        record["qemu_rss_kb"] = qemu_rss_kb(proc.pid)
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        serial.close()
        vars_copy.unlink(missing_ok=True)
        (iter_dir / "qemu.pid").unlink(missing_ok=True)
        record["wall_s"] = round(time.monotonic() - start, 3)
        record["end_utc"] = utc_now()
        record["host_after"] = host_sample()
    return record


def row_line(record: dict) -> str:
    hb = record["host_before"]
    ha = record["host_after"]
    return "\t".join(
        str(value)
        for value in (
            record["iteration"], record["start_utc"], record["end_utc"], record["status"],
            record["login_s"], record["wall_s"], record["ssh"], record["ssh_detail"],
            record["ssh_s"], record["frames_boot"], record["frames_first"],
            record["frames_last"], record["handles"], record["processes"],
            hb.get("load1"), hb.get("mem_available_kb"), hb.get("disk_free_kb"),
            ha.get("load1"), ha.get("mem_available_kb"), ha.get("disk_free_kb"),
            record["qemu_rss_kb"], record["serial"], record["forbidden"],
        )
    )


def iteration_header() -> str:
    return "\t".join(
        (
            "iteration", "start_utc", "end_utc", "status", "login_s", "wall_s",
            "ssh", "ssh_detail", "ssh_s", "frames_boot", "frames_first", "frames_last",
            "handles", "processes", "load1_before", "mem_available_before_kb",
            "disk_free_before_kb", "load1_after", "mem_available_after_kb",
            "disk_free_after_kb", "qemu_rss_kb", "serial", "forbidden",
        )
    )


def read_rows(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    rows = []
    lines = path.read_text(encoding="utf-8").splitlines()
    for line in lines[1:]:
        if not line.strip():
            continue
        fields = line.split("\t")
        rows.append({"iteration": int(fields[0]), "status": fields[3]})
    return rows


def read_tsv_rows(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        return []
    header = lines[0].split("\t")
    rows = []
    numeric = {
        "iteration", "login_s", "wall_s", "ssh_s", "frames_boot", "frames_first",
        "frames_last", "handles", "processes", "load1_before",
        "mem_available_before_kb", "disk_free_before_kb", "load1_after",
        "mem_available_after_kb", "disk_free_after_kb", "qemu_rss_kb",
    }
    for line in lines[1:]:
        if not line.strip():
            continue
        values = line.split("\t")
        row = dict(zip(header, values))
        for key in numeric:
            value = row.get(key, "")
            if value in ("", "None", "null"):
                row[key] = None
            else:
                try:
                    row[key] = float(value) if "." in value else int(value)
                except ValueError:
                    row[key] = None
        rows.append(row)
    return rows


def summary_from_rows(rows: list[dict], out: Path) -> dict:
    if not rows:
        return {"iterations": 0, "findings": ["no iterations completed"]}
    first = rows[0]
    last = rows[-1]
    wall = [row["wall_s"] for row in rows if row.get("wall_s") is not None]
    findings = []
    if any(row.get("status") not in {"pass", "boot-timeout"} for row in rows):
        findings.append("non-recovered iteration failure")
    before = first.get("mem_available_before_kb")
    after = last.get("mem_available_after_kb")
    if before is not None and after is not None and after < before - 524288:
        findings.append("host MemAvailable dropped by more than 512 MiB")
    return {
        "iterations": len(rows),
        "first_iteration": first.get("iteration"),
        "last_iteration": last.get("iteration"),
        "statuses": {status: sum(1 for row in rows if row.get("status") == status) for status in sorted({str(row.get("status")) for row in rows})},
        "wall_s_total": round(sum(wall), 3) if wall else 0,
        "wall_s_min": min(wall) if wall else None,
        "wall_s_max": max(wall) if wall else None,
        "mem_available_before_kb": before,
        "mem_available_after_kb": after,
        "disk_free_before_kb": first.get("disk_free_before_kb"),
        "disk_free_after_kb": last.get("disk_free_after_kb"),
        "findings": findings,
        "out": str(out),
    }


def monitor_selftest() -> int:
    monitor = Monitor(5.0)
    base = 100.0
    monitor.progress(base)
    monitor.progress(base + 2.0)
    monitor.progress(base + 4.0)
    if monitor.stalled(base + 8.0):
        print("k509: FAIL 4 s healthy gap reported as a stall", file=sys.stderr)
        return 1
    if not monitor.stalled(base + 10.0):
        print("k509: FAIL injected 6 s gap was not detected", file=sys.stderr)
        return 1
    print("k509: prove-stall PASS threshold_s=5.0 healthy_gap_s=4.0 injected_gap_s=6.0")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hours", type=float, default=None)
    parser.add_argument("--mode", choices=("tcg", "kvm2", "kvm4"), default="kvm4")
    parser.add_argument("--ssh", action="store_true")
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--boot-timeout", type=float, default=60.0)
    parser.add_argument("--login-timeout", type=float, default=60.0)
    parser.add_argument("--command-timeout", type=float, default=60.0)
    parser.add_argument("--stall", type=float, default=5.0)
    parser.add_argument("--ssh-timeout", type=float, default=20.0)
    parser.add_argument("--max-iterations", type=int, default=None)
    parser.add_argument("--kill-after", type=int, default=None)
    parser.add_argument("--resume", type=Path, default=None)
    parser.add_argument("--prove-stall", action="store_true")
    parser.add_argument("--prove-hung-boot", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.prove_stall:
        return monitor_selftest()
    if args.prove_hung_boot:
        args.hours = 0.05
        args.max_iterations = 2
        args.boot_timeout = 30.0
        args.ssh = False
        args.out = args.out or (ROOT / "build/k509/prove-hung-boot")
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        raise SystemExit(f"k509: image not found: {image} (build the release image first)")
    if not Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS)).is_file():
        raise SystemExit("k509: OVMF vars not found")

    if args.resume is not None:
        state_path = args.resume / "state.json" if args.resume.is_dir() else args.resume
        if not state_path.is_file():
            raise SystemExit(f"k509: state not found: {state_path}")
        state = json.loads(state_path.read_text(encoding="utf-8"))
        out = Path(state["out"])
        config = state["config"]
        image = Path(config["image"])
        if sha256_file(image) != config["image_sha256"]:
            raise SystemExit("k509: resume image sha256 mismatch")
        deadline = datetime.fromisoformat(state["deadline_utc"])
        next_iteration = state["next_iteration"]
        max_iterations = config["max_iterations"]
        print(f"k509: resume out={out} next_iteration={next_iteration} deadline={state['deadline_utc']}")
    else:
        if args.hours is None:
            raise SystemExit("k509: --hours is required (or use --resume)")
        out = (args.out or (ROOT / "build/k509")) / f"run-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
        out.mkdir(parents=True, exist_ok=True)
        start = datetime.now(timezone.utc)
        deadline = start + timedelta(hours=args.hours)
        next_iteration = 1
        max_iterations = args.max_iterations
        state = {
            "version": 1,
            "out": str(out),
            "start_utc": start.isoformat(),
            "deadline_utc": deadline.isoformat(),
            "config": {
                "hours": args.hours,
                "mode": args.mode,
                "ssh": args.ssh,
                "image": str(image),
                "image_sha256": sha256_file(image),
                "commit": git_output("rev-parse", "HEAD"),
                "boot_timeout": args.boot_timeout,
                "login_timeout": args.login_timeout,
                "command_timeout": args.command_timeout,
                "stall": args.stall,
                "ssh_timeout": args.ssh_timeout,
                "max_iterations": args.max_iterations,
                "hang_first": args.prove_hung_boot,
            },
            "next_iteration": 1,
            "current_iteration": None,
            "qemu_pid": None,
        }
        write_state(out / "state.json", state)
    config = state["config"]
    mode_name = config["mode"]
    ssh = config["ssh"]
    boot_timeout = config["boot_timeout"]
    login_timeout = config["login_timeout"]
    command_timeout = config["command_timeout"]
    stall = config["stall"]
    ssh_timeout = config["ssh_timeout"]
    hang_first = config.get("hang_first", False)
    rows_path = out / "iterations.tsv"
    rows = read_rows(rows_path)
    if rows:
        completed = max(row["iteration"] for row in rows)
        next_iteration = max(next_iteration, completed + 1)
    if not rows_path.is_file():
        rows_path.write_text(iteration_header() + "\n", encoding="utf-8")
    kill_stale_qemu(out, next_iteration, image)
    host_path = out / "host.tsv"
    if not host_path.is_file():
        host_path.write_text(
            "utc\titeration\tload1\tmem_available_kb\tmem_free_kb\tdisk_free_kb\tqemu\tjobs\n",
            encoding="utf-8",
        )
    print(
        f"k509: start out={out} existing={len(rows)} next={next_iteration} "
        f"deadline={deadline.isoformat()} image={image} ssh={ssh} mode={mode_name}"
    )
    failed = False
    ssh_runtime = ssh
    ssh_unavailable = 0
    completed = max((row["iteration"] for row in rows), default=0)
    try:
        while True:
            if max_iterations is not None and next_iteration > max_iterations:
                break
            if datetime.now(timezone.utc) >= deadline:
                break
            iteration = next_iteration
            state["current_iteration"] = iteration
            state["qemu_pid"] = None
            write_state(out / "state.json", state)
            hang = hang_first and iteration == 1 and completed == 0
            record = run_iteration(
                image, mode_name, iteration, out,
                3.0 if hang else boot_timeout, login_timeout,
                command_timeout, stall, ssh_runtime, ssh_timeout,
                hang,
            )
            if record["ssh"] == "unavailable":
                ssh_unavailable += 1
                if ssh_unavailable >= 3:
                    ssh_runtime = False
                    print("k509: SSH unavailable for 3 iterations; serial command traffic only")
            elif record["ssh"] == "ok":
                ssh_unavailable = 0
            with rows_path.open("a", encoding="utf-8") as handle:
                handle.write(row_line(record) + "\n")
                handle.flush()
                os.fsync(handle.fileno())
            with host_path.open("a", encoding="utf-8") as handle:
                hb = record["host_before"]
                ha = record["host_after"]
                handle.write(
                    f"{record['end_utc']}\t{iteration}\t{ha.get('load1')}\t"
                    f"{ha.get('mem_available_kb')}\t{ha.get('mem_free_kb')}\t"
                    f"{ha.get('disk_free_kb')}\t{ha.get('qemu')}\t{ha.get('jobs')}\n"
                )
                handle.flush()
                os.fsync(handle.fileno())
            rows.append(record)
            completed = iteration
            state["next_iteration"] = iteration + 1
            state["current_iteration"] = None
            state["qemu_pid"] = None
            write_state(out / "state.json", state)
            print(
                f"k509: iteration={iteration} status={record['status']} "
                f"login_s={record['login_s']} wall_s={record['wall_s']} "
                f"ssh={record['ssh']} frames={record['frames_boot']}/{record['frames_last']} "
                f"processes={record['processes']} load1={record['host_before'].get('load1')}"
            )
            if record["status"] == "forbidden":
                failed = True
                break
            if args.kill_after is not None and completed >= args.kill_after:
                print(f"k509: self-kill after {completed} completed iterations (resume proof)")
                os.kill(os.getpid(), signal.SIGKILL)
            next_iteration = iteration + 1
    finally:
        state["finished_utc"] = utc_now()
        state["completed"] = completed
        state["next_iteration"] = completed + 1
        state["current_iteration"] = None
        state["qemu_pid"] = None
        write_state(out / "state.json", state)
        summary = summary_from_rows(read_tsv_rows(rows_path), out)
        summary["config"] = config
        summary["commit"] = config.get("commit", "unknown")
        summary["start_utc"] = state["start_utc"]
        summary["finished_utc"] = utc_now()
        (out / "summary.json").write_text(
            json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"k509: latest_summary={out / 'summary.json'}")
    if args.prove_hung_boot:
        statuses = [row["status"] for row in rows]
        if statuses[:2] == ["boot-timeout", "pass"]:
            print("k509: prove-hung-boot PASS (boot-timeout detected and recovered)")
            return 0
        print(f"k509: prove-hung-boot FAIL statuses={statuses[:2]}", file=sys.stderr)
        return 1
    if failed:
        return 1
    if any(row["status"] not in {"pass"} for row in rows):
        print("k509: PARTIAL (a boot timeout/stall was recorded; see iterations.tsv)", file=sys.stderr)
        return 1
    print(f"k509: PASS iterations={len(rows)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
