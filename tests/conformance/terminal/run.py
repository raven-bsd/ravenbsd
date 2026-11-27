#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S175 terminal conformance runner (test-only).

Builds /bin/terminal-conformance into a release initfs with a one-shot service, boots the image,
and reads the probe rows from the serial console. The four gate rows (termios, pty pair, controlling
terminal, EIO) and the three signal rows must PASS. The service's second invocation
(`--mutation-termios`) must turn the termios row red, and the runner removes the signal rows from a
copy of the normal text to prove the named rows are required. The public header record and request
encodings are checked by a host clang oracle with the generated include directory first. No product
code changes and no SSH: the probe does not depend on the ssh-hostkeys/tmpfs ordering.

Build separately when iterating (each step has its own cap):
  sh tests/conformance/terminal/build.sh
  cargo xtask initfs --arch x86_64 --profile release
  target/debug/raven-b70-overlay build/x86_64/initfs.img build/conformance/terminal/initfs.img \
      /bin/terminal-conformance=$PWD/build/conformance/terminal/terminal-conformance:0755 \
      /etc/services.d/terminal-conformance.toml=$PWD/tests/conformance/terminal/terminal-conformance.service.toml:0644
  cargo xtask image --arch x86_64 --profile release --initfs build/conformance/terminal/initfs.img \
      --cmdline "console=ttyS0 test-exit=1" --output build/qtest/s175-terminal.img
  python3 tests/conformance/terminal/run.py --no-build

The runner selects the guest architecture (`--arch x86_64|aarch64`) and the accelerator
(`--accel auto|kvm|tcg`): KVM is used only when the host matches the guest and `/dev/kvm` is
readable, otherwise QEMU runs under TCG. Each architecture builds and reads its own image and
output directory; aarch64 derives its build, firmware and console from the existing aarch64 qtests.
"""

from __future__ import annotations

import os
import platform
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SERVICE = ROOT / "tests/conformance/terminal/terminal-conformance.service.toml"
PROBE = ROOT / "build/conformance/terminal/terminal-conformance"
GATE_ROWS = ("termios", "pty-both-ways", "ctty", "eio", "target-vintr")
SIGNAL_ROWS = ("sigwinch", "sighup", "isig")
MUTATION_MARKER = "S175-TERM-MUTATION-BEGIN"

ARCHES = {
    "x86_64": {
        "qemu": "qemu-system-x86_64",
        "console": "ttyS0",
        "firmware": ("/usr/share/OVMF/OVMF_CODE_4M.fd", "/usr/share/OVMF/OVMF_VARS_4M.fd"),
    },
    "aarch64": {
        "qemu": "qemu-system-aarch64",
        "console": "ttyAMA0",
        "firmware": ("/usr/share/AAVMF/AAVMF_CODE.fd", "/usr/share/AAVMF/AAVMF_VARS.fd"),
    },
}

HEADER_PROBE = r"""
#include <stddef.h>
#include <termios.h>
#include <sys/ioctl.h>

_Static_assert(sizeof(struct termios) == 44, "pinned FreeBSD termios size");
_Static_assert(_Alignof(struct termios) == 4, "pinned FreeBSD termios alignment");
_Static_assert(offsetof(struct termios, c_cc) == 16, "c_cc offset");
_Static_assert(offsetof(struct termios, c_ispeed) == 36, "c_ispeed offset");
_Static_assert(offsetof(struct termios, c_ospeed) == 40, "c_ospeed offset");
_Static_assert(NCCS == 20, "NCCS");
_Static_assert(
    VEOF == 0 && VINTR == 8 && VMIN == 16 && VTIME == 17 && VERASE2 == 7,
    "BSD control-character indexes");
