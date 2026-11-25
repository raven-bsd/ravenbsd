#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Shared host runner for the conformance tables (S172).

A topic directory under ``tests/conformance/<topic>/`` supplies:

* ``rows.tsv``       -- ``name<TAB>requirement<TAB>gate`` for every normal row;
* ``mutations.tsv``  -- ``mutation<TAB>row<TAB>requirement`` for every controlled mutation;
* ``build.sh``       -- builds the guest probe (the probe path is a constant below);
* ``<name>.service.toml`` -- the one-shot service that runs the probe with ``--all``.

The runner builds the probe, packs the selected profile's initfs, overlays the probe and a
one-shot service, lets ``cargo xtask qtest`` boot the image, then parses the row contract from
the serial log and owns the final exit mapping.  A row failure is a row result, not an
ambiguous qtest exit status.  The same runner is reused by every topic; the topic only supplies
its probe and tables.

Exit codes: 0 all gate rows and expected mutations behaved, 1 a gate row failed or a mutation
was not caught, 2 the harness/image/execution failed (never reported as a pass).

Usage:
  python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors
  python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors \
      --phase mutation --mutation dup-copies-offset
  python3 tests/conformance/harness/runner.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HARNESS = ROOT / "tests/conformance/harness"
PROBE_GUEST = "/bin/conformance-probe"
ROW_RE = re.compile(r"^CONFORM (?P<topic>\S+) row=(?P<name>\S+) status=(?P<status>PASS|FAIL)")
SUMMARY_RE = re.compile(
    r"^CONFORM (?P<topic>\S+) summary passed=(?P<passed>\d+) total=(?P<total>\d+) "
    r"failures=(?P<failures>\d+)"
)
DONE_RE = re.compile(r"^CONFORM (?P<topic>\S+) done failures=(?P<failures>\d+)")
MUTATION_BEGIN_RE = re.compile(
    r"^CONFORM-MUTATION topic=(?P<topic>\S+) begin=(?P<name>\S+)"
)
MUTATION_END_RE = re.compile(
    r"^CONFORM-MUTATION topic=(?P<topic>\S+) end=(?P<name>\S+) failures=(?P<failures>\d+)"
)


class HarnessError(Exception):
    """An image/build/execution failure, never a row verdict."""


