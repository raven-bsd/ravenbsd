#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K434 aarch64 release net/sshd row with a real host client exchange.

Boots the aarch64 release image on QEMU `virt` with a virtio-mmio NIC, logs in on the console and
asserts `svcd: netd up`, `svcd: sshd up`, `Server listening on 0.0.0.0 port 22.`, and the console
marker. It then forwards a free host TCP port to the guest's port 22 and runs the host OpenSSH
client against it (banner exchange, password auth as root, one command and its exact output). The
serial is also checked for the pre-fix restore fault (`child killed`, `exception:`) so a service
banner alone cannot pass the row.

Usage:
  python3 tests/ssh/aarch64-net.py [--image build/qtest/release-aarch64-k434-r189.img]
"""

from __future__ import annotations

import argparse
import os
import shutil
import socket
import subprocess
import sys
from pathlib import Path

import pexpect

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tests/soak"))
import run as soak  # noqa: E402

DEFAULT_IMAGE = ROOT / "build/qtest/release-aarch64-k434-r189.img"
OUT = ROOT / "build/k434-aarch64-net"
ASKPASS = HERE / "askpass.sh"
MARKER = "DONE-42"
CLIENT_MARKER = "K434-CLIENT-OK"
CONSOLE_COMMAND = "cat /tmp/sshd.log 2>/dev/null; echo DONE-$((6*7))"
CLIENT_COMMAND = f"echo {CLIENT_MARKER}"


def free_port() -> int:
    """Reserve and release an ephemeral host port for the QEMU hostfwd mapping."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_listener(host: str, port: int, timeout: float) -> bool:
    """Wait for the guest sshd port to accept TCP; bounded, no test retry."""
    import time

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return True
        except OSError:
            time.sleep(0.5)
    return False


