#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S179 credential and limit conformance on the release image.

Boots the release image once, logs in as the unprivileged `raven` account over SSH, runs
`/bin/cred-probe`, reads `/etc/passwd` and `/etc/group`, and asserts the ticket's gate rows:

  * real/effective uid/gid equal the `raven` passwd entry and `getgroups` equals the group
    memberships from `/etc/group`;
  * a non-permitted `setuid`/`setgid` fails with EPERM;
  * `umask(027)` makes a 0666 create 0640 and `umask(077)` makes it 0600;
  * `getrlimit`/`setrlimit` round-trip and an unprivileged hard-limit raise fails with EPERM;
  * `RLIMIT_NOFILE` returns EMFILE at the soft limit;
  * `getrusage(RUSAGE_SELF)` grows over three samples and `ru_maxrss` is at least the mapped size.

`--mutation` runs the probe with `--mutation=ignore-hard-limit`: the probe reports the hard raise
as accepted, and this harness must turn the row red. That is the negative check; the product tree
is not changed.

Usage:
  python3 tests/conformance/credentials/run.py --image build/conformance/credentials/credentials.img
  python3 tests/conformance/credentials/run.py --image ... --mutation
  python3 tests/conformance/credentials/run.py --selftest

Exit: 0 all rows green, 1 a row/style check failed, 2 setup error.
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
MAP_BYTES = 4 * 1024 * 1024
EMFILE = 24

KV_RE = re.compile(r"([A-Za-z0-9_-]+)=([^\s]+)")


def parse_rows(text: str) -> dict[str, dict[str, str]]:
    """Parse the probe's `cred: <row> key=value ...` lines."""
    rows: dict[str, dict[str, str]] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("cred: "):
            continue
        body = line[len("cred: ") :]
        if not body:
            continue
        head, _, rest = body.partition(" ")
        fields = {key: value for key, value in KV_RE.findall(rest)}
        if head in rows and head != "FAIL":
            # The probe prints one row per name except FAIL; a duplicate is an ambiguity.
            rows[f"{head}#dup"] = fields
        rows[head] = fields
    return rows


def parse_passwd(text: str) -> dict[str, dict[str, object]]:
    """Parse `/etc/passwd` into name -> uid/gid/home/shell."""
    entries: dict[str, dict[str, object]] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split(":")
        if len(fields) != 7:
            continue
        name, _password, uid, gid, _gecos, home, shell = fields
        if uid.isdigit() and gid.isdigit():
            entries[name] = {
                "uid": int(uid),
                "gid": int(gid),
                "home": home,
                "shell": shell,
            }
    return entries


def parse_group(text: str) -> dict[str, dict[str, object]]:
    """Parse `/etc/group` into name -> gid/members."""
    entries: dict[str, dict[str, object]] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split(":")
        if len(fields) != 4:
            continue
        name, _password, gid, members = fields
        if gid.isdigit():
            entries[name] = {
                "gid": int(gid),
                "members": [member for member in members.split(",") if member],
            }
    return entries


def _int(row: dict[str, str] | None, key: str, failures: list[str], name: str) -> int | None:
    if row is None or key not in row:
        failures.append(f"{name}: missing {key}")
        return None
    try:
        return int(row[key], 0)
    except ValueError:
        failures.append(f"{name}: {key}={row[key]!r} is not an integer")
        return None


def _octal(row: dict[str, str] | None, key: str, failures: list[str], name: str) -> int | None:
    if row is None or key not in row:
        failures.append(f"{name}: missing {key}")
        return None
    try:
        return int(row[key], 8)
    except ValueError:
        failures.append(f"{name}: {key}={row[key]!r} is not octal")
        return None


