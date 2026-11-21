#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K275 scenario-matrix runner for the K253 packet-level TCP harness.

The runner keeps the wire scenarios in one place and can point them at:

* the current checkout (``--image build/qtest/net-conformance.img``);
* a ready image from another lane (``--image /path/to/net-conformance.img``);
* a git branch or ref (``--branch origin/kevin/K271``); the script creates a
  detached worktree, overlays the test-only guest fixture if that branch
  predates K253, builds its image, then runs the same host-side scenarios;
* an existing worktree (``--worktree /path``), which is useful when a port lane
  wants to keep its own build directory.

Every scenario is run through ``scripts/net-conformance.sh`` so the timeout,
namespace, tap and RST-filter rules stay in one place.  The runner writes one
pcap, one guest serial log and one command log per scenario under
``tests/net-conformance/results/k275/<label>/`` and a summary JSON/Markdown.
It changes no product source.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RESULTS_ROOT = ROOT / "tests" / "net-conformance" / "results" / "k275"
SCENARIOS = (
    "handshake",
    "zero-window",
    "window-update",
    "retransmit",
    "fin-rst",
    "ecn",
    "pmtu-blackhole",
    "plpmtud",
)
RESULT_RE = re.compile(
    r"net-conformance:\s+(?P<scenario>\S+)\s+(?P<result>pass|fail)\s+"
    r"\((?P<seconds>[^)]*)\)\s*(?P<detail>.*)"
)


def log(message: str) -> None:
    print(f"net-conformance-matrix: {message}", flush=True)


