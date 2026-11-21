#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K300 TCP soak evidence runner and collector.

Wraps the K285 soak entry point (`scripts/tcp-soak.sh`, which supervises
`scripts/tcp-soak-driver.py`) with two things the ticket asks for:

* a 24 h runner with a host-side stall monitor. While the soak child is
  alive, the run artifacts (event stream, serial logs, socket-state
  evidence) must keep growing. No growth for `--stall` seconds is a
  RUNNER stall: the runner writes `runner-stall-evidence.txt` (tails of
  the evidence streams plus a sha256 manifest of everything present),
  terminates the child process group and exits non-zero. A stall is a
  failure, never a retry. This complements the driver's guest-level
  stall detection (missing `tcp-cycle-done` heartbeat); the runner
  catches host-side hangs the guest cannot report.
* artifact-hash evidence. After the run (pass or fail) the collector
  hashes every run artifact (sha256 + size) and writes
  `evidence.json` / `evidence.txt` into the run directory.
  `evidence.txt` is sha256sum-compatible so the owner can re-verify
  later with coreutils. `--verify EVIDENCE_JSON` re-hashes the recorded
  artifacts and fails on any missing file, size change or hash
  mismatch. That is what the ticket's mutation check exercises: tamper
  with an artifact after collection and verification must fail.

