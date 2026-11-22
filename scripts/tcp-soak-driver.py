#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K285 TCP soak driver.

Supervises one foreground QEMU boot of the tcp-soak image, logs in, stages the guest TCP
workload (/etc/tcp-soak-workload.sh), timestamps every cycle, preserves a raw serial log
plus JSONL/summary, and fails on a workload error, a boot timeout, a QEMU exit, or a
missing cycle-done heartbeat (stall). A stall is a failure, never a retry: on stall the
driver dumps the last socket-state evidence and the last serial lines before exiting
non-zero.

`--selftest` is the fast no-QEMU proof: it checks the gap monitor against healthy and
injected gaps and replays a healthy transcript plus a stall-injected transcript through
the line handler. It never boots QEMU and never claims a 24 h pass.
"""

import argparse
import json
import os
import pty
import queue
import select
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from pathlib import Path


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


class Monitor:
    """Stall detector shared by the real run and the --selftest proof."""

    def __init__(self, stall_seconds):
        self.stall_seconds = float(stall_seconds)
        self.last_progress = time.monotonic()

    def progress(self, now=None):
        self.last_progress = time.monotonic() if now is None else now

    def gap(self, now=None):
        now = time.monotonic() if now is None else now
        return now - self.last_progress

    def stalled(self, now=None):
        return self.gap(now) > self.stall_seconds


class Serial:
    """Read the QEMU serial pty, preserve raw/stamped logs, and queue complete lines."""

    def __init__(self, master, raw_path, stamped_path):
        self.master = master
        self.raw = open(raw_path, "wb")
        self.stamped = open(stamped_path, "w", encoding="utf-8")
        self.lines = queue.Queue()
        self.lock = threading.Lock()
        self.text = ""
        self.stop = threading.Event()
        self.start = time.monotonic()
        self.thread = threading.Thread(target=self._reader, daemon=True)

    def start_reader(self):
        self.thread.start()

    def send(self, text):
        os.write(self.master, text.encode())

    def _reader(self):
        buffer = b""
        while not self.stop.is_set():
            ready, _, _ = select.select([self.master], [], [], 0.1)
            if not ready:
                continue
            try:
                chunk = os.read(self.master, 4096)
            except OSError:
                break
            if not chunk:
                break
            now = time.monotonic()
            self.raw.write(chunk)
            self.raw.flush()
            text = chunk.decode(errors="replace")
            with self.lock:
                self.text = (self.text + text)[-131072:]
            buffer += chunk
            while b"\n" in buffer:
                raw_line, buffer = buffer.split(b"\n", 1)
                line = raw_line.decode(errors="replace").rstrip("\r")
                stamped = f"{now - self.start:.6f} {line}\n"
                self.stamped.write(stamped)
                self.stamped.flush()
                self.lines.put((now, line))
        if buffer:
            line = buffer.decode(errors="replace").rstrip("\r")
            now = time.monotonic()
            self.stamped.write(f"{now - self.start:.6f} {line}\n")
            self.stamped.flush()
            self.lines.put((now, line))
        self.stamped.close()
        self.raw.close()

    def wait_text(self, needle, timeout, proc=None):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            with self.lock:
                if needle in self.text:
                    return True
            if proc is not None and proc.poll() is not None:
                return False
            time.sleep(0.05)
        return False

    def wait_line(self, needle, timeout, proc=None):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                _, line = self.lines.get(timeout=0.25)
            except queue.Empty:
                if proc is not None and proc.poll() is not None:
                    return False
                continue
            if needle in line:
                return True
        return False

    def close(self):
        self.stop.set()
        self.thread.join(timeout=2)


MARKERS = (
    "tcp-ready",
    "tcp-cycle: ",
    "tcp-echo: ",
    "tcp-wait: ",
    "tcp-cycle-done: ",
    "tcp-done",
)


class Driver:
    def __init__(self, args):
        self.args = args
        self.out = Path(args.out)
        self.out.mkdir(parents=True, exist_ok=True)
        self.events_path = self.out / "tcp-soak.jsonl"
        self.events = open(self.events_path, "a", encoding="utf-8")
        self.socket_state_path = self.out / "socket-state.log"
        self.socket_state = open(self.socket_state_path, "a", encoding="utf-8")
        self.start_mono = time.monotonic()
        self.start_utc = utc_now()
        self.monitor = Monitor(args.stall)
        self.expected = max(1, int(args.duration) // max(1, int(args.interval)))
        self.cycles = 0
        self.cycle = 0
        self.echo_ok = False
        self.echo_rc = ""
        self.wait_rc = ""
        self.result = "running"
        self.error = ""
        self.proc = None
        self.serial = None
        self.vars_copy = self.out / "OVMF_VARS.fd"

    def event(self, name, **fields):
        record = {
            "utc": utc_now(),
            "mono_s": round(time.monotonic() - self.start_mono, 6),
            "event": name,
        }
        record.update(fields)
        self.events.write(json.dumps(record, sort_keys=True) + "\n")
        self.events.flush()

    def fail(self, result, error):
        if self.result != "running":
            return
        self.result = result
        self.error = error
        self.event("failure", result=result, error=error)

    def qemu_command(self):
        command = [
            self.args.qemu,
            "-machine",
            "q35",
            "-m",
            "512M",
            "-smp",
            "4",
        ]
        if os.access("/dev/kvm", os.R_OK | os.W_OK):
            command += ["-enable-kvm", "-cpu", "host"]
        command += [
            "-serial",
            "stdio",
            "-display",
            "none",
            "-monitor",
            "none",
            "-no-reboot",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={self.args.code}",
            "-drive",
            f"if=pflash,format=raw,file={self.vars_copy}",
            "-drive",
            f"format=raw,file={self.args.image}",
            "-nic",
            "none",
            "-netdev",
            "user,id=n0",
            "-device",
            "virtio-net-pci,netdev=n0,disable-legacy=on",
        ]
        return command

    def handle(self, now, line):
        if line.startswith("tcp-cycle: "):
            try:
                self.cycle = int(line.split(":", 1)[1].strip())
            except ValueError:
                self.fail("workload", f"bad cycle line: {line}")
                return
            self.echo_ok = False
            self.echo_rc = ""
            self.wait_rc = ""
            self.event("cycle", cycle=self.cycle)
        elif line.startswith("tcp-echo: "):
            self.echo_rc = line.split(":", 1)[1].strip()
            self.echo_ok = self.echo_rc == "0"
            self.event("echo", cycle=self.cycle, rc=self.echo_rc)
            if not self.echo_ok:
                self.fail(
                    "workload",
                    f"nc client exit {self.echo_rc} in cycle {self.cycle}",
                )
        elif line.startswith("tcp-wait: "):
            self.wait_rc = line.split(":", 1)[1].strip()
            self.event("wait", cycle=self.cycle, rc=self.wait_rc)
        elif line.startswith("tcp-cycle-done: "):
            try:
                number = int(line.split(":", 1)[1].strip())
            except ValueError:
                self.fail("workload", f"bad cycle-done line: {line}")
                return
            if number != self.cycle or not self.echo_ok:
                missing = []
                if number != self.cycle:
                    missing.append(f"cycle marker {self.cycle}")
                if not self.echo_ok:
                    missing.append(f"echo rc={self.echo_rc or '-'}")
                self.fail("workload", f"cycle {number} incomplete: missing {missing}")
                return
            self.cycles = number
            self.monitor.progress(now)
            self.event("cycle_done", cycle=number, cycles=self.cycles)
        elif line.startswith("tcp-ready"):
            self.event("ready_seen", expected_cycles=self.expected)
        elif line.startswith("tcp-done"):
            self.event("workload_done", cycles=self.cycles)
        else:
            # Non-marker guest output between markers is socket/traffic evidence.
            self.socket_state.write(f"[cycle {self.cycle}] {line}\n")
            self.socket_state.flush()

    def run(self):
        shutil.copyfile(self.args.vars, self.vars_copy)
        master, slave = pty.openpty()
        self.event(
            "start",
            image=self.args.image,
            profile=self.args.profile,
            commit=self.args.commit,
            duration_s=self.args.duration,
            interval_s=self.args.interval,
            port=self.args.port,
            expected_cycles=self.expected,
            stall_s=self.args.stall,
            qemu=self.args.qemu,
            kvm=os.access("/dev/kvm", os.R_OK | os.W_OK),
        )
        self.proc = subprocess.Popen(
            self.qemu_command(),
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
            start_new_session=True,
        )
        os.close(slave)
        self.serial = Serial(master, self.out / "serial.log", self.out / "serial.stamped.log")
        self.serial.start_reader()

        if not self.serial.wait_text("login:", 120, self.proc):
            self.fail("boot", "guest did not reach the login prompt within 120 s")
            return self.finish()
        self.serial.send("root\n")
        if not self.serial.wait_text("Password:", 30, self.proc):
            self.fail("boot", "guest did not reach the password prompt")
            return self.finish()
        self.serial.send("raven\n")
        if not self.serial.wait_text("raven# ", 60, self.proc):
            self.fail("boot", "guest did not reach the root prompt")
            return self.finish()
        self.event("login")

        command = (
            f"/bin/brush /etc/tcp-soak-workload.sh "
            f"{self.expected} {self.args.interval} {self.args.port}\n"
        )
        self.serial.send(command)
        if not self.serial.wait_line("tcp-ready", 60, self.proc):
            self.fail("workload", "guest did not print tcp-ready")
            return self.finish()
        self.monitor.progress()
        self.event("ready", expected_cycles=self.expected)

        deadline = time.monotonic() + float(self.args.duration) + 120.0
        while self.cycles < self.expected:
            try:
                now, line = self.serial.lines.get(timeout=0.25)
            except queue.Empty:
                now = time.monotonic()
                line = ""
            if line:
                self.handle(now, line)
                if self.result != "running":
                    break
            if self.proc.poll() is not None and self.cycles < self.expected:
                self.fail(
                    "qemu",
                    f"QEMU exited before the workload finished ({self.cycles}/{self.expected})",
                )
                break
            if self.result == "running" and self.monitor.stalled(now):
                gap = self.monitor.gap(now)
                self.event(
                    "stall",
                    gap_s=round(gap, 3),
                    threshold_s=self.args.stall,
                    last_cycle=self.cycles,
                )
                self.fail(
                    "stall",
                    f"no cycle-done for {gap:.1f}s (threshold {self.args.stall}s) "
                    f"after cycle {self.cycles}",
                )
                break
            if time.monotonic() > deadline:
                self.fail("timeout", "workload did not finish before the host deadline")
                break

        if self.result == "running":
            if not self.serial.wait_line("tcp-done", max(30, float(self.args.stall)), self.proc):
                self.fail("timeout", "guest did not print tcp-done")
            else:
                self.result = "pass"
                self.event("done", cycles=self.cycles, expected_cycles=self.expected)
        return self.finish()

    def dump_evidence(self):
        """Preserve the last socket-state snapshot and the last serial lines."""
        path = self.out / "stall-evidence.txt"
        lines = [
            f"tcp-soak evidence dump ({self.result}: {self.error or '-'})",
            f"start_utc={self.start_utc} last_cycle={self.cycles}/"
            f"{self.expected} stall_seconds={self.args.stall}",
            f"gap_s={round(self.monitor.gap(), 3)}",
            "pcap: n/a (in-guest tcpdump is not packaged in the tcp-soak profile yet; "
            "see the TCP soak runbook backlog)",
            "",
            "--- last socket-state evidence lines ---",
        ]
        try:
            snapshot = Path(self.socket_state_path).read_text(encoding="utf-8").splitlines()
            lines.extend(snapshot[-40:])
        except OSError:
            lines.append("(socket-state.log missing)")
        lines.append("")
        lines.append("--- last 200 serial lines (stamped) ---")
        try:
            stamped = (self.out / "serial.stamped.log").read_text(encoding="utf-8").splitlines()
            lines.extend(stamped[-200:])
        except OSError:
            lines.append("(serial.stamped.log missing)")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        self.event("evidence_dump", path=str(path))

    def finish(self):
        if self.result != "pass":
            self.dump_evidence()
        if self.serial is not None:
            if self.proc is not None and self.proc.poll() is None:
                self.serial.send("\x03")
                time.sleep(0.2)
                self.serial.send("exit 0\n")
                try:
                    self.proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    pass
            if self.proc is not None and self.proc.poll() is None:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait()
            self.serial.close()
        self.write_summary()
        self.socket_state.close()
        self.events.close()
        return 0 if self.result == "pass" else 1

    def write_summary(self):
        end_utc = utc_now()
        elapsed = round(time.monotonic() - self.start_mono, 3)
        summary = {
            "result": self.result,
            "error": self.error,
            "start_utc": self.start_utc,
            "end_utc": end_utc,
            "elapsed_s": elapsed,
            "requested_duration_s": self.args.duration,
            "interval_s": self.args.interval,
            "port": self.args.port,
            "expected_cycles": self.expected,
            "cycles": self.cycles,
            "stall_seconds": self.args.stall,
            "stall_gap_s": round(self.monitor.gap(), 3),
            "image": self.args.image,
            "profile": self.args.profile,
            "commit": self.args.commit,
            "qemu": self.args.qemu,
            "serial_log": str(self.out / "serial.log"),
            "serial_stamped": str(self.out / "serial.stamped.log"),
            "socket_state": str(self.socket_state_path),
            "evidence_dump": str(self.out / "stall-evidence.txt")
            if self.result != "pass"
            else "",
            "events": str(self.events_path),
            "summary": str(self.out / "summary.json"),
        }
        (self.out / "summary.json").write_text(
            json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        (self.out / "summary.txt").write_text(
            "\n".join(
                [
                    f"tcp-soak: {self.result}",
                    f"error: {self.error or '-'}",
                    f"cycles: {self.cycles}/{self.expected}",
                    f"elapsed_s: {elapsed}",
                    f"stall_seconds: {self.args.stall}",
                    f"serial: {self.out / 'serial.log'}",
                    f"socket-state: {self.socket_state_path}",
                    f"events: {self.events_path}",
                    f"summary: {self.out / 'summary.json'}",
                ]
            )
            + "\n",
            encoding="utf-8",
        )


def selftest():
    """Monitor proof plus transcript replay (no QEMU)."""
    monitor = Monitor(5.0)
    base = 100.0
    monitor.progress(base)
    for offset in (2.0, 4.0, 6.0):
        monitor.progress(base + offset)
    if monitor.stalled(base + 8.0):
        print(
            "tcp-soak-selftest: FAIL healthy 2 s gaps were reported as a stall",
            file=sys.stderr,
        )
        return 1
    if not monitor.stalled(base + 12.0):
        print(
            "tcp-soak-selftest: FAIL injected 6 s gap was not detected",
            file=sys.stderr,
        )
        return 1

    args = parse_selftest_args()
    driver = Driver(args)

    # Healthy transcript: two complete cycles then workload done.
    healthy = [
        ("tcp-cycle: 1", 0.0),
        ("tcp-echo: 0", 1.0),
        ("tcp-wait: 0", 2.0),
        ("tcp-cycle-done: 1", 3.0),
        ("tcp-cycle: 2", 4.0),
        ("tcp-echo: 0", 5.0),
        ("tcp-wait: 0", 6.0),
        ("tcp-cycle-done: 2", 7.0),
        ("tcp-done", 8.0),
    ]
    for line, offset in healthy:
        driver.handle(base + offset, line)
    if driver.cycles != 2 or driver.result != "running":
        print(
            f"tcp-soak-selftest: FAIL healthy transcript gave cycles={driver.cycles} "
            f"result={driver.result}",
            file=sys.stderr,
        )
        return 1

    # Stall-injected transcript: cycle 1 completes, then silence past the threshold.
    stall_driver = Driver(args)
    for line, offset in [
        ("tcp-cycle: 1", 0.0),
        ("tcp-echo: 0", 1.0),
        ("tcp-wait: 0", 2.0),
        ("tcp-cycle-done: 1", 3.0),
    ]:
        stall_driver.handle(base + offset, line)
    now = base + 3.0 + 5.0 + 2.0  # last progress at base+3, now base+10
    if not stall_driver.monitor.stalled(now):
        print(
            "tcp-soak-selftest: FAIL injected stall (7 s silence, 5 s threshold) "
            "was not detected",
            file=sys.stderr,
        )
        return 1
    gap = stall_driver.monitor.gap(now)
    stall_driver.event(
        "stall",
        gap_s=round(gap, 3),
        threshold_s=args.stall,
        last_cycle=stall_driver.cycles,
    )
    stall_driver.fail("stall", f"selftest injected stall after cycle {stall_driver.cycles}")
    if stall_driver.result != "stall":
        print(
            f"tcp-soak-selftest: FAIL injected stall did not fail the run "
            f"(result={stall_driver.result})",
            file=sys.stderr,
        )
        return 1

    # Negative transcript: a non-zero nc client rc must fail the run immediately.
    bad_driver = Driver(args)
    bad_driver.handle(base, "tcp-cycle: 1")
    bad_driver.handle(base + 1, "tcp-echo: 1")
    if bad_driver.result != "workload":
        print(
            f"tcp-soak-selftest: FAIL non-zero echo rc did not fail the run "
            f"(result={bad_driver.result})",
            file=sys.stderr,
        )
        return 1

    print(
        "tcp-soak-selftest: PASS healthy_gap=2s injected_gap=6s threshold=5s "
        f"last_progress_s={monitor.last_progress - base:.1f} "
        f"healthy_cycles={driver.cycles} stall_result={stall_driver.result} "
        f"bad_echo_result={bad_driver.result}"
    )
    return 0


def parse_selftest_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "tcp-soak-selftest"))
    parser.add_argument("--stall", type=float, default=5.0)
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument("--interval", type=int, default=5)
    parser.add_argument("--port", type=int, default=7000)
    parser.add_argument("--image", default="selftest")
    parser.add_argument("--profile", default="selftest")
    parser.add_argument("--commit", default="selftest")
    return parser.parse_known_args()[0]


def parse():
    parser = argparse.ArgumentParser()
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--image")
    parser.add_argument("--qemu", default="qemu-system-x86_64")
    parser.add_argument("--code", default="/usr/share/OVMF/OVMF_CODE_4M.fd")
    parser.add_argument("--vars", default="/usr/share/OVMF/OVMF_VARS_4M.fd")
    parser.add_argument("--duration", type=int, default=600)
    parser.add_argument("--interval", type=int, default=10)
    parser.add_argument("--stall", type=float, default=40.0)
    parser.add_argument("--port", type=int, default=7000)
    parser.add_argument("--out", default="build/tcp-soak/run")
    parser.add_argument("--profile", default="tcp-soak")
    parser.add_argument("--commit", default="unknown")
    args = parser.parse_args()
    if args.selftest:
        return args
    for name in ("image", "qemu", "code", "vars", "out"):
        if not getattr(args, name):
            parser.error(f"--{name} is required")
    return args


def main():
    args = parse()
    if args.selftest:
        return selftest()
    return Driver(args).run()


if __name__ == "__main__":
    sys.exit(main())