def read_table(path: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise HarnessError(f"{path}:{line_number}: expected three tab-separated fields")
        rows.append({"name": fields[0], "requirement": fields[1], "gate": fields[2]})
    if not rows:
        raise HarnessError(f"{path}: empty table")
    return rows


def read_mutations(path: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise HarnessError(f"{path}:{line_number}: expected three tab-separated fields")
        rows.append({"name": fields[0], "row": fields[1], "requirement": fields[2]})
    if not rows:
        raise HarnessError(f"{path}: empty mutation table")
    return rows


def run_step(label: str, argv: list[str], timeout: int) -> tuple[int, str]:
    print(f"conformance: step={label} argv={argv} timeout={timeout}", flush=True)
    wrapped = ["timeout", str(timeout), *argv]
    try:
        completed = subprocess.run(
            wrapped,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    except OSError as error:
        raise HarnessError(f"{label}: cannot start {argv[0]}: {error}") from error
    output = completed.stdout or ""
    print(output, end="" if output.endswith("\n") else "\n", flush=True)
    return completed.returncode, output


def parse_log(text: str, topic: str) -> dict[str, object]:
    """Split a serial log into the normal section and each mutation section."""
    normal_rows: dict[str, str] = {}
    normal_done: int | None = None
    mutations: dict[str, dict[str, object]] = {}
    current: dict[str, str] | None = normal_rows
    current_name: str | None = None
    current_rows: dict[str, str] = normal_rows
    for line in text.splitlines():
        begin = MUTATION_BEGIN_RE.match(line)
        if begin is not None and begin.group("topic") == topic:
            current_name = begin.group("name")
            current_rows = {}
            mutations[current_name] = {"rows": current_rows, "done": None}
            current = None
            continue
        end = MUTATION_END_RE.match(line)
        if end is not None and end.group("topic") == topic:
            if current_name is None or current_name != end.group("name"):
                raise HarnessError(f"mutation end without matching begin: {line}")
            mutations[current_name]["done"] = int(end.group("failures"))
            current_name = None
            current = normal_rows
            continue
        row = ROW_RE.match(line)
        if row is not None and row.group("topic") == topic:
            target = current_rows if current_name is not None else normal_rows
            target[row.group("name")] = row.group("status")
            continue
        done = DONE_RE.match(line)
        if done is not None and done.group("topic") == topic and current_name is None:
            normal_done = int(done.group("failures"))
    return {
        "rows": normal_rows,
        "done": normal_done,
        "mutations": mutations,
    }


def evaluate(
    parsed: dict[str, object],
    rows: list[dict[str, str]],
    mutations: list[dict[str, str]],
    phase: str,
    mutation: str | None,
) -> list[str]:
    """Return failure reasons; empty means the selected phase behaved as specified."""
    reasons: list[str] = []
    normal_rows: dict[str, str] = parsed["rows"]  # type: ignore[assignment]
    if phase == "normal":
        for row in rows:
            if row["gate"] != "1":
                continue
            status = normal_rows.get(row["name"])
            if status != "PASS":
                reasons.append(f"row:{row['name']}={status or 'missing'}")
        if parsed["done"] != 0:
            reasons.append(f"normal:done={parsed['done']}")
        for expected in mutations:
            section = parsed["mutations"].get(expected["name"])  # type: ignore[union-attr]
            if section is None:
                reasons.append(f"mutation:{expected['name']}=missing-section")
                continue
            status = section["rows"].get(expected["row"])
            if status != "FAIL":
                reasons.append(f"mutation:{expected['name']}:{expected['row']}={status}")
            if section["done"] is None:
                reasons.append(f"mutation:{expected['name']}:end=missing")
        return reasons

    expected = next((item for item in mutations if item["name"] == mutation), None)
    if expected is None:
        return [f"mutation:{mutation}=unknown"]
    section = parsed["mutations"].get(mutation)  # type: ignore[union-attr]
    if section is None:
        return [f"mutation:{mutation}=missing-section"]
    status = section["rows"].get(expected["row"])
    # The controlled mutation is caught only when the row it targets turns red.
    if status != "FAIL":
        reasons.append(f"mutation:{mutation}:{expected['row']}={status or 'missing'}")
    if section["done"] is None:
        reasons.append(f"mutation:{mutation}:end=missing")
    return reasons


def self_test() -> int:
    """Host-only check of the log parser and exit mapping, with no QEMU."""
    rows = [
        {"name": "row-a", "requirement": "T-R01", "gate": "1"},
        {"name": "row-b", "requirement": "T-R01", "gate": "1"},
    ]
    mutations = [{"name": "mut-a", "row": "row-a", "requirement": "T-R01"}]
    good = "\n".join(
        [
            "CONFORM demo begin",
            "CONFORM demo row=row-a status=PASS detail=a=1",
            "CONFORM demo row=row-b status=PASS detail=b=2",
            "CONFORM demo summary passed=2 total=2 failures=0",
            "CONFORM demo done failures=0",
            "CONFORM-MUTATION topic=demo begin=mut-a",
            "CONFORM demo row=row-a status=FAIL detail=a=0",
            "CONFORM demo summary passed=0 total=1 failures=1",
            "CONFORM-MUTATION topic=demo end=mut-a failures=1",
        ]
    )
    bad = good.replace("row=row-b status=PASS", "row=row-b status=FAIL")
    undetected = good.replace(
        "CONFORM demo row=row-a status=FAIL", "CONFORM demo row=row-a status=PASS"
    )
    failures: list[str] = []
    if evaluate(parse_log(good, "demo"), rows, mutations, "normal", None):
        failures.append("self-test: good normal log rejected")
    if not evaluate(parse_log(bad, "demo"), rows, mutations, "normal", None):
        failures.append("self-test: failing gate row accepted")
    if evaluate(parse_log(good, "demo"), rows, mutations, "mutation", "mut-a"):
        failures.append("self-test: caught mutation not accepted")
    if not evaluate(parse_log(undetected, "demo"), rows, mutations, "mutation", "mut-a"):
        failures.append("self-test: undetected mutation accepted")
    if not evaluate(parse_log("", "demo"), rows, mutations, "normal", None):
        failures.append("self-test: empty log accepted")
    if failures:
        for failure in failures:
            print(f"conformance-self-test: FAIL {failure}", flush=True)
        return 1
    print("conformance-self-test: PASS", flush=True)
    return 0


def write_case(
    topic: str,
    name: str,
    profile: str,
    initfs: Path,
    timeout: int,
    serial: Path,
) -> Path:
    case = serial
    text = "\n".join(
        [
            "# SPDX-License-Identifier: BSD-3-Clause",
            "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
            "# Generated by tests/conformance/harness/runner.py.",
            f'name = "{name}"',
            'arch = "x86_64"',
            f'profile = "{profile}"',
            f"timeout_secs = {timeout}",
            f'initfs = "{initfs.relative_to(ROOT)}"',
            'boot_cmdline = "console=ttyS0 test-exit=1"',
            'expect_exit = "any"',
            "",
            "[[expect]]",
            f"regex = '^CONFORM {topic} begin$'",
            "",
            "[[expect]]",
            f"regex = '^CONFORM {topic} done failures=[0-9]+$'",
            "",
            "[[forbid]]",
            "regex = '(?i)panic|EXCEPTION'",
            "",
        ]
    )
    case.write_text(text, encoding="utf-8")
    return case


def build_and_run(args: argparse.Namespace, topic_dir: Path, topic: str, rows: list[dict[str, str]],
                  mutations: list[dict[str, str]]) -> int:
    out = ROOT / "build" / "conformance" / topic
    out.mkdir(parents=True, exist_ok=True)
    profile = args.profile
    build = topic_dir / "build.sh"
    service = topic_dir / "descriptor-conformance.service.toml"
    probe = ROOT / "build" / "conformance" / topic / "conformance-probe"
    if not build.is_file():
        raise HarnessError(f"{topic}: missing {build}")
    if not service.is_file():
        raise HarnessError(f"{topic}: missing {service}")

    code, _ = run_step("probe-build", ["sh", str(build.relative_to(ROOT))], 240)
    if code != 0:
        raise HarnessError(f"probe build failed rc={code}")
    if not probe.is_file():
        raise HarnessError(f"probe build produced no {probe}")

    code, _ = run_step(
        "initfs", ["cargo", "xtask", "initfs", "--arch", "x86_64", "--profile", profile], 900
    )
    if code != 0:
        raise HarnessError(f"initfs build failed rc={code}")

    base = ROOT / "build" / "x86_64" / "initfs.img"
    overlay = out / "initfs.img"
    svcd = out / "svcd.conf"
    svcd.write_text(
        "# SPDX-License-Identifier: BSD-3-Clause\n"
        "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n"
        "# Generated by tests/conformance/harness/runner.py.\n"
        f"exit_when_exits = {args.service}\n",
        encoding="utf-8",
    )
    code, _ = run_step(
        "overlay",
        [
            "target/debug/raven-b70-overlay",
            str(base.relative_to(ROOT)),
            str(overlay.relative_to(ROOT)),
            f"{PROBE_GUEST}={probe.relative_to(ROOT)}:0755",
            f"/etc/services.d/{args.service}.toml={service.relative_to(ROOT)}:0644",
            f"/etc/svcd.conf={svcd.relative_to(ROOT)}:0644",
        ],
        120,
    )
    if code != 0:
        raise HarnessError(f"overlay failed rc={code}")

    serial = out / f"{topic}.toml"
    write_case(topic, f"{topic}-conformance", profile, overlay, args.guest_timeout, serial)
    code, output = run_step(
        "qtest",
        ["cargo", "xtask", "qtest", "--case", str(serial.relative_to(ROOT))],
        args.guest_timeout + args.build_margin,
    )
    log = ROOT / "build" / "qtest" / f"{topic}-conformance.log"
    if not log.is_file():
        raise HarnessError(f"qtest produced no serial log at {log}")
    text = log.read_text(encoding="utf-8", errors="replace")
    (out / "serial.log").write_text(text, encoding="utf-8")
    if code != 0:
        tail = "\n".join(output.splitlines()[-20:])
        raise HarnessError(f"qtest failed rc={code}: {tail}")

    parsed = parse_log(text, topic)
    reasons = evaluate(parsed, rows, mutations, args.phase, args.mutation)
    summary = {
        "topic": topic,
        "phase": args.phase,
        "mutation": args.mutation,
        "rows": parsed["rows"],
        "mutations": {
            key: value["rows"] for key, value in parsed["mutations"].items()  # type: ignore[union-attr]
        },
        "reasons": reasons,
    }
    (out / "result.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    if reasons:
        print(f"conformance: FAIL reasons={';'.join(reasons)}", flush=True)
        if args.phase == "mutation":
            infra = any(
                marker in reason
                for reason in reasons
                for marker in ("missing-section", "unknown", "end=missing")
            )
            if infra:
                return 2
            # The targeted row stayed green: the mutation was not caught, so the test passed.
            print("conformance: MUTATION-NOT-CAUGHT", flush=True)
            return 0
        return 1
    print(
        f"conformance: PASS topic={topic} phase={args.phase} rows={len(parsed['rows'])} "
        f"mutations={len(parsed['mutations'])}",
        flush=True,
    )
    if args.phase == "mutation":
        # The row did turn red: that is the expected test result for a mutation command.
        print("conformance: MUTATION-CAUGHT", flush=True)
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--topic", help="topic directory, e.g. tests/conformance/descriptors")
    parser.add_argument("--profile", default="release")
    parser.add_argument("--service", default="descriptor-conformance")
    parser.add_argument("--phase", choices=("normal", "mutation"), default="normal")
    parser.add_argument("--mutation", help="mutation name for --phase mutation")
    parser.add_argument("--guest-timeout", type=int, default=180)
    parser.add_argument("--build-margin", type=int, default=420)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.topic:
        parser.error("--topic is required unless --self-test")
    if args.phase == "mutation" and not args.mutation:
        parser.error("--phase mutation requires --mutation")
    topic_dir = ROOT / args.topic
    if not topic_dir.is_dir():
        parser.error(f"topic directory not found: {topic_dir}")
    topic = topic_dir.name
    try:
        rows = read_table(topic_dir / "rows.tsv")
        mutations = read_mutations(topic_dir / "mutations.tsv")
        return build_and_run(args, topic_dir, topic, rows, mutations)
    except HarnessError as error:
        print(f"conformance: HARNESS-ERROR {error}", file=sys.stderr, flush=True)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