_Static_assert(TIOCGETA == 0x402c7413UL, "TIOCGETA");
_Static_assert(TIOCSETA == 0x802c7414UL, "TIOCSETA");
_Static_assert(TIOCSETAW == 0x802c7415UL, "TIOCSETAW");
_Static_assert(TIOCSETAF == 0x802c7416UL, "TIOCSETAF");
_Static_assert(TIOCGWINSZ == 0x40087468UL, "TIOCGWINSZ");
_Static_assert(TIOCSWINSZ == 0x80087467UL, "TIOCSWINSZ");
_Static_assert(TIOCGPGRP == 0x40047477UL, "TIOCGPGRP");
_Static_assert(TIOCSPGRP == 0x80047476UL, "TIOCSPGRP");
_Static_assert(TIOCSCTTY == 0x20007461UL, "TIOCSCTTY");
_Static_assert(FIONBIO == 0x8004667eUL, "FIONBIO");
_Static_assert(FIOCLEX == 0x20006601UL, "FIOCLEX");
_Static_assert(FIONCLEX == 0x20006602UL, "FIONCLEX");

int main(void) { return 0; }
"""


def step(out: Path, label: str, argv: list[str], timeout: int) -> None:
    print(f"S175-TERM-RUN {label}", flush=True)
    result = subprocess.run(argv, cwd=str(ROOT), capture_output=True, timeout=timeout)
    (out / f"{label}.log").write_bytes(result.stdout + result.stderr)
    if result.returncode != 0:
        raise SystemExit(f"S175-TERM-RUN {label} failed rc={result.returncode}; see {out}/{label}.log")


def header_oracle(arch: str, out: Path) -> None:
    """Compile the FreeBSD terminal ABI expectations against the public headers."""
    out.mkdir(parents=True, exist_ok=True)
    header_oracle = out / "header-oracle.c"
    header_oracle.write_text(HEADER_PROBE, encoding="utf-8")
    clang = shutil.which("clang")
    if clang is None:
        raise SystemExit("S175-TERM-RUN header-oracle failed: clang not found")
    resource = subprocess.run(
        [clang, "-print-resource-dir"], capture_output=True, text=True, check=True
    ).stdout.strip()
    step(
        out,
        "header-oracle",
        [
            clang,
            f"--target={arch}-unknown-ravenbsd",
            "-nostdinc",
            "-isystem",
            f"{resource}/include",
            "-isystem",
            str(ROOT / "libc/ravenlibc/abi/generated/include"),
            "-isystem",
            str(ROOT / f"libc/ravenlibc/arch/{arch}/include"),
            "-isystem",
            str(ROOT / "libc/ravenlibc/include"),
            "-D__BSD_VISIBLE=1",
            "-std=gnu11",
            "-c",
            str(header_oracle),
            "-o",
            str(out / "header-oracle.o"),
        ],
        60,
    )


def normal_failures(text: str) -> list[str]:
    found = rows(text)
    failures: list[str] = []
    for name in GATE_ROWS + SIGNAL_ROWS:
        if found.get(name) != "PASS":
            failures.append(f"normal:{name}={found.get(name)}")
    if "S175-TERM done failures=" not in text:
        failures.append("normal:incomplete-output")
    elif "S175-TERM done failures=0" not in text:
        failures.append("normal:done-not-zero")
    return failures


def build_image(arch: str, image: Path, out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    step(out, "fixture-build", ["sh", "tests/conformance/terminal/build.sh", arch], 240)
    shutil.rmtree(ROOT / f"build/cargo/uutils-{arch}", ignore_errors=True)
    # The terminal probe needs the initfs services, not brush. The libc-test profile stages the
    # same pty/vfs/console servers without the shell and keeps the terminal image ABI-consistent.
    step(out, "initfs-build",
         ["cargo", "xtask", "initfs", "--arch", arch, "--profile", "libc-test"], 600)
    initfs = out / "terminal-initfs.img"
    step(
        out,
        "overlay",
        [
            "target/debug/raven-b70-overlay",
            f"build/{arch}/initfs.img",
            str(initfs),
            f"/bin/terminal-conformance={PROBE}:0755",
            f"/etc/services.d/terminal-conformance.toml={SERVICE}:0644",
        ],
        120,
    )
    step(
        out,
        "image-build",
        [
            "cargo", "xtask", "image", "--arch", arch, "--profile", "libc-test",
            "--initfs", str(initfs),
            "--cmdline", f"console={ARCHES[arch]['console']} test-exit=1",
            "--output", str(image),
        ],
        900,
    )


def rows(output: str) -> dict[str, str]:
    found: dict[str, str] = {}
    for line in output.splitlines():
        if not line.startswith("S175-TERM row="):
            continue
        parts = line.split()
        name = parts[1].split("=", 1)[1]
        status = parts[2].split("=", 1)[1]
        found[name] = status
    return found


def parse_args(argv: list[str]) -> tuple[str, str, bool]:
    arch = os.environ.get("S175_TERM_ARCH", "x86_64")
    accel = os.environ.get("S175_TERM_ACCEL", "auto")
    no_build = False
    rest = argv[1:]
    index = 0
    while index < len(rest):
        argument = rest[index]
        if argument == "--no-build":
            no_build = True
        elif argument in ("--arch", "--accel"):
            if index + 1 >= len(rest):
                raise SystemExit(f"S175-TERM-RUN {argument} needs a value")
            if argument == "--arch":
                arch = rest[index + 1]
            else:
                accel = rest[index + 1]
            index += 1
        else:
            raise SystemExit(f"S175-TERM-RUN unknown argument {argument}")
        index += 1
    if arch not in ARCHES:
        raise SystemExit(f"S175-TERM-RUN unknown arch {arch}; use {', '.join(ARCHES)}")
    if accel not in ("auto", "kvm", "tcg"):
        raise SystemExit(f"S175-TERM-RUN unknown accelerator {accel}")
    return arch, accel, no_build


def accelerator(arch: str, requested: str) -> str:
    """Pick KVM only when the host can run this guest with it, TCG otherwise."""
    if requested != "auto":
        return requested
    if arch == platform.machine() and Path("/dev/kvm").is_file() and os.access("/dev/kvm", os.R_OK | os.W_OK):
        return "kvm"
    return "tcg"


def qemu_argv(arch: str, accel: str, image: Path, out: Path, serial: Path) -> list[str]:
    profile = ARCHES[arch]
    code = Path(profile["firmware"][0])
    vars_template = Path(profile["firmware"][1])
    for firmware in (code, vars_template):
        if not firmware.is_file():
            raise SystemExit(f"S175-TERM-RUN firmware missing: {firmware}")
    vars_path = out / "vars.fd"
    shutil.copyfile(vars_template, vars_path)
    argv = [str(profile["qemu"])]
    if arch == "x86_64":
        argv += ["-machine", "q35", "-m", "512M", "-smp", "4"]
    else:
        argv += ["-M", "virt,gic-version=2", "-m", "512M"]
    if accel == "kvm":
        argv += ["-enable-kvm", "-cpu", "host"]
    elif arch == "x86_64":
        argv += ["-cpu", "max"]
    else:
        argv += ["-cpu", "cortex-a72"]
    argv += [
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
    ]
    if arch == "x86_64":
        argv += ["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"]
    argv += ["-nic", "none"]
    return argv


QEMU_EXIT_OK = (0, 33)  # 33 is `isa-debug-exit` success (guest exit code 0).
QEMU_COMPLETION_GRACE = 5.0


def qemu_exit_failure(returncode: int | None) -> str | None:
    """Return a failure label for a QEMU child exit, or None for the expected sentinel."""
    if returncode is None:
        return "qemu:running"
    if returncode in QEMU_EXIT_OK:
        return None
    return f"qemu:exit={returncode}"


def terminate_qemu(qemu: subprocess.Popen[bytes]) -> None:
    """Terminate the child explicitly and reap it, escalating to SIGKILL."""
    qemu.terminate()
    try:
        qemu.wait(timeout=1)
    except subprocess.TimeoutExpired:
        qemu.kill()
        qemu.wait()


def read_serial(serial: Path, fallback: str) -> str:
    """Read the serial file, keeping the last captured text only if the file is empty."""
    final = serial.read_text(encoding="utf-8", errors="replace")
    return final or fallback


def collect_serial(
    serial: Path,
    qemu: subprocess.Popen[bytes],
    deadline: float,
    *,
    completion_grace: float = QEMU_COMPLETION_GRACE,
    on_capture=None,
) -> tuple[str, str | None]:
    """Read the serial log until both runs finish or the deadline expires.

    A child can exit between the poll and the next capture read, so the serial is read once more
    after it exits. When both completion markers were seen before the deadline the child gets a
    bounded completion grace and the serial is read again after that exit, so bytes written while
    the child shuts down are kept. When the deadline expires first the child is terminated at once
    and the result is an explicit deadline failure: late output written during cleanup is never
    accepted, and the deadline is never extended.
    """
    text = ""
    complete = False
    while time.monotonic() < deadline:
        if qemu.poll() is not None:
            break
        text = serial.read_text(encoding="utf-8", errors="replace")
        if on_capture is not None:
            on_capture(text)
        if MUTATION_MARKER in text and text.count("S175-TERM done failures=") >= 2:
            complete = True
            break
        time.sleep(0.2)

    if not complete and qemu.poll() is None:
        terminate_qemu(qemu)
        return read_serial(serial, text), "serial:deadline"

    if complete and qemu.poll() is None:
        try:
            qemu.wait(timeout=completion_grace)
        except subprocess.TimeoutExpired:
            terminate_qemu(qemu)
            return read_serial(serial, text), "qemu:terminated"

    return read_serial(serial, text), qemu_exit_failure(qemu.returncode)


def mutation_incomplete(text: str) -> bool:
    """Whether the mutation run is missing its completion marker."""
    if MUTATION_MARKER not in text:
        return True
    _, mutation = text.split(MUTATION_MARKER, 1)
    return "S175-TERM done failures=" not in mutation


def main() -> int:
    arch, accel, no_build = parse_args(sys.argv)
    image = Path(os.environ.get("S175_TERM_IMAGE", ROOT / f"build/qtest/s175-terminal-{arch}.img"))
    out = Path(os.environ.get("S175_TERM_OUT", ROOT / f"build/conformance/terminal/run-{arch}"))
    serial = out / "serial.log"
    header_oracle(arch, out)
    if not no_build or not image.is_file():
        build_image(arch, image, out)
    else:
        print(f"S175-TERM-RUN image reuse {image}", flush=True)
    chosen = accelerator(arch, accel)
    print(f"S175-TERM-RUN arch={arch} accel={chosen} image={image}", flush=True)

    out.mkdir(parents=True, exist_ok=True)
    serial.write_bytes(b"")
    argv = qemu_argv(arch, chosen, image, out, serial)
    qemu = subprocess.Popen(argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL,
                            stderr=open(out / "qemu.stderr", "wb"), start_new_session=True)
    failures: list[str] = []
    try:
        deadline = time.monotonic() + (300 if arch == "aarch64" else 180)
        text, exit_failure = collect_serial(serial, qemu, deadline)
        (out / "serial-copy.log").write_text(text, encoding="utf-8")
        if exit_failure:
            failures.append(exit_failure)

        if MUTATION_MARKER in text:
            normal, mutation = text.split(MUTATION_MARKER, 1)
        else:
            normal, mutation = text, ""
        normal_rows = rows(normal)
        mutation_rows = rows(mutation)
        failures.extend(normal_failures(normal))
        if mutation_rows.get("termios") != "FAIL":
            failures.append(f"mutation:termios={mutation_rows.get('termios')}")
        if mutation_incomplete(text):
            failures.append("mutation:incomplete-output")

        # Test-only mutation: remove the signal rows from a copy of the normal output. The
        # checker must reject it, proving SIGWINCH/SIGHUP/ISIG are named, required rows.
        mutated = "\n".join(
            line for line in normal.splitlines()
            if not any(line.startswith(f"S175-TERM row={name} ") for name in SIGNAL_ROWS)
        )
        mutated_failures = normal_failures(mutated)
        missing = [
            name for name in SIGNAL_ROWS
            if not any(f"normal:{name}=" in failure for failure in mutated_failures)
        ]
        (out / "signal-mutation.log").write_text(
            mutated + "\n" + "\n".join(mutated_failures) + "\n", encoding="utf-8"
        )
        if missing:
            failures.append("signal-mutation:harness accepted missing " + ",".join(missing))

        print(normal, end="", flush=True)
        print(mutation, end="", flush=True)
        print(f"S175-TERM-RUN rows_normal={normal_rows} rows_mutation={mutation_rows} "
              f"signal_mutation_failures={mutated_failures}", flush=True)
        if failures:
            print(f"S175-TERM-RUN FAIL reasons={';'.join(failures)}", flush=True)
            return 1
        print("S175-TERM-RUN PASS", flush=True)
        return 0
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()


if __name__ == "__main__":
    raise SystemExit(main())