def ssh_client(host: str, port: int, timeout: float) -> tuple[bool, bool, str]:
    """Run one host OpenSSH session: banner, password auth, command, exact output.

    Returns `(ok, banner, detail)`. `banner` is the remote protocol line seen by the client; the
    detail is a short failure class for the report, never a retry.
    """
    log = OUT / "client.log"
    argv = [
        "ssh",
        "-v",
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
        "ConnectTimeout=10",
        "-o",
        "NumberOfPasswordPrompts=1",
        f"root@{host}",
        CLIENT_COMMAND,
    ]
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = "raven"
    try:
        completed = subprocess.run(
            argv,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
            start_new_session=True,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        stdout = (error.stdout or "") if isinstance(error.stdout, str) else ""
        stderr = (error.stderr or "") if isinstance(error.stderr, str) else ""
        log.write_text(f"argv={argv!r}\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}\n", encoding="utf-8")
        return False, False, "client-timeout"
    log.write_text(
        f"argv={argv!r}\nreturncode={completed.returncode}\nSTDOUT:\n{completed.stdout}\n"
        f"STDERR:\n{completed.stderr}\n",
        encoding="utf-8",
    )
    banner = "Remote protocol version 2.0" in completed.stderr
    if completed.returncode != 0:
        if "Connection refused" in completed.stderr:
            return False, banner, "connection-refused"
        if "Permission denied" in completed.stderr:
            return False, banner, "auth-refused"
        return False, banner, f"client-exit-{completed.returncode}"
    if CLIENT_MARKER not in completed.stdout:
        return False, banner, "missing-command-output"
    if not banner:
        return False, banner, "missing-banner"
    return True, banner, "ok"


def host_echo(host: str, port: int = 23097, timeout: float = 10.0) -> tuple[bool, str]:
    """Send one exact payload to the guest echo service and require the same bytes back."""
    try:
        with socket.create_connection((host, port), timeout=timeout) as stream:
            stream.settimeout(timeout)
            stream.sendall(b"PING\n")
            data = b""
            while len(data) < 5:
                chunk = stream.recv(5 - len(data))
                if not chunk:
                    break
                data += chunk
            return data == b"PING\n", f"got={data!r}"
    except OSError as error:
        return False, f"err={error}"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument("--timeout", type=float, default=180.0)
    parser.add_argument("--tap", default="", help="tap interface for the real client exchange")
    parser.add_argument("--host", default="10.0.2.15", help="guest IP for the tap client")
    args = parser.parse_args(argv)
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        print(f"aarch64-net: missing image {image}", flush=True)
        return 2
    OUT.mkdir(parents=True, exist_ok=True)
    vars_source = os.environ.get("RAVEN_AAVMF_VARS", soak.DEFAULT_AAVMF_VARS)
    vars_path = OUT / ".vars.fd"
    shutil.copyfile(vars_source, vars_path)
    serial = OUT / "serial.log"
    client_port = free_port()
    command = soak.qemu_argv("aarch64", soak.MODES["tcg"], image, vars_path)
    # QEMU `virt` defaults the virtio-mmio transports to legacy and the first `-device` lands on
    # the last bus; pin the modern transport on bus 0 (0x0a000000, SPI 16/INTID 48) to match the
    # staged device tree. The user-net path forwards a free host port to the guest sshd; the tap
    # path (run inside the private netns wrapper) uses the DHCP guest address directly.
    if args.tap:
        command += [
            "-global",
            "virtio-mmio.force-legacy=false",
            "-device",
            "virtio-net-device,netdev=n0,bus=virtio-mmio-bus.0",
            "-netdev",
            f"tap,id=n0,ifname={args.tap},script=no,downscript=no",
        ]
        client_host = args.host
        client_port = 22
    else:
        command += [
            "-global",
            "virtio-mmio.force-legacy=false",
            "-device",
            "virtio-net-device,netdev=n0,bus=virtio-mmio-bus.0",
            "-netdev",
            f"user,id=n0,hostfwd=tcp:127.0.0.1:{client_port}-:22",
        ]
        client_host = "127.0.0.1"
    console_ok = False
    client_ok = False
    client_banner = False
    client_detail = "not-run"
    child = pexpect.spawn(
        command[0], command[1:], encoding="utf-8", codec_errors="replace", timeout=args.timeout
    )
    with serial.open("w", encoding="utf-8") as log:
        child.logfile = log
        try:
            child.expect("login:", timeout=180)
            child.sendline("root")
            child.expect("Password:", timeout=30)
            child.sendline("raven")
            child.expect("# ", timeout=60)
            child.sendline(CONSOLE_COMMAND)
            child.expect(MARKER, timeout=120)
            console_ok = True
            # Accepted-socket data path without sshd: the release-echo image runs the tiny echo
            # service on 23097; the host sends PING and requires the exact bytes back.
            echo_ok = False
            echo_detail = "not-run"
            if wait_listener(client_host, 23097, 30):
                echo_ok, echo_detail = host_echo(client_host)
            else:
                echo_detail = "echo-port-never-listened"
            # Primary row: the host client exchange. Capture it before any guest-side diagnostic
            # so a diagnostic that wedges the console cannot hide the client result.
            if wait_listener(client_host, client_port, 30):
                client_ok, client_banner, client_detail = ssh_client(client_host, client_port, 40)
            else:
                client_detail = "forwarded-port-never-listened"
            # Guest child probes: prove ordinary spawned children work, then capture the
            # hanging /bin/ssh -V child's K391 state and last syscall with proc-probe.
            guest_note = "not-run"
            # The svcd-spawned proc-probe scanner (release image) prints the state/last syscall
            # of syscall-blocked or ssh processes. The shell foreground ssh is the hanging child;
            # the same shell-spawn hang blocks running the probe from this shell.
            try:
                child.sendline("/bin/ssh -V 2>&1; echo SSHTOOL-$?")
                child.expect(r"SSHTOOL-[0-9]+", timeout=20)
                guest_note = "ssh-tool-ok"
            except (pexpect.TIMEOUT, pexpect.EOF):
                guest_note = "ssh-foreground-hung"
        except (pexpect.EOF, pexpect.TIMEOUT) as error:
            print(f"aarch64-net: console wait {type(error).__name__}", file=sys.stderr)
            print((child.before or "")[-1200:], file=sys.stderr)
        finally:
            child.close(force=True)
    vars_path.unlink(missing_ok=True)
    text = serial.read_text(encoding="utf-8", errors="replace")
    netd = "svcd: netd up" in text
    sshd = "svcd: sshd up" in text
    listening = "Server listening on 0.0.0.0 port 22." in text
    nic = "netd: ready" in text
    faults = [
        line.strip()
        for line in text.splitlines()
        if "child killed" in line or line.lstrip().startswith("exception:")
    ]
    loop_banner = "Remote protocol version 2.0" in text
    print(
        f"aarch64-net: netd_up={int(netd)} netd_ready={int(nic)} sshd_up={int(sshd)} "
        f"listening={int(listening)} marker={int(console_ok)} client_ok={int(client_ok)} "
        f"banner={int(client_banner)} loop_banner={int(loop_banner)} client={client_detail} "
        f"echo_ok={int(echo_ok)} echo={echo_detail} faults={len(faults)} guest={guest_note} "
        f"transport={'tap' if args.tap else 'user'}"
    )
    for line in text.splitlines():
        if any(token in line for token in ("Remote protocol version", "banner exchange", "Permission denied")):
            print("loop:", line.strip()[-220:])
    for line in faults[:4]:
        print(f"serial-fault: {line[-220:]}")
    for line in text.splitlines():
        if any(
            token in line
            for token in ("svcd: netd", "svcd: sshd", "netd: ready", "nic:info", "Server listening")
        ):
            print("serial:", line.strip()[-200:])
    for line in text.splitlines():
        if (
            line.startswith("proc-probe:")
            or line.startswith("proc-scan:")
            or line.startswith("SSHTOOL-")
        ):
            print("probe:", line.strip()[-220:])
    traces = [line.strip() for line in text.splitlines() if line.startswith("systrace:")]
    for line in traces[-8:]:
        print("systrace:", line)
    if client_detail != "ok":
        client_log = OUT / "client.log"
        if client_log.is_file():
            print("client-log-tail:", client_log.read_text(errors="replace")[-1200:])
    ok = (
        console_ok
        and netd
        and sshd
        and listening
        and client_ok
        and client_banner
        and not faults
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