def check_rows(
    probe_text: str,
    passwd_text: str,
    group_text: str,
    umask_exec_text: str,
) -> list[str]:
    """Return the failed assertions with the measured values."""
    failures: list[str] = []
    rows = parse_rows(probe_text)
    passwd = parse_passwd(passwd_text)
    groups = parse_group(group_text)

    raven = passwd.get("raven")
    if raven is None:
        failures.append("passwd: no raven entry")
        return failures

    expected_groups = sorted(
        {int(raven["gid"])}
        | {
            int(entry["gid"])
            for entry in groups.values()
            if "raven" in entry["members"]  # type: ignore[operator]
        }
    )

    ids = rows.get("ids")
    for key, want in (
        ("uid", int(raven["uid"])),
        ("euid", int(raven["uid"])),
        ("gid", int(raven["gid"])),
        ("egid", int(raven["gid"])),
    ):
        got = _int(ids, key, failures, "ids")
        if got is not None and got != want:
            failures.append(f"ids: {key}={got}, passwd wants {want}")

    group_row = rows.get("groups")
    count = _int(group_row, "count", failures, "groups")
    got_count = _int(group_row, "got", failures, "groups")
    listed: list[int] = []
    if group_row is not None and "list" in group_row:
        try:
            listed = sorted(int(value) for value in group_row["list"].split(",") if value)
        except ValueError:
            failures.append(f"groups: list={group_row['list']!r} is not a gid list")
    if count is not None and count != len(expected_groups):
        failures.append(f"groups: count={count}, `/etc/group` wants {len(expected_groups)}")
    if got_count is not None and got_count != len(expected_groups):
        failures.append(f"groups: got={got_count}, `/etc/group` wants {len(expected_groups)}")
    if listed != expected_groups:
        failures.append(f"groups: list={listed}, `/etc/group` wants {expected_groups}")

    for name in ("setuid-nonpermitted", "setgid-nonpermitted"):
        row = rows.get(name)
        if _int(row, "ret", failures, name) != -1:
            failures.append(f"{name}: must fail")
        if _int(row, "errno", failures, name) != 1:
            failures.append(f"{name}: expected EPERM (1)")

    old = _octal(rows.get("umask-old"), "mask", failures, "umask-old")
    if old is not None and old != 0o022:
        failures.append(f"umask-old: {old:04o}, expected 0022")
    mode = _octal(rows.get("umask-file"), "mode", failures, "umask-file")
    if mode is not None and mode != 0o640:
        failures.append(f"umask-file: {mode:04o}, expected 0640")
    returned = _octal(rows.get("umask-return"), "mask", failures, "umask-return")
    if returned is not None and returned != 0o027:
        failures.append(f"umask-return: {returned:04o}, expected 0027")
    mode = _octal(rows.get("umask-file-077"), "mode", failures, "umask-file-077")
    if mode is not None and mode != 0o600:
        failures.append(f"umask-file-077: {mode:04o}, expected 0600")

    kernel_limit = rows.get("kernel-rlimit-nofile")
    cur = _int(kernel_limit, "cur", failures, "kernel-rlimit-nofile")
    hard = _int(kernel_limit, "max", failures, "kernel-rlimit-nofile")
    if cur is not None and hard is not None and cur > hard:
        failures.append(f"kernel-rlimit-nofile: cur={cur} > max={hard}")
    for name in ("kernel-rlimit-roundtrip", "kernel-rlimit-soft-raise", "kernel-rlimit-restore"):
        if _int(rows.get(name), "errno", failures, name) != 0:
            failures.append(f"{name}: expected 0")
    if _int(rows.get("kernel-rlimit-hard-raise"), "errno", failures,
            "kernel-rlimit-hard-raise") != 1:
        failures.append("kernel-rlimit-hard-raise: expected EPERM (1)")
    raised_state = rows.get("kernel-rlimit-hard-raise-state")
    if _int(raised_state, "errno", failures, "kernel-rlimit-hard-raise-state") != 0:
        failures.append("kernel-rlimit-hard-raise-state: getrlimit failed")
    raised_cur = _int(raised_state, "cur", failures, "kernel-rlimit-hard-raise-state")
    raised_max = _int(raised_state, "max", failures, "kernel-rlimit-hard-raise-state")
    if cur is not None and raised_cur is not None and raised_cur != cur:
        failures.append(
            f"kernel-rlimit-hard-raise: soft changed {cur} -> {raised_cur} after EPERM"
        )
    if hard is not None and raised_max is not None and raised_max != hard:
        failures.append(
            f"kernel-rlimit-hard-raise: hard changed {hard} -> {raised_max} after EPERM"
        )

    if _int(rows.get("libc-getrlimit"), "ret", failures, "libc-getrlimit") != 0:
        failures.append("libc-getrlimit: expected 0")
    if _int(rows.get("libc-setrlimit"), "ret", failures, "libc-setrlimit") != 0:
        failures.append("libc-setrlimit: expected 0")
    if _int(rows.get("libc-hard-raise"), "ret", failures, "libc-hard-raise") != -1:
        failures.append("libc-hard-raise: must fail")
    if _int(rows.get("libc-hard-raise"), "errno", failures, "libc-hard-raise") != 1:
        failures.append("libc-hard-raise: expected EPERM (1)")

    nofile = rows.get("nofile")
    soft = _int(nofile, "soft", failures, "nofile")
    base = _int(nofile, "base", failures, "nofile")
    opened = _int(nofile, "open-ok", failures, "nofile")
    if soft is not None and base is not None and opened is not None:
        if opened != soft - base:
            failures.append(
                f"nofile: open-ok={opened}, expected soft-base={soft - base}"
            )
    if _int(nofile, "errno", failures, "nofile") != EMFILE:
        failures.append("nofile: expected EMFILE (24)")

    for prefix in ("kernel-rusage", "libc-rusage"):
        samples: list[tuple[int, int, int]] = []
        for index in range(3):
            name = f"{prefix}{index}"
            row = rows.get(name)
            if row is None:
                failures.append(f"{name}: missing")
                continue
            if _int(row, "errno", failures, name) != 0:
                failures.append(f"{name}: syscall failed")
            user = _int(row, "user", failures, name)
            system = _int(row, "sys", failures, name)
            maxrss = _int(row, "maxrss", failures, name)
            if user is not None and system is not None and maxrss is not None:
                samples.append((user, system, maxrss))
        for index in range(1, len(samples)):
            before = samples[index - 1][0] + samples[index - 1][1]
            after = samples[index][0] + samples[index][1]
            if after < before:
                failures.append(f"{prefix}{index}: user+sys went backwards ({before} -> {after})")
        if samples and samples[-1][2] < MAP_BYTES:
            failures.append(f"{prefix}: maxrss={samples[-1][2]} < mapped {MAP_BYTES}")

    if "umask-child" not in parse_rows(umask_exec_text):
        failures.append("umask-exec: no child row")
    else:
        child = parse_rows(umask_exec_text)["umask-child"]
        mask = _octal(child, "mask", failures, "umask-child")
        if mask is not None and mask != 0o027:
            failures.append(f"umask-exec: child mask={mask:04o}, expected 0027")

    if "FAIL" in rows:
        failures.append(f"probe: FAIL row={rows['FAIL']}")
    return failures