`--selftest` is the no-QEMU proof: a synthetic run collects and
verifies cleanly, a tampered artifact fails verification, a missing
artifact fails verification, a summary-less run directory fails
collection, and the stall monitor does not flag healthy progress but
does flag an injected gap.
"""

import argparse
import hashlib
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

SCHEMA = "ravenbsd.tcp-soak.evidence/1"
TICKET = "K300"

# The driver's known run artifacts, relative to the run directory.
ARTIFACTS = (
    "summary.json",
    "summary.txt",
    "serial.log",
    "serial.stamped.log",
    "tcp-soak.jsonl",
    "socket-state.log",
    "stall-evidence.txt",
)

# Artifacts whose byte growth counts as runner-level progress.
MONITORED = (
    "tcp-soak.jsonl",
    "serial.stamped.log",
    "serial.log",
    "socket-state.log",
)

EXIT_STALL = 3
EXIT_USAGE = 4


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path):
    """Return (hexdigest, size_in_bytes) for one file."""
    digest = hashlib.sha256()
    size = 0
    with open(path, "rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
            size += len(chunk)
    return digest.hexdigest(), size


def collect(run_dir, image=None):
    """Hash the run artifacts and write evidence.json / evidence.txt.

    Returns (evidence_dict, error). error is None on success.
    """
    run_dir = Path(run_dir)
    if not run_dir.is_dir():
        return None, f"run dir missing: {run_dir}"
    summary_path = run_dir / "summary.json"
    if not summary_path.exists():
        return None, f"summary.json missing in {run_dir} (the run did not finish)"
    try:
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return None, f"cannot parse {summary_path}: {exc}"

    artifacts = []
    for name in ARTIFACTS:
        path = run_dir / name
        if not path.exists():
            continue
        digest, size = sha256_file(path)
        artifacts.append({"name": name, "sha256": digest, "size": size})

    evidence = {
        "schema": SCHEMA,
        "ticket": TICKET,
        "collected_utc": utc_now(),
        "collector": "scripts/tcp-soak-evidence.py",
        "run_dir": str(run_dir.resolve()),
        "run_summary": summary,
        "artifacts": artifacts,
        "image": None,
    }
    if image:
        image_path = Path(image)
        if image_path.exists():
            digest, size = sha256_file(image_path)
            evidence["image"] = {
                "path": str(image_path.resolve()),
                "sha256": digest,
                "size": size,
            }
        else:
            print(
                f"tcp-soak-evidence: warning: image not found, skipping: {image}",
                file=sys.stderr,
            )

    (run_dir / "evidence.json").write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    manifest = "\n".join(
        [f"{a['sha256']}  {run_dir.resolve() / a['name']}" for a in artifacts]
    )
    if evidence["image"] is not None:
        manifest += f"\n{evidence['image']['sha256']}  {evidence['image']['path']}"
    (run_dir / "evidence.txt").write_text(manifest + "\n", encoding="utf-8")
    return evidence, None


def verify(evidence_path):
    """Re-hash the artifacts recorded in an evidence file.

    Returns (ok, problems).
    """
    path = Path(evidence_path)
    try:
        evidence = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return False, [f"cannot read evidence file {evidence_path}: {exc}"]
    run_dir = Path(evidence.get("run_dir", ""))
    if not run_dir.is_dir():
        return False, [f"run_dir from evidence file missing: {run_dir}"]

    problems = []
    for entry in evidence.get("artifacts", []):
        name = entry.get("name", "")
        target = run_dir / name
        if not target.exists():
            problems.append(f"missing artifact: {name}")
            continue
        digest, size = sha256_file(target)
        if size != entry.get("size"):
            problems.append(
                f"size changed: {name} (now {size}, recorded {entry.get('size')})"
            )
        if digest != entry.get("sha256"):
            problems.append(f"hash mismatch: {name}")
    image = evidence.get("image")
    if image is not None:
        target = Path(image.get("path", ""))
        if not target.exists():
            problems.append(f"missing image: {target}")
        else:
            digest, size = sha256_file(target)
            if size != image.get("size") or digest != image.get("sha256"):
                problems.append(f"hash mismatch: image {target}")
    return not problems, problems


class Monitor:
    """Host-side stall detector: no monitored progress for the threshold."""

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


def _total_monitored_bytes(run_dir):
    total = 0
    for name in MONITORED:
        try:
            total += (run_dir / name).stat().st_size
        except OSError:
            pass
    return total


def _newest_run_dir(out_root, before):
    candidates = [
        path
        for path in out_root.glob("run-*")
        if path.is_dir() and path.name not in before
    ]
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.name)


def _kill_process_group(proc):
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except (ProcessLookupError, PermissionError):
        pass
    try:
        proc.wait(timeout=5)
        return
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    proc.wait()


def handle_runner_stall(run_dir, proc, monitor, out_root, stall_seconds):
    """Preserve stall evidence, kill the child tree, return EXIT_STALL."""
    lines = [
        "tcp-soak-evidence runner stall",
        f"utc={utc_now()}",
        f"child_pid={proc.pid}",
        f"stall_seconds={stall_seconds}",
        f"stall_gap_s={round(monitor.gap(), 3)}",
        f"run_dir={run_dir if run_dir is not None else 'not found'}",
        "child state=killed by runner (a stall is a failure, never a retry)",
        "",
    ]
    if run_dir is not None:
        for label, name, count in (
            ("last 50 tcp-soak.jsonl lines", "tcp-soak.jsonl", 50),
            ("last 100 serial.stamped.log lines", "serial.stamped.log", 100),
        ):
            path = run_dir / name
            lines.append(f"--- {label} ---")
            if path.exists():
                lines.extend(path.read_text(encoding="utf-8", errors="replace").splitlines()[-count:])
            else:
                lines.append(f"({name} missing)")
            lines.append("")
        evidence, error = collect(run_dir)
        lines.append("--- artifact manifest at stall time ---")
        if evidence is not None:
            for entry in evidence["artifacts"]:
                lines.append(f"{entry['sha256']}  {run_dir / entry['name']}")
            lines.append(f"evidence={run_dir / 'evidence.json'} (partial-run manifest)")
        else:
            lines.append(f"(collect failed: {error})")
    path = out_root / "runner-stall-evidence.txt"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(
        f"tcp-soak-evidence: RUNNER STALL no artifact progress for "
        f"{stall_seconds}s; evidence={path}; killing pid {proc.pid}",
        file=sys.stderr,
    )
    _kill_process_group(proc)
    return EXIT_STALL


def run_soak(args):
    root = Path(__file__).resolve().parent.parent
    out_root = Path(args.out)
    out_root.mkdir(parents=True, exist_ok=True)
    before = {p.name for p in out_root.glob("run-*") if p.is_dir()}

    command = [
        "sh",
        str(root / "scripts" / "tcp-soak.sh"),
        "--duration",
        str(args.duration),
        "--interval",
        str(args.interval),
        "--out",
        str(out_root),
    ]
    if args.port:
        command += ["--port", str(args.port)]
    if args.image:
        command += ["--image", str(args.image)]
    print(
        f"tcp-soak-evidence: run={' '.join(command)} "
        f"runner_stall={args.stall}s (driver-level stall is set by scripts/tcp-soak.sh)"
    )
    proc = subprocess.Popen(
        command,
        cwd=root,
        start_new_session=True,
    )

    monitor = Monitor(args.stall)
    monitor.progress()
    run_dir = None
    last_total = -1
    while proc.poll() is None:
        time.sleep(1.0)
        now = time.monotonic()
        if run_dir is None:
            run_dir = _newest_run_dir(out_root, before)
            if run_dir is not None:
                monitor.progress(now)
                print(f"tcp-soak-evidence: monitoring {run_dir}")
        if run_dir is not None:
            total = _total_monitored_bytes(run_dir)
            if total > last_total:
                last_total = total
                monitor.progress(now)
        if monitor.stalled(now):
            return handle_runner_stall(run_dir, proc, monitor, out_root, args.stall)

    if run_dir is None:
        run_dir = _newest_run_dir(out_root, before)
    if run_dir is None:
        print(
            f"tcp-soak-evidence: FAIL no run directory found under {out_root}",
            file=sys.stderr,
        )
        return EXIT_USAGE

    # Without an explicit --image, hash the default image the K285 entry
    # point uses, so the evidence names the exact bytes that were booted.
    image = args.image or str(root / "build" / "tcp-soak" / "image.img")
    evidence, error = collect(run_dir, image=image)
    if error is not None:
        print(
            f"tcp-soak-evidence: FAIL evidence collection: {error}", file=sys.stderr
        )
    else:
        result = evidence["run_summary"].get("result", "unknown")
        cycles = evidence["run_summary"].get("cycles")
        expected = evidence["run_summary"].get("expected_cycles")
        print(
            f"tcp-soak-evidence: evidence={run_dir / 'evidence.json'} "
            f"result={result} cycles={cycles}/{expected} "
            f"artifacts={len(evidence['artifacts'])}"
        )
    return proc.returncode if proc.returncode is not None else 0


def _write_artifacts(run_dir, names):
    for name in names:
        path = run_dir / name
        if name == "summary.json":
            path.write_text(
                json.dumps(
                    {
                        "result": "pass",
                        "error": "",
                        "cycles": 2,
                        "expected_cycles": 2,
                        "elapsed_s": 12.0,
                        "commit": "selftest",
                    },
                    indent=2,
                    sort_keys=True,
                )
                + "\n",
                encoding="utf-8",
            )
        elif name == "tcp-soak.jsonl":
            path.write_text(
                '{"cycle": 1, "event": "cycle_done"}\n'
                '{"cycle": 2, "event": "cycle_done"}\n',
                encoding="utf-8",
            )
        elif name == "serial.log":
            path.write_text("tcp-ready\ntcp-cycle: 1\ntcp-done\n", encoding="utf-8")
        elif name == "serial.stamped.log":
            path.write_text(
                "0.100 tcp-ready\n0.200 tcp-cycle: 1\n0.300 tcp-done\n", encoding="utf-8"
            )
        elif name == "socket-state.log":
            path.write_text(
                "[cycle 1] netstat: tcp TIME_WAIT 127.0.0.1:7001 iface=lo\n",
                encoding="utf-8",
            )
        elif name == "summary.txt":
            path.write_text("tcp-soak: pass\n", encoding="utf-8")


def selftest():
    """No-QEMU proof of collection, verification and stall detection."""
    failures = []
    root = Path(tempfile.mkdtemp(prefix="tcp-soak-evidence-selftest-"))

    # 1) Healthy synthetic run: collect then verify must pass.
    healthy = root / "run-healthy"
    healthy.mkdir()
    _write_artifacts(healthy, list(ARTIFACTS))
    evidence, error = collect(healthy)
    if error is not None or evidence is None:
        failures.append(f"healthy collect failed: {error}")
    else:
        for entry in evidence["artifacts"]:
            digest, _ = sha256_file(healthy / entry["name"])
            if digest != entry["sha256"]:
                failures.append(f"evidence hash wrong for {entry['name']}")
        ok, problems = verify(healthy / "evidence.json")
        if not ok:
            failures.append(f"healthy verify failed: {problems}")
        else:
            print("selftest: collect+verify healthy PASS")

    # 2) Mutation check: tamper with an artifact, verification must fail.
    target = healthy / "socket-state.log"
    with open(target, "ab") as handle:
        handle.write(b"tampered\n")
    ok, problems = verify(healthy / "evidence.json")
    if ok:
        failures.append("mutation check: tampered artifact was NOT detected")
    elif not any("socket-state.log" in p for p in problems):
        failures.append(
            f"mutation check: detection did not name the artifact: {problems}"
        )
    else:
        print("selftest: mutation check (tampered artifact) PASS")

    # 3) Missing-artifact negative check.
    (healthy / "serial.log").unlink()
    ok, problems = verify(healthy / "evidence.json")
    if ok:
        failures.append("missing artifact was NOT detected")
    elif not any("serial.log" in p for p in problems):
        failures.append(f"missing-artifact check did not name the file: {problems}")
    else:
        print("selftest: missing-artifact check PASS")

    # 4) A run directory without summary.json must fail collection.
    empty = root / "run-nosummary"
    empty.mkdir()
    _, error = collect(empty)
    if error is None:
        failures.append("collect on a summary-less run dir should have failed")
    else:
        print("selftest: no-summary collect failure PASS")

    # 5) Stall monitor: healthy 2 s gaps are not flagged, an injected 6 s
    #    gap past the 5 s threshold is.
    monitor = Monitor(5.0)
    base = 100.0
    monitor.progress(base)
    for offset in (2.0, 4.0, 6.0):
        monitor.progress(base + offset)
    if monitor.stalled(base + 8.0):
        failures.append("stall monitor flagged healthy 2 s gaps")
    elif not monitor.stalled(base + 12.0):
        failures.append("stall monitor did not flag the injected 6 s gap")
    else:
        print("selftest: stall monitor healthy/injected PASS")

    # 6) No progress at all: a fresh monitor stalls just past the threshold.
    silent = Monitor(5.0)
    silent.progress(0.0)
    if silent.stalled(5.0):
        failures.append("stall monitor fired exactly at the threshold")
    elif not silent.stalled(6.0):
        failures.append("stall monitor never fired on total silence")
    else:
        print("selftest: total-silence stall PASS")

    if failures:
        for failure in failures:
            print(f"tcp-soak-evidence-selftest: FAIL {failure}", file=sys.stderr)
        return 1
    print(
        "tcp-soak-evidence-selftest: PASS collect_verify=ok mutation=detected "
        "missing=detected no_summary=fails monitor=ok"
    )
    return 0


def parse():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--collect", metavar="RUN_DIR")
    parser.add_argument("--verify", metavar="EVIDENCE_JSON")
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--duration", type=int, default=600)
    parser.add_argument("--interval", type=int, default=10)
    parser.add_argument("--stall", type=float, default=300.0)
    parser.add_argument("--port")
    parser.add_argument("--out", default="build/tcp-soak")
    parser.add_argument("--image")
    return parser.parse_args()


def main():
    args = parse()
    if args.selftest:
        return selftest()
    if args.collect:
        evidence, error = collect(args.collect, image=args.image or None)
        if error is not None:
            print(f"tcp-soak-evidence: FAIL {error}", file=sys.stderr)
            return 1
        print(
            f"tcp-soak-evidence: evidence={Path(args.collect) / 'evidence.json'} "
            f"artifacts={len(evidence['artifacts'])}"
        )
        return 0
    if args.verify:
        ok, problems = verify(args.verify)
        if not ok:
            for problem in problems:
                print(f"tcp-soak-evidence: VERIFY FAIL {problem}", file=sys.stderr)
            return 1
        print(f"tcp-soak-evidence: verify PASS {args.verify}")
        return 0
    if args.run:
        if args.duration < 1 or args.interval < 1:
            print("tcp-soak-evidence: duration and interval must be positive", file=sys.stderr)
            return EXIT_USAGE
        return run_soak(args)
    print(
        "tcp-soak-evidence: choose --selftest, --run, --collect RUN_DIR or --verify FILE",
        file=sys.stderr,
    )
    return EXIT_USAGE


if __name__ == "__main__":
    sys.exit(main())
