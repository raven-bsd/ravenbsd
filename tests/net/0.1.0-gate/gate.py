#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""S85n 0.1.0 network and TCP gate.

Boot the release profile image with a test-only overlay that starts one guest
TCP probe as a service. The probe waits for the static virtio-net interface,
listens on the guest NIC address, and echoes a known payload back to the host
client connected through QEMU user-network hostfwd. The gate asserts the exact
bytes in both directions and the guest exit status.

The overlay adds only the probe, its service manifest and a test-only
`/etc/svcd.conf`; every product program, driver and manifest still comes from
the release profile. No kernel, netd or profile behaviour is changed or
reimplemented here.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent

PROBE_PORT = 23085
PAYLOAD = b"S85N-NET-GATE-PAYLOAD"
ACK = b"S85N-NET-GATE-ACK"

PANIC = "panic"
BOOT = "boot"
INTERFACE = "interface"
LISTENER = "listener"
CONNECT = "connect"
ECHO = "echo"
SESSION = "session"
SETUP = "setup"
PROBE = "probe_build"
OVERLAY = "overlay_build"
INITFS = "initfs_build"
IMAGE = "image_build"

MUTATIONS = ("none", "closed-port", "drop-payload")

EXIT_STAGE = {
    SETUP: 2,
    PROBE: 3,
    OVERLAY: 4,
    INITFS: 5,
    IMAGE: 6,
    BOOT: 10,
    INTERFACE: 11,
    LISTENER: 12,
    CONNECT: 13,
    ECHO: 14,
    SESSION: 15,
    PANIC: 16,
}

PROBE_BUILD_TIMEOUT = 180
OVERLAY_BUILD_TIMEOUT = 120
ADDON_TIMEOUT = 300
INITFS_TIMEOUT = 300
IMAGE_TIMEOUT = 300
BOOT_TIMEOUT = 300
HOST_TIMEOUT = 30
EXIT_TIMEOUT = 120

QEMU_SUCCESS = 33


class GateFailure(Exception):
    """One failed gate stage with its exact detail."""

    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(detail)
        self.stage = stage
        self.detail = detail


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="S85n 0.1.0 network and TCP gate",
        allow_abbrev=False,
    )
    parser.add_argument("--arch", default="x86_64", choices=["x86_64"])
    parser.add_argument("--accel", default="kvm", choices=["kvm", "tcg"])
    parser.add_argument("--mutation", default="none", choices=list(MUTATIONS))
    parser.add_argument("--evidence-dir", default=os.environ.get("RAVEN_EVIDENCE_DIR"))
    return parser.parse_args(argv)


def mutation_guest_port(mutation: str, probe_port: int) -> int:
    """Guest port the host-forward targets; the closed-port mutation moves it."""
    if mutation == "none" or mutation == "drop-payload":
        return probe_port
    if mutation == "closed-port":
        return probe_port + 1
    raise ValueError(f"unknown mutation `{mutation}`")


def mutation_payload(mutation: str, payload: bytes) -> bytes:
    """Payload the host sends; the drop mutation removes the last byte."""
    if mutation == "none" or mutation == "closed-port":
        return payload
    if mutation == "drop-payload":
        return payload[:-1]
    raise ValueError(f"unknown mutation `{mutation}`")


def echo_matches(payload: bytes, echo: bytes) -> bool:
    """The guest must return exactly the bytes the host sent."""
    return echo == payload