def selftest() -> int:
    """Exercise the row parser and every assertion with a clean and a dirty fixture."""
    failures: list[str] = []
    passwd = (
        "root:x:0:0:root:/root:/bin/brush\n"
        "raven:x:1000:1000:raven:/home/raven:/bin/brush\n"
    )
    groups = "root:x:0:root\nraven:x:1000:raven\n"
    clean = "\n".join(
        [
            "cred: ids uid=1000 euid=1000 gid=1000 egid=1000",
            "cred: groups count=1 got=1 list=1000",
            "cred: setuid-nonpermitted ret=-1 errno=1",
            "cred: setgid-nonpermitted ret=-1 errno=1",
            "cred: umask-old mask=0022",
            "cred: umask-file mode=0640",
            "cred: umask-return mask=0027",
            "cred: umask-file-077 mode=0600",
            "cred: kernel-rlimit-nofile cur=256 max=256",
            "cred: kernel-rlimit-roundtrip errno=0",
            "cred: kernel-rlimit-soft-raise errno=0",
            "cred: kernel-rlimit-hard-raise errno=1",
            "cred: kernel-rlimit-hard-raise-state errno=0 cur=256 max=256",
            "cred: kernel-rlimit-restore errno=0",
            "cred: libc-getrlimit ret=0 errno=0 cur=1024 max=32768",
            "cred: libc-setrlimit ret=0 errno=0",
            "cred: libc-hard-raise ret=-1 errno=1",
            "cred: nofile soft=1024 base=3 open-ok=1021 errno=24",
            "cred: libc-rusage0 ret=0 errno=0 user=1 sys=2 maxrss=8388608",
            "cred: libc-rusage1 ret=0 errno=0 user=2 sys=3 maxrss=8388608",
            "cred: libc-rusage2 ret=0 errno=0 user=3 sys=4 maxrss=8388608",
            "cred: libc-rusage-mapped=4194304",
            "cred: kernel-rusage0 errno=0 user=1 sys=2 maxrss=8388608",
            "cred: kernel-rusage1 errno=0 user=2 sys=3 maxrss=8388608",
            "cred: kernel-rusage2 errno=0 user=3 sys=4 maxrss=8388608",
            "cred: kernel-rusage-mapped=4194304",
            "cred: done failures=0",
            "cred: ok",
        ]
    )
    umask_exec = "cred: umask-exec-parent mask=0027\ncred: umask-child mask=0027\n"

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("clean", check_rows(clean, passwd, groups, umask_exec), [])
    dirty = clean.replace("cred: kernel-rlimit-hard-raise errno=1",
                          "cred: kernel-rlimit-hard-raise errno=0")
    check(
        "dirty-hard-raise",
        "kernel-rlimit-hard-raise: expected EPERM (1)"
        in check_rows(dirty, passwd, groups, umask_exec),
        True,
    )
    dirty_state = clean.replace(
        "cred: kernel-rlimit-hard-raise-state errno=0 cur=256 max=256",
        "cred: kernel-rlimit-hard-raise-state errno=0 cur=256 max=257",
    )
    check(
        "dirty-hard-raise-state",
        any(
            "hard changed" in failure
            for failure in check_rows(dirty_state, passwd, groups, umask_exec)
        ),
        True,
    )
    dirty_groups = clean.replace("cred: groups count=1 got=1 list=1000",
                                 "cred: groups count=2 got=2 list=1000,1000")
    check(
        "dirty-groups",
        any("groups:" in failure for failure in check_rows(dirty_groups, passwd, groups,
                                                          umask_exec)),
        True,
    )
    dirty_exec = umask_exec.replace("umask-child mask=0027", "umask-child mask=0022")
    check(
        "dirty-umask-exec",
        any("umask-exec" in failure for failure in check_rows(clean, passwd, groups, dirty_exec)),
        True,
    )
    if failures:
        for failure in failures:
            print(f"cred-conformance: selftest FAIL: {failure}", file=sys.stderr)
        return 1
    print("cred-conformance: selftest PASS")
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
    raise SystemExit(f"cred-conformance: unknown accelerator {accel!r}")


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


