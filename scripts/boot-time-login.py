#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Power-on to brush-login timing for the x86_64 release image.

The clock starts immediately before the QEMU process is spawned (power-on, including OVMF) and
stops when the guest prints the brush prompt ``raven# `` after the root/raven login. Every stage is
timestamped from the serial stream: loader banner, kernel banner, first ``svcd:`` line, per-unit
``svcd: <name> up`` lines, getty, login prompt, password submission and brush prompt.

No boot component is changed by this tool; it only timestamps serial arrivals and sends the login
dialogue.
"""

import argparse
import os
import re
import socket
import statistics
import subprocess
import sys
import tempfile
import time

QEMU = "qemu-system-x86_64"
DEFAULT_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"
STARTED_RE = re.compile(r"^([0-9]+(?:\.[0-9]+)?)\s+(\S+):\s+started$")
UP_RE = re.compile(r"^svcd:\s+(\S+)\s+up$")


def ms(value):
    return None if value is None else (value / 1_000_000.0)


def fmt(value):
    return "NA" if value is None else "%.1f" % value


def p90(values):
    values = sorted(values)
    if not values:
        return None
    index = int((9 * len(values) + 9) // 10) - 1
    return values[max(0, min(len(values) - 1, index))]


def read_load():
    try:
        with open("/proc/loadavg", encoding="ascii") as handle:
            return handle.read().split()[0]
    except OSError:
        return "unknown"


def parse_service_start(line):
    """Return the service name from a guest ``<epoch> <name>: started`` line."""
    match = STARTED_RE.match(line)
    return match.group(2) if match else None


def parse_service_up(line):
    """Return the service name from a ``svcd: <name> up`` line."""
    match = UP_RE.search(line)
    return match.group(1) if match else None


def run_once(args, image, vars_file, run_number, out_dir):
    sock_path = os.path.join(out_dir, "serial-%d.sock" % run_number)
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    qemu = [
        QEMU,
        "-machine", "q35",
        "-m", "512M",
        "-smp", str(args.smp),
        "-serial", "unix:%s,server=on,wait=off" % sock_path,
        "-display", "none",
        "-monitor", "none",
        "-no-reboot",
    ]
    if args.accel == "kvm":
        qemu.extend(["-enable-kvm", "-cpu", "host"])
    qemu.extend([
        "-drive", "if=pflash,format=raw,readonly=on,file=%s" % args.ovmf_code,
        "-drive", "if=pflash,format=raw,file=%s" % vars_file,
        "-drive", "format=raw,file=%s" % image,
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", "user,id=n0",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ])

    log_path = os.path.join(out_dir, "run-%d.log" % run_number)
    start = time.monotonic_ns()
    load_before = read_load()
    with open(log_path, "wb") as log:
        proc = subprocess.Popen(
            qemu,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        sock = None
        for _ in range(2000):
            try:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.connect(sock_path)
                break
            except OSError:
                sock = None
                if proc.poll() is not None:
                    break
                time.sleep(0.005)
        if sock is None:
            proc.terminate()
            proc.wait()
            raise RuntimeError("cannot connect to QEMU serial socket")

        sock.settimeout(0.01)
        deadline = start + args.timeout * 1_000_000_000
        data = b""
        line_buf = b""
        services = []
        service_start_ns = {}
        service_ready_ns = {}
        loader_ns = None
        kernel_ns = None
        first_svc_ns = None
        getty_ns = None
        login_ns = None
        password_prompt_ns = None
        password_sent_ns = None
        brush_ns = None
        sent_login = False
        sent_password = False
        kernel_id = None
        reason = "no brush prompt"

        while True:
            if time.monotonic_ns() >= deadline:
                reason = "timeout"
                break
            try:
                chunk = sock.recv(8192)
            except socket.timeout:
                continue
            except OSError:
                reason = "serial closed"
                break
            if not chunk:
                reason = "QEMU exited"
                break
            log.write(chunk)
            log.flush()
            data += chunk
            line_buf += chunk
            now = time.monotonic_ns()

            while b"\n" in line_buf:
                raw, line_buf = line_buf.split(b"\n", 1)
                line = raw.decode("utf-8", "replace").rstrip("\r")
                unit = parse_service_up(line)
                if unit:
                    services.append((unit, now))
                    service_ready_ns.setdefault(unit, now)
                    if first_svc_ns is None:
                        first_svc_ns = now
                    if unit == "getty" and getty_ns is None:
                        getty_ns = now
                elif "svcd:" in line and first_svc_ns is None:
                    first_svc_ns = now
                started = parse_service_start(line)
                if started:
                    service_start_ns.setdefault(started, now)

            if loader_ns is None and b"loader: RavenBSD loader" in data:
                loader_ns = now
            if kernel_ns is None:
                match = re.search(
                    rb"RavenBSD kernel 0\.1 \(x86_64\) ([0-9a-f]{7,40})\s",
                    data,
                )
                if match:
                    kernel_ns = now
                    kernel_id = match.group(1).decode("ascii")
            if first_svc_ns is None and b"svcd:" in data:
                first_svc_ns = now
            if getty_ns is None and b"svcd: getty up" in data:
                getty_ns = now
            if login_ns is None and b"login:" in data:
                login_ns = now
                sock.sendall(b"root\n")
                sent_login = True
            if (
                password_prompt_ns is None
                and re.search(rb"(?i)password:", data) is not None
            ):
                password_prompt_ns = now
                sock.sendall(b"raven\n")
                password_sent_ns = now
                sent_password = True
            if b"raven# " in data:
                brush_ns = now
                break

        sock.close()
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
    load_after = read_load()

    missing = []
    for name, value in (
        ("loader", loader_ns),
        ("kernel", kernel_ns),
        ("first_svcd", first_svc_ns),
        ("getty", getty_ns),
        ("login", login_ns),
        ("password", password_prompt_ns),
        ("brush", brush_ns),
    ):
        if value is None:
            missing.append(name)
    if not sent_login:
        missing.append("login_send")
    if not sent_password:
        missing.append("password_send")
    if missing:
        raise RuntimeError("%s; missing=%s" % (reason, ",".join(missing)))

    stage = {
        "firmware": ms(loader_ns - start),
        "loader": ms(kernel_ns - loader_ns),
        "kernel": ms(first_svc_ns - kernel_ns),
        "services": ms(getty_ns - first_svc_ns),
        "getty": ms(login_ns - getty_ns),
        "login_abs": ms(login_ns - start),
        "login_to_password": ms(password_prompt_ns - login_ns),
        "password": ms(brush_ns - password_sent_ns),
        "auth": ms(brush_ns - login_ns),
        "total": ms(brush_ns - start),
    }
    slowest = []
    for unit, when in services:
        slowest.append((ms(when - first_svc_ns), unit))
    slowest.sort(reverse=True)
    top = ",".join("%s:%.1f" % (unit, value) for value, unit in slowest[:3])
    service_timing = []
    for unit in sorted(set(service_start_ns) | set(service_ready_ns)):
        start_ns = service_start_ns.get(unit)
        ready_ns = service_ready_ns.get(unit)
        service_timing.append(
            {
                "name": unit,
                "start": ms(start_ns - first_svc_ns) if start_ns is not None else None,
                "ready": ms(ready_ns - first_svc_ns) if ready_ns is not None else None,
            }
        )
    service_timing.sort(
        key=lambda item: (item["ready"] is None, item["ready"] or 0.0, item["name"])
    )
    return {
        "run": run_number,
        "kernel_id": kernel_id,
        "load_before": load_before,
        "load_after": load_after,
        "stage": stage,
        "slowest": top,
        "service_timing": service_timing,
        "log": log_path,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--smp", type=int, default=4)
    parser.add_argument("--accel", choices=("kvm", "tcg"), default="kvm")
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--out", default="build/boot-time-login")
    parser.add_argument("--ovmf-code", default=os.environ.get("RAVEN_OVMF_CODE", DEFAULT_CODE))
    parser.add_argument("--ovmf-vars", default=os.environ.get("RAVEN_OVMF_VARS", DEFAULT_VARS))
    args = parser.parse_args()

    if not os.path.isfile(args.image):
        parser.error("image not found: %s" % args.image)
    if not os.path.isfile(args.ovmf_code) or not os.path.isfile(args.ovmf_vars):
        parser.error("OVMF firmware not found; set RAVEN_OVMF_CODE/RAVEN_OVMF_VARS")
    if args.runs < 1:
        parser.error("--runs must be at least 1")
    if args.accel == "kvm" and not os.access("/dev/kvm", os.R_OK | os.W_OK):
        parser.error("/dev/kvm is not readable/writable; use --accel tcg")

    commit = subprocess.check_output(
        ["git", "rev-parse", "--short", "HEAD"], text=True
    ).strip()
    os.makedirs(args.out, exist_ok=True)
    print(
        "boot-time-login: image=%s sha256=%s accel=%s smp=%s runs=%s commit=%s"
        % (
            args.image,
            subprocess.check_output(["sha256sum", args.image], text=True).split()[0],
            args.accel,
            args.smp,
            args.runs,
            commit,
        )
    )

    results = []
    with tempfile.TemporaryDirectory(prefix="boot-time-login.") as tmp:
        for run in range(1, args.runs + 1):
            vars_file = os.path.join(tmp, "vars-%d.fd" % run)
            with open(args.ovmf_vars, "rb") as source, open(vars_file, "wb") as dest:
                dest.write(source.read())
            try:
                result = run_once(args, args.image, vars_file, run, args.out)
            except RuntimeError as error:
                print("boot-time-login: FAIL run=%d %s" % (run, error), file=sys.stderr)
                return 1
            if result["kernel_id"] != commit:
                print(
                    "boot-time-login: FAIL run=%d kernel_id=%s commit=%s"
                    % (run, result["kernel_id"], commit),
                    file=sys.stderr,
                )
                return 1
            results.append(result)
            stage = result["stage"]
            print(
                "boot-time-login: run=%d firmware_ms=%s loader_ms=%s kernel_ms=%s "
                "services_ms=%s getty_ms=%s login_ms=%s login_to_password_ms=%s "
                "password_ms=%s auth_ms=%s total_ms=%s slowest=%s load=%s->%s kernel_id=%s"
                % (
                    result["run"],
                    fmt(stage["firmware"]),
                    fmt(stage["loader"]),
                    fmt(stage["kernel"]),
                    fmt(stage["services"]),
                    fmt(stage["getty"]),
                    fmt(stage["login_abs"]),
                    fmt(stage["login_to_password"]),
                    fmt(stage["password"]),
                    fmt(stage["auth"]),
                    fmt(stage["total"]),
                    result["slowest"] or "-",
                    result["load_before"],
                    result["load_after"],
                    result["kernel_id"],
                )
            )
            print(
                "boot-time-login: services run=%d %s"
                % (
                    result["run"],
                    ",".join(
                        "%s:%s:%s"
                        % (item["name"], fmt(item["start"]), fmt(item["ready"]))
                        for item in result["service_timing"]
                    ),
                )
            )

    keys = (
        "firmware",
        "loader",
        "kernel",
        "services",
        "getty",
        "login_abs",
        "login_to_password",
        "password",
        "auth",
        "total",
    )
    summary = {}
    for key in keys:
        values = [result["stage"][key] for result in results]
        summary[key] = (
            statistics.median(values),
            p90(values),
            max(values),
        )
    print(
        "boot-time-login: p50 firmware_ms=%s loader_ms=%s kernel_ms=%s services_ms=%s "
        "getty_ms=%s login_ms=%s login_to_password_ms=%s password_ms=%s auth_ms=%s total_ms=%s"
        % tuple(fmt(summary[key][0]) for key in keys)
    )
    print(
        "boot-time-login: p90 firmware_ms=%s loader_ms=%s kernel_ms=%s services_ms=%s "
        "getty_ms=%s login_ms=%s login_to_password_ms=%s password_ms=%s auth_ms=%s total_ms=%s"
        % tuple(fmt(summary[key][1]) for key in keys)
    )
    print(
        "boot-time-login: max firmware_ms=%s loader_ms=%s kernel_ms=%s services_ms=%s "
        "getty_ms=%s login_ms=%s login_to_password_ms=%s password_ms=%s auth_ms=%s total_ms=%s"
        % tuple(fmt(summary[key][2]) for key in keys)
    )

    service_values = {}
    for result in results:
        for item in result["service_timing"]:
            entry = service_values.setdefault(item["name"], {"start": [], "ready": []})
            if item["start"] is not None:
                entry["start"].append(item["start"])
            if item["ready"] is not None:
                entry["ready"].append(item["ready"])
    service_summary = []
    for name, values in service_values.items():
        start = statistics.median(values["start"]) if values["start"] else None
        ready = statistics.median(values["ready"]) if values["ready"] else None
        service_summary.append((ready, start, name))
    service_summary.sort(key=lambda item: (item[0] is None, item[0] or 0.0, item[2]))
    print(
        "boot-time-login: service_p50="
        + ",".join(
            "%s:%s:%s" % (name, fmt(start), fmt(ready))
            for ready, start, name in service_summary
        )
    )

    total = summary["total"][0]
    if total < 1000.0:
        print("boot-time-login: TARGET<1000ms MET total_p50_ms=%.1f" % total)
    else:
        candidates = [
            ("firmware", "OVMF firmware/platform boot"),
            ("loader", "loader"),
            ("kernel", "kernel init"),
            ("services", "svcd component service"),
            ("getty", "getty/login"),
            ("login_to_password", "login authentication"),
            ("password", "brush shell/libc startup"),
        ]
        name, owner = max(candidates, key=lambda item: summary[item[0]][0])
        print(
            "boot-time-login: TARGET<1000ms MISSED total_p50_ms=%.1f dominant=%s ms=%.1f owner=%s"
            % (total, name, summary[name][0], owner)
        )

    slow_units = {}
    for result in results:
        for item in result["slowest"].split(","):
            if ":" not in item:
                continue
            unit, value = item.split(":", 1)
            slow_units.setdefault(unit, []).append(float(value))
    if slow_units:
        ranked = sorted(
            (statistics.median(values), unit) for unit, values in slow_units.items()
        )
        print(
            "boot-time-login: slowest_svcd_units="
            + ",".join("%s:%.1f" % (unit, value) for value, unit in reversed(ranked[-3:]))
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
