#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""S85k 0.1.0 SSH login gate.

Boot the release profile image in QEMU, prove that the guest sshd is listening,
log in with the host OpenSSH client, run a marker command and a PTY command, and
assert a clean session close. Every failure is reported with the exact stage and
with the serial and client logs preserved under the prjmgmt evidence root.

The gate never uses the console shell as an SSH result: the console is only used
to prove whether the release image carries sshd, sshd-session and sshd-auth.
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

import pexpect

ROOT = Path(__file__).resolve().parents[3]

MARKER = "S85K-SSH-LOGIN-OK"

BOOT = "boot"
IMAGE = "image"
SSHD_MISSING = "sshd_missing"
SSHD_INCOMPLETE = "sshd_incomplete"
SSHD_NOT_LISTENING = "sshd_not_listening"
HANDSHAKE = "sshd_handshake"
AUTH = "authentication"
PTY = "pty"
SESSION = "session"
SETUP = "setup"

PROBE_PRESENT = "present"
PROBE_ABSENT = "absent"
PROBE_INCOMPLETE = "incomplete"
PROBE_NOT_LISTENING = "not_listening"
PROBE_UNKNOWN = "unknown"

BOOT_TIMEOUT = 300
LISTEN_TIMEOUT = 90
CLIENT_TIMEOUT = 60
IMAGE_TIMEOUT = 300
ADDON_TIMEOUT = 540
FIXTURE_TIMEOUT = 300

EXIT_STAGE = {
    SETUP: 2,
    IMAGE: 3,
    BOOT: 10,
    SSHD_MISSING: 12,
    SSHD_INCOMPLETE: 13,
    SSHD_NOT_LISTENING: 14,
    HANDSHAKE: 15,
    AUTH: 16,
    PTY: 17,
    SESSION: 18,
}


def marker_present(stdout: str) -> bool:
    """The marker must be the whole client output, never a console transcript."""
    return stdout.strip() == MARKER


def sshd_state_from_serial(text: str) -> str:
    """Read the svcd boot report for the sshd service row.

    The release boot prints one row per service (`<time>  sshd  ok`). The row is
    present only when the release profile stages the service; `ok` is svcd's own
    readiness (the listening line in /tmp/sshd.log), so it is not a console guess.
    """
    row = None
    for line in text.splitlines():
        match = re.match(r"^\s*\d+\.\d+\s+sshd\s+(\S+)", line)
        if match:
            row = match.group(1)
    if row is None:
        return PROBE_ABSENT
    if "ok" in row:
        return PROBE_PRESENT
    return PROBE_NOT_LISTENING


def clean_close(stderr: str) -> bool:
    """OpenSSH -v reports the remote status and the freed session channel on a clean close."""
    return "Exit status 0" in stderr and "channel 0: free: client-session" in stderr


def classify(
    *,
    serial: str,
    sshd_probe: str,
    client_rc: int | None,
    client_stdout: str,
    client_stderr: str,
) -> str | None:
    """Return the exact failing stage, or None when the SSH login passed."""
    del serial
    if sshd_probe == PROBE_ABSENT:
        return SSHD_MISSING
    if sshd_probe == PROBE_INCOMPLETE:
        return SSHD_INCOMPLETE
    if client_rc is None:
        return SESSION
    if "PTY allocation request failed" in client_stderr:
        return PTY
    if client_rc != 0:
        if re.search(r"(?i)permission denied|authentication failed", client_stderr):
            return AUTH
        if re.search(
            r"(?i)host key verification failed|kex_exchange_identification|"
            r"unable to negotiate|no matching host key|connection closed by",
            client_stderr,
        ):
            return HANDSHAKE
        if re.search(
            r"(?i)connection refused|connection timed out|no route to host|"
            r"connection reset by peer",
            client_stderr,
        ):
            return SSHD_NOT_LISTENING
        return SESSION
    if not marker_present(client_stdout):
        return SESSION
    if not clean_close(client_stderr):
        return SESSION
    return None