def run(image: Path, accel: str, mutation: bool, keep: bool) -> int:
    if not image.is_file():
        print(f"cred-conformance: image {image} is missing", file=sys.stderr)
        return 2
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = ROOT / "build" / "conformance" / "credentials" / f"run-{stamp}"
    child, serial, port = boot(run_dir, image, accel)
    try:
        code, passwd, error = ssh(port, "/bin/cat /etc/passwd", 30)
        if b"raven:x:" not in passwd or b"root:x:" not in passwd:
            print(
                f"cred-conformance: /bin/cat /etc/passwd failed (exit {code}): "
                f"stdout={passwd[-500:]!r} stderr={error[-500:]!r}",
                file=sys.stderr,
            )
            return 2
        code_group, group, error_group = ssh(port, "/bin/cat /etc/group", 30)
        if b"raven:x:1000:" not in group:
            print(
                f"cred-conformance: /bin/cat /etc/group failed (exit {code_group}): "
                f"stdout={group[-500:]!r} stderr={error_group[-500:]!r}",
                file=sys.stderr,
            )
            return 2
        command = "/bin/cred-probe --mutation=ignore-hard-limit" if mutation else "/bin/cred-probe"
        code, stdout, stderr = ssh(port, command, 60)
        code_exec, umask_exec, stderr_exec = ssh(port, "/bin/cred-probe umask-exec", 60)
        probe_text = stdout.decode("utf-8", "replace")
        (run_dir / "probe.out").write_text(probe_text, encoding="utf-8")
        (run_dir / "probe.err").write_text(stderr.decode("utf-8", "replace"), encoding="utf-8")
        (run_dir / "umask-exec.out").write_text(
            umask_exec.decode("utf-8", "replace"), encoding="utf-8"
        )
        serial_text = serial.read_text(encoding="utf-8", errors="replace")
        failures = check_rows(
            probe_text,
            passwd.decode("utf-8", "replace"),
            group.decode("utf-8", "replace"),
            umask_exec.decode("utf-8", "replace"),
        )
        for line in (
            "KERNEL PANIC",
            "EXCEPTION",
            "SegmentOutsideUserRange",
            "bad ELF",
            "killed by page fault",
            "tlb shootdown timeout",
        ):
            if line in serial_text:
                failures.append(f"serial: {line}")
        if code == 255 or code_exec == 255:
            print(
                "cred-conformance: note: the SSH command exit-status channel returned 255 while "
                "the command stdout was delivered; the probe's in-band `cred:` rows are the "
                "verdict (open gap, reported)"
            )
        if mutation:
            target = "kernel-rlimit-hard-raise: expected EPERM (1)"
            if any(target in failure for failure in failures):
                print(
                    "cred-conformance: MUTATION RED: kernel-rlimit-hard-raise errno=0 "
                    "(expected 1)"
                )
                print(f"cred-conformance: run={run_dir}")
                return 1
            print(
                "cred-conformance: MUTATION DID NOT TURN THE KERNEL HARD-RAISE ROW RED",
                file=sys.stderr,
            )
            for failure in failures:
                print(f"cred-conformance: MUTATION context: {failure}", file=sys.stderr)
            return 1
        if failures:
            for failure in failures:
                print(f"cred-conformance: FAIL: {failure}")
            print(f"cred-conformance: run={run_dir}")
            return 1
        print("cred-conformance: PASS ids groups setid umask rlimit nofile rusage")
        print(f"cred-conformance: run={run_dir}")
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
        print("cred-conformance: --image is required (or --selftest)", file=sys.stderr)
        return 2
    image = options.image if options.image.is_absolute() else ROOT / options.image
    return run(image, options.accel, options.mutation, options.keep)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