def qemu_argv(
    *,
    arch: str,
    accel: str,
    image: Path,
    firmware_code: Path,
    firmware_vars: Path,
    serial: Path,
    host_port: int,
    guest_port: int,
) -> list[str]:
    if arch != "x86_64":
        raise ValueError(f"unsupported architecture `{arch}`")
    argv = [
        "qemu-system-x86_64",
        "-machine",
        "q35",
        "-m",
        "512M",
        "-smp",
        "4",
        "-display",
        "none",
        "-monitor",
        "none",
        "-no-reboot",
        "-serial",
        f"file:{serial}",
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={firmware_code}",
        "-drive",
        f"if=pflash,format=raw,file={firmware_vars}",
        "-drive",
        f"format=raw,file={image}",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
    ]
    if accel == "kvm":
        argv += ["-enable-kvm", "-cpu", "host"]
    else:
        argv += ["-cpu", "max"]
    argv += [
        "-nic",
        "none",
        "-netdev",
        f"user,id=n0,hostfwd=tcp:127.0.0.1:{host_port}-:{guest_port}",
        "-device",
        "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    return argv


def classify(*, serial: str, host: str, qemu_exit: int) -> str | None:
    """Return the exact failing stage, or None when the round trip passed."""
    if re.search(r"(?i)kernel panic|\bpanic\b|EXCEPTION", serial):
        return PANIC
    if "net-gate: interface eth0 10.0.2.15/24 up" not in serial:
        return INTERFACE
    if f"net-gate: listening tcp 10.0.2.15:{PROBE_PORT}" not in serial:
        return LISTENER
    if "net-gate-host: connect failed" in host or "net-gate-host: echo read failed" in host:
        return CONNECT
    expected = (
        f"net-gate-host: echo received {len(PAYLOAD)} bytes payload="
        + PAYLOAD.decode("ascii")
    )
    if expected not in host or "net-gate-host: echo mismatch" in host:
        return ECHO
    if f"net-gate: echo received {len(PAYLOAD)} bytes payload=" + PAYLOAD.decode(
        "ascii"
    ) not in serial:
        return ECHO
    if "net-gate: done" not in serial or "net-gate-host: done" not in host:
        return SESSION
    if qemu_exit != QEMU_SUCCESS:
        return SESSION
    return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def log(message: str) -> None:
    print(f"s85n: {message}", flush=True)


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", errors="replace")


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def run_command(
    argv: list[str],
    *,
    timeout: int,
    log_path: Path,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            argv,
            cwd=ROOT,
            capture_output=True,
            env=env,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as expired:
        output = (expired.stdout or "") + (expired.stderr or "")
        write_text(
            log_path,
            f"$ {' '.join(argv)}\n{output}\ntimeout after {timeout}s\n",
        )
        raise
    write_text(
        log_path,
        f"$ {' '.join(argv)}\n{result.stdout}{result.stderr}\nexit={result.returncode}\n",
    )
    return result


def build_probe(evidence: Path) -> Path:
    manifest = HERE / "probe" / "Cargo.toml"
    target_dir = ROOT / "build" / "cargo" / "net-gate-probe"
    env = dict(os.environ)
    env["RUSTFLAGS"] = (
        "-C code-model=small -C relocation-model=static -C lto=no "
        f"-C link-arg=-T{ROOT / 'userland' / 'linker' / 'x86_64.ld'}"
    )
    argv = [
        "cargo",
        "build",
        "--manifest-path",
        str(manifest),
        "-p",
        "raven-net-gate-probe",
        "--target",
        "x86_64-unknown-none",
        "--release",
        "--target-dir",
        str(target_dir),
    ]
    try:
        result = run_command(
            argv,
            timeout=PROBE_BUILD_TIMEOUT,
            log_path=evidence / "probe-build.log",
            env=env,
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(PROBE, "guest probe build timed out")
    finally:
        (HERE / "probe" / "Cargo.lock").unlink(missing_ok=True)
    if result.returncode != 0:
        raise GateFailure(PROBE, "guest probe build failed; see probe-build.log")
    binary = target_dir / "x86_64-unknown-none" / "release" / "raven-net-gate-probe"
    if not binary.is_file():
        raise GateFailure(PROBE, f"guest probe binary missing: {binary}")
    return binary


def build_overlay(evidence: Path) -> Path:
    manifest = HERE / "overlay" / "Cargo.toml"
    target_dir = ROOT / "build" / "cargo" / "net-gate-overlay"
    argv = [
        "cargo",
        "build",
        "--manifest-path",
        str(manifest),
        "-p",
        "raven-net-gate-overlay",
        "--target-dir",
        str(target_dir),
    ]
    try:
        result = run_command(
            argv, timeout=OVERLAY_BUILD_TIMEOUT, log_path=evidence / "overlay-build.log"
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(OVERLAY, "initfs overlay tool build timed out")
    finally:
        (HERE / "overlay" / "Cargo.lock").unlink(missing_ok=True)
    if result.returncode != 0:
        raise GateFailure(OVERLAY, "initfs overlay tool build failed; see overlay-build.log")
    binary = target_dir / "debug" / "raven-net-gate-overlay"
    if not binary.is_file():
        raise GateFailure(OVERLAY, f"initfs overlay tool missing: {binary}")
    return binary


def build_initfs(evidence: Path) -> Path:
    argv = ["cargo", "xtask", "initfs", "--arch", "x86_64", "--profile", "release"]
    try:
        result = run_command(
            argv, timeout=INITFS_TIMEOUT, log_path=evidence / "initfs-build.log"
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(INITFS, "release initfs build timed out")
    if result.returncode != 0:
        raise GateFailure(INITFS, "release initfs build failed; see initfs-build.log")
    initfs = ROOT / "build" / "x86_64" / "initfs.img"
    if not initfs.is_file():
        raise GateFailure(INITFS, f"release initfs missing: {initfs}")
    return initfs


def overlay_initfs(
    overlay: Path, base: Path, probe: Path, evidence: Path
) -> Path:
    output = ROOT / "build" / "net-gate" / "initfs.img"
    output.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        str(overlay),
        str(base),
        str(output),
        f"/sbin/net-gate={probe}:0o755",
        f"/etc/services.d/net-gate.toml={HERE / 'net-gate.service.manifest'}",
        f"/etc/svcd.conf={HERE / 'svcd.conf'}",
    ]
    try:
        result = run_command(
            argv, timeout=60, log_path=evidence / "overlay.log"
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(OVERLAY, "initfs overlay timed out")
    if result.returncode != 0:
        raise GateFailure(OVERLAY, f"initfs overlay failed: {result.stderr.strip()}")
    if not output.is_file():
        raise GateFailure(OVERLAY, f"overlaid initfs missing: {output}")
    return output


def ensure_ssh_addon(evidence: Path) -> None:
    required = [
        ROOT / "build" / "ssh" / "x86_64" / "bin" / "sshd",
        ROOT / "build" / "ssh" / "x86_64" / "libexec" / "sshd-session",
        ROOT / "build" / "ssh" / "x86_64" / "libexec" / "sshd-auth",
    ]
    if all(path.is_file() for path in required):
        return
    log("release image needs the OpenSSH add-on; building it")
    try:
        result = run_command(
            ["sh", "addons/ssh/build.sh", "--arch", "x86_64"],
            timeout=ADDON_TIMEOUT,
            log_path=evidence / "addon-build.log",
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(IMAGE, "OpenSSH add-on build timed out")
    if result.returncode != 0:
        raise GateFailure(IMAGE, "OpenSSH add-on build failed; see addon-build.log")


def build_image(initfs: Path, evidence: Path) -> Path:
    ensure_ssh_addon(evidence)
    image = ROOT / "build" / "net-gate" / "release.img"
    image.parent.mkdir(parents=True, exist_ok=True)
    argv = [
        "cargo",
        "xtask",
        "image",
        "--arch",
        "x86_64",
        "--profile",
        "release",
        "--cmdline",
        "console=ttyS0 loglevel=info test-exit=1",
        "--initfs",
        str(initfs.relative_to(ROOT)),
        "--output",
        str(image.relative_to(ROOT)),
    ]
    try:
        result = run_command(
            argv, timeout=IMAGE_TIMEOUT, log_path=evidence / "image-build.log"
        )
    except subprocess.TimeoutExpired:
        raise GateFailure(IMAGE, "release image build timed out")
    stale = result.returncode != 0 and (
        "stale addon binary" in result.stdout or "stale addon binary" in result.stderr
    )
    if stale:
        log("staged OpenSSH binaries are stale; rebuilding the add-on once")
        (ROOT / "build" / "ssh" / "x86_64" / "bin" / "sshd").unlink(missing_ok=True)
        ensure_ssh_addon(evidence)
        try:
            result = run_command(
                argv, timeout=IMAGE_TIMEOUT, log_path=evidence / "image-build.log"
            )
        except subprocess.TimeoutExpired:
            raise GateFailure(IMAGE, "release image rebuild timed out")
    if result.returncode != 0:
        raise GateFailure(IMAGE, "release image build failed; see image-build.log")
    if not image.is_file():
        raise GateFailure(IMAGE, f"release image missing: {image}")
    return image


def firmware(evidence: Path, tag: str) -> tuple[Path, Path]:
    code = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
    var = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
    for path in (code, var):
        if not path.is_file():
            raise GateFailure(SETUP, f"firmware image not found: {path}")
    vars_copy = evidence / f"firmware-vars-{tag}.fd"
    shutil.copyfile(var, vars_copy)
    return code, vars_copy


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def recv_exact(sock: socket.socket, wanted: int) -> bytes:
    chunks: list[bytes] = []
    total = 0
    while total < wanted:
        chunk = sock.recv(wanted - total)
        if not chunk:
            raise ConnectionError(f"peer closed after {total} of {wanted} bytes")
        chunks.append(chunk)
        total += len(chunk)
    return b"".join(chunks)


def host_client(
    *, host_port: int, mutation: str, log_path: Path
) -> str:
    """Run the host side and return its log text."""
    lines = [f"net-gate-host: connecting 127.0.0.1:{host_port}"]
    payload = mutation_payload(mutation, PAYLOAD)
    deadline = time.monotonic() + HOST_TIMEOUT
    sock: socket.socket | None = None
    while True:
        try:
            sock = socket.create_connection(("127.0.0.1", host_port), timeout=2)
            break
        except OSError as error:
            if time.monotonic() >= deadline:
                lines.append(f"net-gate-host: connect failed: {error}")
                write_text(log_path, "\n".join(lines) + "\n")
                return "\n".join(lines)
            time.sleep(0.2)
    lines.append(f"net-gate-host: connected 127.0.0.1:{host_port}")
    sock.settimeout(HOST_TIMEOUT)
    try:
        sock.sendall(payload)
        lines.append(f"net-gate-host: sent {len(payload)} bytes")
        if mutation == "drop-payload":
            sock.shutdown(socket.SHUT_WR)
        else:
            try:
                echo = recv_exact(sock, len(PAYLOAD))
            except (OSError, ConnectionError) as error:
                lines.append(f"net-gate-host: echo read failed: {error}")
            else:
                lines.append(
                    f"net-gate-host: echo received {len(echo)} bytes payload="
                    + echo.decode("ascii", "replace")
                )
                if not echo_matches(PAYLOAD, echo):
                    lines.append("net-gate-host: echo mismatch")
                else:
                    sock.sendall(ACK)
                    lines.append(
                        f"net-gate-host: ack sent {ACK.decode('ascii')}"
                    )
                    sock.shutdown(socket.SHUT_WR)
                    lines.append("net-gate-host: done")
    except OSError as error:
        lines.append(f"net-gate-host: echo read failed: {error}")
    finally:
        sock.close()
    write_text(log_path, "\n".join(lines) + "\n")
    return "\n".join(lines)


def wait_serial_marker(
    serial: Path, marker: str, process: subprocess.Popen[bytes], timeout: int
) -> str:
    deadline = time.monotonic() + timeout
    while True:
        text = read_text(serial)
        if marker in text:
            return text
        if re.search(r"(?i)kernel panic|\bpanic\b|EXCEPTION", text):
            return text
        if process.poll() is not None:
            return text
        if time.monotonic() >= deadline:
            return text
        time.sleep(0.2)


def wait_exit(process: subprocess.Popen[bytes], timeout: int) -> int | None:
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return None


def run_cell(
    *, arch: str, accel: str, mutation: str, evidence: Path
) -> tuple[str | None, str]:
    """Build, boot and verify one cell; return (failing stage, detail)."""
    tag = f"{arch}-{accel}" + ("" if mutation == "none" else f"-{mutation}")
    serial = evidence / f"serial-{tag}.log"
    host_log = evidence / f"host-{tag}.log"
    qemu_log = evidence / f"qemu-{tag}.log"

    probe = build_probe(evidence)
    overlay = build_overlay(evidence)
    base = build_initfs(evidence)
    initfs = overlay_initfs(overlay, base, probe, evidence)
    image = build_image(initfs, evidence)
    code, vars_copy = firmware(evidence, tag)

    host_port = free_port()
    guest_port = mutation_guest_port(mutation, PROBE_PORT)
    argv = qemu_argv(
        arch=arch,
        accel=accel,
        image=image,
        firmware_code=code,
        firmware_vars=vars_copy,
        serial=serial,
        host_port=host_port,
        guest_port=guest_port,
    )
    log(f"booting {tag}: {image.relative_to(ROOT)} on host port {host_port}")
    with qemu_log.open("w", encoding="utf-8", errors="replace") as logfile:
        process = subprocess.Popen(
            argv,
            cwd=ROOT,
            stdout=logfile,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
        )
    try:
        serial_text = wait_serial_marker(
            serial, f"net-gate: listening tcp 10.0.2.15:{PROBE_PORT}", process, BOOT_TIMEOUT
        )
        if not serial_text:
            raise GateFailure(BOOT, f"no serial output; see {serial.name}")
        if re.search(r"(?i)kernel panic|\bpanic\b|EXCEPTION", serial_text):
            raise GateFailure(PANIC, f"guest faulted; see {serial.name}")
        if "net-gate: interface eth0 10.0.2.15/24 up" not in serial_text:
            raise GateFailure(
                INTERFACE,
                f"release guest did not report the eth0 10.0.2.15/24 interface; see {serial.name}",
            )
        if f"net-gate: listening tcp 10.0.2.15:{PROBE_PORT}" not in serial_text:
            raise GateFailure(
                LISTENER,
                f"guest probe did not listen on 10.0.2.15:{PROBE_PORT}; see {serial.name}",
            )

        host_text = host_client(
            host_port=host_port, mutation=mutation, log_path=host_log
        )
        qemu_exit = wait_exit(process, EXIT_TIMEOUT)
        if qemu_exit is None:
            process.kill()
            qemu_exit = process.wait()
            serial_text = read_text(serial)
            stage = classify(serial=serial_text, host=host_text, qemu_exit=qemu_exit)
            raise GateFailure(
                stage or SESSION,
                f"guest did not exit within {EXIT_TIMEOUT}s; see {serial.name}",
            )
        serial_text = read_text(serial)
        stage = classify(serial=serial_text, host=host_text, qemu_exit=qemu_exit)
        if stage is not None:
            raise GateFailure(
                stage,
                f"stage {stage} failed; see {serial.name} and {host_log.name}",
            )
        return None, "pass"
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        summary = {
            "ticket": "S85n",
            "arch": arch,
            "accel": accel,
            "mutation": mutation,
            "guest_port": guest_port,
            "host_port": host_port,
            "payload": PAYLOAD.decode("ascii"),
            "ack": ACK.decode("ascii"),
            "serial": serial.name,
            "host_log": host_log.name,
            "image_sha256": sha256(image),
            "probe_sha256": sha256(probe),
            "initfs_sha256": sha256(initfs),
            "serial_tail": read_text(serial)[-4000:],
            "host_log_text": read_text(host_log),
        }
        write_text(
            evidence / f"summary-{tag}.json",
            json.dumps(summary, indent=2, sort_keys=True) + "\n",
        )


def main(argv: list[str]) -> int:
    options = parse_args(argv)
    if not options.evidence_dir:
        print("s85n: --evidence-dir is required", file=sys.stderr)
        return EXIT_STAGE[SETUP]
    evidence = Path(options.evidence_dir)
    evidence.mkdir(parents=True, exist_ok=True)
    stage: str | None = None
    detail = ""
    started = time.monotonic()
    try:
        stage, detail = run_cell(
            arch=options.arch,
            accel=options.accel,
            mutation=options.mutation,
            evidence=evidence,
        )
    except GateFailure as failure:
        stage = failure.stage
        detail = failure.detail
    except subprocess.TimeoutExpired as expired:
        stage = IMAGE if "image" in str(expired.cmd) else BOOT
        detail = f"timeout after {expired.timeout}s: {expired.cmd}"
    elapsed = round(time.monotonic() - started, 3)
    status = "PASS" if stage is None else f"FAIL stage={stage}"
    log(f"{status} {options.arch}/{options.accel} mutation={options.mutation} elapsed={elapsed}s")
    if detail:
        log(detail)
    return 0 if stage is None else EXIT_STAGE[stage]


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