def client_argv(
    *,
    ssh: str,
    port: int,
    user: str,
    known_hosts: str,
    command: str,
    allocate_pty: bool,
) -> list[str]:
    argv = [
        ssh,
        "-v",
        "-p",
        str(port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "GlobalKnownHostsFile=/dev/null",
        "-o",
        f"UserKnownHostsFile={known_hosts}",
        "-o",
        "PreferredAuthentications=password",
        "-o",
        "PubkeyAuthentication=no",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        "ConnectTimeout=15",
    ]
    if allocate_pty:
        argv.append("-tt")
    argv.extend([f"{user}@127.0.0.1", command])
    return argv


def log(message: str) -> None:
    print(f"s85k: {message}", flush=True)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", errors="replace")


def build_addon(arch: str, evidence: Path) -> None:
    log(f"building the OpenSSH add-on for {arch}")
    log_path = evidence / f"addon-build-{arch}.log"
    result = subprocess.run(
        ["sh", "addons/ssh/build.sh", "--arch", arch],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=ADDON_TIMEOUT,
        check=False,
    )
    write_text(log_path, result.stdout + result.stderr)
    if result.returncode != 0:
        raise GateFailure(
            IMAGE,
            f"OpenSSH add-on build failed with exit {result.returncode}; see {log_path}",
        )


def run_image_build(arch: str, image: Path, log_path: Path) -> tuple[int, str]:
    argv = [
        "cargo",
        "xtask",
        "image",
        "--arch",
        arch,
        "--profile",
        "release",
        "--output",
        str(image.relative_to(ROOT)),
    ]
    proc = subprocess.Popen(
        argv,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        output, _ = proc.communicate(timeout=IMAGE_TIMEOUT)
    except subprocess.TimeoutExpired:
        proc.kill()
        output, _ = proc.communicate()
        write_text(log_path, output)
        raise GateFailure(
            IMAGE,
            f"release image build exceeded {IMAGE_TIMEOUT}s; see {log_path}",
        )
    write_text(log_path, output)
    return proc.returncode or 0, output


def profile_fixtures() -> list[str]:
    """Arch-selected fixture names staged by the release profile and its shell parent."""
    names: set[str] = set()
    candidates = [ROOT / "profiles" / "release.toml", ROOT / "profiles" / "shell.toml"]
    for directory in (ROOT / "profiles" / "release.d", ROOT / "profiles" / "shell.d"):
        candidates.extend(sorted(directory.glob("*.toml")))
    for path in candidates:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            continue
        names.update(re.findall(r"build/fixtures/([A-Za-z0-9._-]+)/\{arch\}", text))
    return sorted(names)


def stage_fixtures(arch: str, evidence: Path) -> None:
    """Stage the arch-selected fixture inputs before the image build.

    The fixture scripts take the target arch, but xtask's fixture hook runs them
    without it and stages x86_64 files into a cross-arch build. Run each documented
    `tests/fixtures/<name>/build.sh <arch>` from the resolved profile first.
    """
    for name in profile_fixtures():
        staged = ROOT / "build" / "fixtures" / name / arch
        if staged.is_dir():
            continue
        script = ROOT / "tests" / "fixtures" / name / "build.sh"
        if not script.is_file():
            raise GateFailure(
                IMAGE, f"the release profile stages fixture {name} but {script} is missing"
            )
        log(f"staging fixture {name} for {arch}")
        log_path = evidence / f"fixtures-{arch}-{name}.log"
        result = subprocess.run(
            ["sh", str(script), arch],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=FIXTURE_TIMEOUT,
            check=False,
        )
        write_text(log_path, result.stdout + result.stderr)
        if result.returncode != 0:
            raise GateFailure(
                IMAGE,
                f"fixture {name} staging failed with exit {result.returncode}; see {log_path}",
            )


def build_image(arch: str, evidence: Path) -> Path:
    image = ROOT / "build" / "qtest" / f"s85k-ssh-login-{arch}.img"
    addon = ROOT / "build" / "ssh" / arch / "bin" / "sshd"
    if not addon.is_file():
        build_addon(arch, evidence)
    stage_fixtures(arch, evidence)
    log(f"building release image {image.relative_to(ROOT)}")
    log(f"image build RUSTUP_HOME={os.environ.get('RUSTUP_HOME', '<unset>')}")
    log_path = evidence / f"image-build-{arch}.log"
    code, output = run_image_build(arch, image, log_path)
    stale = code != 0 and "stale addon binary" in output
    if stale:
        log("staged OpenSSH binaries are stale; rebuilding the add-on once")
        build_addon(arch, evidence)
        code, output = run_image_build(arch, image, log_path)
    if code != 0:
        raise GateFailure(
            IMAGE,
            f"release image build failed with exit {code}; see {log_path}",
        )
    return image


class GateFailure(Exception):
    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(detail)
        self.stage = stage
        self.detail = detail


def firmware(arch: str) -> tuple[Path, Path]:
    names = {
        "x86_64": ("RAVEN_OVMF_CODE", "RAVEN_OVMF_VARS",
                   "/usr/share/OVMF/OVMF_CODE_4M.fd", "/usr/share/OVMF/OVMF_VARS_4M.fd"),
        "aarch64": ("RAVEN_AAVMF_CODE", "RAVEN_AAVMF_VARS",
                    "/usr/share/AAVMF/AAVMF_CODE.fd", "/usr/share/AAVMF/AAVMF_VARS.fd"),
    }
    env_code, env_vars, default_code, default_vars = names[arch]
    code = Path(os.environ.get(env_code, default_code))
    var = Path(os.environ.get(env_vars, default_vars))
    for path in (code, var):
        if not path.is_file():
            raise GateFailure(SETUP, f"firmware image not found: {path}")
    return code, var


def qemu_argv(arch: str, accel: str, image: Path, code: Path, var: Path, port: int) -> list[str]:
    net = [
        "-nic", "none",
        "-netdev", f"user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    if arch == "x86_64":
        argv = [
            "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
            "-serial", "stdio", "-display", "none", "-monitor", "none", "-no-reboot",
        ]
        argv += ["-enable-kvm", "-cpu", "host"] if accel == "kvm" else ["-cpu", "max"]
        argv += [
            "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,format=raw,file={var}",
            "-drive", f"format=raw,file={image}",
            "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        ]
        return argv + net
    if accel != "tcg":
        raise GateFailure(SETUP, "aarch64 supports only --accel tcg on this host")
    return [
        "qemu-system-aarch64", "-M", "virt,gic-version=2", "-cpu", "cortex-a72",
        "-m", "512M", "-nographic", "-semihosting-config", "enable=on,target=native",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={var}",
        "-drive", f"format=raw,file={image}",
    ] + net


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def banner(port: int, deadline: float) -> str | None:
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=2) as sock:
                sock.settimeout(3)
                data = sock.recv(256)
                if data.startswith(b"SSH-"):
                    return data.decode("utf-8", "replace").strip()
                return None
        except OSError:
            time.sleep(0.5)
    return None


def sshd_ready(text: str) -> bool:
    """svcd may print `svcd: sshd up` right after the getty login prompt on the same line."""
    return re.search(r"svcd: sshd up\s*$", text, re.MULTILINE) is not None


def wait_sshd(child: pexpect.spawn, initial: str) -> tuple[str, str]:
    """Drain the serial until svcd reports `sshd up`, then return the state and text."""
    text = initial
    state = sshd_state_from_serial(text)
    if state != PROBE_PRESENT:
        return state, text
    deadline = time.monotonic() + LISTEN_TIMEOUT
    while True:
        if sshd_ready(text):
            return PROBE_PRESENT, text
        if time.monotonic() >= deadline:
            return PROBE_NOT_LISTENING, text
        try:
            text += child.read_nonblocking(size=4096, timeout=0.5)
        except pexpect.TIMEOUT:
            continue
        except pexpect.EOF:
            return PROBE_NOT_LISTENING, text


def run_client(
    *, argv: list[str], env: dict[str, str], log_path: Path, timeout: int
) -> tuple[int, str, str]:
    result = subprocess.run(
        argv,
        cwd=ROOT,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    write_text(
        log_path,
        "$ " + " ".join(argv) + "\n\nSTDOUT\n" + result.stdout + "\nSTDERR\n" + result.stderr,
    )
    return result.returncode, result.stdout, result.stderr


def serial_tail(path: Path, limit: int = 4000) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")[-limit:]
    except OSError:
        return ""


def gate(arch: str, accel: str, image: Path, evidence: Path, password: str) -> int:
    tag = f"{arch}-{accel}"
    serial = evidence / f"serial-{tag}.log"
    client_exec = evidence / f"client-exec-{tag}.log"
    client_pty = evidence / f"client-pty-{tag}.log"
    summary = evidence / f"summary-{tag}.json"
    code, var_source = firmware(arch)
    vars_copy = evidence / f"firmware-vars-{tag}.fd"
    shutil.copyfile(var_source, vars_copy)
    port = free_port()
    child: pexpect.spawn | None = None
    logfile = None
    sshd_probe = PROBE_UNKNOWN
    banner_text = ""
    stage: str | None = None
    detail = ""
    exec_rc = -1
    marker_stdout = ""
    marker_stderr = ""
    pty_rc = -1
    pty_stdout = ""
    pty_stderr = ""
    started = time.monotonic()
    try:
        log(f"booting {arch}/{accel} image {image.relative_to(ROOT)} on port {port}")
        argv = qemu_argv(arch, accel, image, code, vars_copy, port)
        logfile = serial.open("w", encoding="utf-8", errors="replace")
        child = pexpect.spawn(
            argv[0],
            argv[1:],
            cwd=str(ROOT),
            encoding="utf-8",
            codec_errors="replace",
            timeout=BOOT_TIMEOUT,
            logfile=logfile,
        )
        child.setecho(False)
        try:
            child.expect([r"(?i)login:\s*", r"(?i)panic|EXCEPTION|Kernel panic"], timeout=BOOT_TIMEOUT)
        except pexpect.TIMEOUT:
            raise GateFailure(BOOT, f"release image did not reach the login prompt; see {serial}")
        except pexpect.EOF:
            raise GateFailure(
                BOOT, f"QEMU exited before the login prompt; see {serial} and the run log"
            )
        if re.search(r"(?i)panic|EXCEPTION|Kernel panic", child.after):
            raise GateFailure(BOOT, f"guest faulted before login; see {serial}")

        logfile.flush()
        initial = child.before + child.after
        sshd_probe, _ = wait_sshd(child, initial)
        log(f"guest sshd state: {sshd_probe}")
        if sshd_probe == PROBE_ABSENT:
            raise GateFailure(
                SSHD_MISSING,
                "the release boot report has no sshd service row; the release profile "
                "does not stage the OpenSSH add-on (owner: K195)",
            )
        if sshd_probe == PROBE_NOT_LISTENING:
            raise GateFailure(
                SSHD_NOT_LISTENING,
                f"the release boot report has an sshd row but no 'svcd: sshd up'; see {serial}",
            )

        banner_text = banner(port, time.monotonic() + LISTEN_TIMEOUT) or ""
        if not banner_text:
            raise GateFailure(
                SSHD_NOT_LISTENING,
                f"sshd binary is staged but no SSH banner reached 127.0.0.1:{port}; "
                f"see {serial} and /tmp/sshd.log",
            )
        log(f"sshd banner: {banner_text}")

        known_hosts = evidence / f"known-hosts-{tag}"
        env = dict(os.environ)
        env.update(
            {
                "SSH_ASKPASS": str(ROOT / "tests" / "ssh" / "0.1.0-login-gate" / "askpass.sh"),
                "SSH_ASKPASS_REQUIRE": "force",
                "DISPLAY": ":0",
                "RAVEN_SSH_PASSWORD": password,
            }
        )
        ssh = "/usr/bin/ssh"
        exec_argv = client_argv(
            ssh=ssh, port=port, user="raven", known_hosts=str(known_hosts),
            command="echo " + MARKER, allocate_pty=False,
        )
        rc, stdout, stderr = run_client(
            argv=exec_argv, env=env, log_path=client_exec, timeout=CLIENT_TIMEOUT
        )
        marker_stdout, marker_stderr = stdout, stderr
        exec_rc = rc
        stage = classify(
            serial=serial_tail(serial), sshd_probe=sshd_probe,
            client_rc=rc, client_stdout=stdout, client_stderr=stderr,
        )
        if stage is not None:
            raise GateFailure(stage, f"host OpenSSH exec row failed; see {client_exec}")

        pty_argv = client_argv(
            ssh=ssh, port=port, user="raven", known_hosts=str(known_hosts),
            command="pwd", allocate_pty=True,
        )
        pty_rc, pty_stdout, pty_stderr = run_client(
            argv=pty_argv, env=env, log_path=client_pty, timeout=CLIENT_TIMEOUT
        )
        if "PTY allocation request failed" in pty_stderr:
            raise GateFailure(PTY, f"sshd refused the PTY request; see {client_pty}")
        if pty_rc != 0 or not re.search(r"(?m)^/", pty_stdout):
            raise GateFailure(PTY, f"PTY command did not return a path; see {client_pty}")
        if not clean_close(pty_stderr):
            raise GateFailure(SESSION, f"PTY session did not close cleanly; see {client_pty}")
        return 0
    except GateFailure as failure:
        stage, detail = failure.stage, failure.detail
        return EXIT_STAGE.get(stage, 1)
    except pexpect.TIMEOUT:
        stage, detail = BOOT, f"pexpect timeout; see {serial}"
        return EXIT_STAGE[BOOT]
    finally:
        if child is not None:
            try:
                child.close(force=True)
            except Exception:
                pass
        if logfile is not None:
            logfile.close()
        elapsed = round(time.monotonic() - started, 3)
        status = "PASS" if stage is None else f"FAIL stage={stage}"
        record = {
            "ticket": "S85k",
            "arch": arch,
            "accel": accel,
            "image": str(image.relative_to(ROOT)),
            "image_sha256": sha256(image) if image.is_file() else "",
            "result": "pass" if stage is None else "fail",
            "stage": stage,
            "detail": detail,
            "sshd_probe": sshd_probe,
            "banner": banner_text,
            "marker": MARKER,
            "elapsed_seconds": elapsed,
            "serial_tail": serial_tail(serial, 2000),
            "client_exec": {"rc": exec_rc, "stdout": marker_stdout,
                            "stderr": marker_stderr},
            "client_pty": {"rc": pty_rc, "stdout": pty_stdout, "stderr": pty_stderr},
        }
        write_text(summary, json.dumps(record, indent=2, sort_keys=True) + "\n")
        log(f"{status} arch={arch} accel={accel} elapsed={elapsed}s")
        log(f"summary: {summary}")
        if stage is not None:
            log(f"detail: {detail}")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="S85k 0.1.0 SSH login gate")
    parser.add_argument("--arch", choices=("x86_64", "aarch64"), required=True)
    parser.add_argument("--accel", choices=("kvm", "tcg"), required=True)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--password", default="raven")
    args = parser.parse_args(argv)
    evidence = args.evidence_dir.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    image = args.image.resolve() if args.image else None
    try:
        if image is None:
            image = build_image(args.arch, evidence)
        elif not image.is_file():
            raise GateFailure(IMAGE, f"image not found: {image}")
    except GateFailure as failure:
        log(f"FAIL stage={failure.stage}")
        log(f"detail: {failure.detail}")
        return EXIT_STAGE.get(failure.stage, 1)
    except subprocess.TimeoutExpired:
        log(f"FAIL stage={IMAGE}")
        log(f"detail: release image build exceeded {IMAGE_TIMEOUT}s")
        return EXIT_STAGE[IMAGE]
    return gate(args.arch, args.accel, image, evidence, args.password)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