def sanitize(label: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", label.strip()).strip("-")
    return cleaned or "image"


def image_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_capped(command: list[str], log_path: Path, timeout: int) -> tuple[int, str]:
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        output = completed.stdout + completed.stderr
        code = completed.returncode
    except subprocess.TimeoutExpired as error:
        output = (error.stdout or "") + (error.stderr or "")
        if isinstance(output, bytes):
            output = output.decode("utf-8", "replace")
        output += f"\nmatrix: TIMEOUT after {timeout}s\n"
        code = 124
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text(output, encoding="utf-8")
    elapsed = time.monotonic() - started
    log(f"{' '.join(command)} -> exit {code} ({elapsed:.1f}s)")
    return code, output


def build_image(args: argparse.Namespace, log_path: Path) -> tuple[Path | None, str]:
    if args.image:
        image = Path(args.image)
        if image.is_file():
            return image, f"image {image}"
        return None, f"image {image} is not a file"
    command = ["sh", "scripts/net-conformance.sh", "--build-only"]
    if args.branch:
        command += ["--branch", args.branch]
        target = args.branch
    elif args.worktree:
        command += ["--worktree", str(args.worktree)]
        target = args.worktree
    else:
        command += []
        target = "current checkout"
    code, output = run_capped(command, log_path, args.build_timeout)
    if code != 0:
        return None, f"build failed for {target} (exit {code})"
    image = None
    for line in output.splitlines():
        if line.startswith("NETCONF_IMAGE="):
            image = Path(line.split("=", 1)[1].strip())
    if image is None:
        return None, f"build for {target} did not print NETCONF_IMAGE"
    return image, f"built {target}"


def parse_result(scenario: str, output: str, code: int) -> dict:
    match = None
    for candidate in RESULT_RE.finditer(output):
        if candidate.group("scenario") == scenario:
            match = candidate
    if match is None:
        if code == 77:
            return {"result": "narrowed", "detail": "scenario narrowed", "seconds": ""}
        return {"result": "error", "detail": f"no scenario result line (exit {code})", "seconds": ""}
    return {
        "result": match.group("result"),
        "detail": match.group("detail").strip(),
        "seconds": match.group("seconds").strip(),
    }


def write_summary(label: str, image: Path, commit: str, results: list[dict],
                  directory: Path) -> None:
    summary = {
        "label": label,
        "commit": commit,
        "image": str(image),
        "image_sha256": image_sha256(image),
        "scenarios": results,
    }
    (directory / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf-8"
    )
    lines = [
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->",
        "<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->",
        "",
        f"# K275 matrix: {label}",
        "",
        f"* commit: `{commit}`",
        f"* image: `{image}`",
        f"* image sha256: `{summary['image_sha256']}`",
        "",
        "| Scenario | Result | Detail | pcap | serial |",
        "|---|---|---|---|---|",
    ]
    for item in results:
        lines.append(
            f"| `{item['scenario']}` | {item['result']} | {item['detail']} | "
            f"`{item['pcap']}` | `{item['serial']}` |"
        )
    (directory / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def selftest() -> int:
    assert sanitize("origin/kevin/K271") == "origin-kevin-K271"
    assert sanitize("feature/name with spaces") == "feature-name-with-spaces"
    assert sanitize("") == "image"
    assert set(SCENARIOS) == {
        "handshake", "zero-window", "window-update", "retransmit", "fin-rst"
    }
    sample = (
        "net-conformance: zero-window fail (15.99s) probe=1B seq=1; "
        "after-reopen=43B; total=44/44; replayed-probe=yes\n"
    )
    parsed = parse_result("zero-window", sample, 1)
    assert parsed["result"] == "fail"
    assert "replayed-probe=yes" in parsed["detail"]
    log("selftest PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--branch", help="git ref to build in a detached worktree")
    parser.add_argument("--worktree", type=Path, help="existing worktree to build")
    parser.add_argument("--image", help="ready net-conformance image")
    parser.add_argument("--commit", help="source commit to record (default: selected checkout HEAD)")
    parser.add_argument("--label", help="results directory name (default: target name)")
    parser.add_argument("--scenario", action="append", choices=SCENARIOS, default=[])
    parser.add_argument("--results-root", type=Path, default=RESULTS_ROOT)
    parser.add_argument("--build-timeout", type=int, default=900)
    parser.add_argument("--scenario-timeout", type=int, default=180)
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    if args.branch and args.worktree:
        parser.error("--branch and --worktree are mutually exclusive")
    if args.branch and args.image:
        parser.error("--branch and --image are mutually exclusive")
    if not (args.branch or args.worktree or args.image):
        # Default: build the current checkout in place; the matrix still records
        # a label and an image hash.
        pass

    label = sanitize(args.label or args.branch or (
        str(args.worktree) if args.worktree else (
            Path(args.image).stem if args.image else "current"
        )
    ))
    directory = args.results_root / label
    directory.mkdir(parents=True, exist_ok=True)

    image, note = build_image(args, directory / "build.log")
    if image is None:
        log(note)
        return 1
    log(note)
    if args.commit:
        commit = args.commit
    elif args.branch:
        commit = subprocess.check_output(
            ["git", "rev-parse", args.branch], cwd=ROOT, text=True
        ).strip()
    elif args.worktree:
        commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=args.worktree, text=True
        ).strip()
    else:
        commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()

    scenarios = args.scenario or list(SCENARIOS)
    results = []
    for scenario in scenarios:
        pcap = directory / f"{scenario}.pcap"
        serial = directory / f"{scenario}-serial.log"
        command = [
            "sh", "scripts/net-conformance.sh",
            "--target", "raven",
            "--no-build",
            "--image", str(image),
            "--scenario", scenario,
            "--pcap", str(pcap),
            "--serial", str(serial),
        ]
        code, output = run_capped(
            command, directory / f"{scenario}.log", args.scenario_timeout
        )
        parsed = parse_result(scenario, output, code)
        try:
            pcap_path = str(pcap.relative_to(ROOT))
            serial_path = str(serial.relative_to(ROOT))
        except ValueError:
            pcap_path = str(pcap)
            serial_path = str(serial)
        results.append({
            "scenario": scenario,
            "exit": code,
            "pcap": pcap_path,
            "serial": serial_path,
            **parsed,
        })
        log(f"{scenario}: {parsed['result']} {parsed['detail']}")
    write_summary(label, image, commit, results, directory)
    failures = [item for item in results if item["result"] != "pass"]
    log(f"summary {directory / 'summary.md'}")
    return 1 if failures else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        import traceback

        traceback.print_exc()
        print(f"net-conformance-matrix: ERROR {error}", file=sys.stderr)
        sys.exit(2)
