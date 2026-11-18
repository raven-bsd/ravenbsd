#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Collect and verify the RavenBSD 0.1.0 release evidence packet.

The collector records the current commit, the full `make check`, the host-only POSIX report, the
libc gate verdict/rows, the recorded kernel gate, the dynamic-userland logs, the release-login
transcript, image hashes and the Pi 4 board status. It never turns a missing, stale or virt/synthetic
artifact into a PASS. `--self-test` exercises the refusal logic with the fixtures under
`tests/release/evidence/fixtures/`.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VERSION = "0.1.0"
PACKET = ROOT / "docs/releases/0.1.0-evidence.md"
EVIDENCE = ROOT / "build/release" / VERSION
KERNEL_GATE = ROOT / "docs/kernel-gate.md"
POSIX_REPORT = ROOT / "docs/posix/CONFORMANCE.md"
RELEASE_IMAGES = ROOT / "docs/release-images.md"
PI4_DEFAULT = EVIDENCE / "pi4-serial.log"
IMAGE_X86_DEFAULT = ROOT / "build/x86_64/ravenbsd-x86_64.img"
IMAGE_AARCH64_DEFAULT = ROOT / "build/aarch64/ravenbsd-pi4.img"
MAX_FILE_BYTES = 16 * 1024 * 1024
HEX_COMMIT = re.compile(r"^[0-9a-f]{7,40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


@dataclass
class Gate:
    name: str
    status: str
    command: str
    started: str
    ended: str
    exit_code: str
    evidence: str
    sha256: str
    note: str
    required: bool = True


@dataclass
class Run:
    command: list[str]
    started: str
    ended: str
    code: int
    log: Path


def utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256(path: Path) -> str:
    if not path.is_file():
        return ""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_text(path: Path) -> str:
    if not path.is_file() or path.stat().st_size > MAX_FILE_BYTES:
        return ""
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip()


def head_commit() -> str:
    return git("rev-parse", "HEAD")


def tree_dirty() -> bool:
    return bool(git("status", "--porcelain"))


def commit_matches(recorded: str, head: str) -> bool:
    return bool(recorded and head) and (recorded.startswith(head) or head.startswith(recorded))


def commit_is_current(recorded: str, head: str) -> bool:
    """Accept the recorded commit when it is HEAD or an ancestor of HEAD.

    A generated report cannot contain the commit that adds it, so the stamp is
    normally the parent of the commit that ships the file.
    """
    if not HEX_COMMIT.match(recorded or "") or not HEX_COMMIT.match(head or ""):
        return False
    if commit_matches(recorded, head):
        return True
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", recorded, head],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode == 0


def run_command(argv: list[str], log: Path) -> Run:
    log.parent.mkdir(parents=True, exist_ok=True)
    started = utc()
    with log.open("w", encoding="utf-8") as handle:
        result = subprocess.run(
            argv,
            cwd=ROOT,
            stdout=handle,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    return Run(argv, started, utc(), result.returncode, log)


def run_gate(name: str, argv: list[str], head: str, dirty: bool) -> Gate:
    log = EVIDENCE / f"{name}.log"
    result = run_command(argv, log)
    if result.code != 0:
        status = "FAIL"
    elif dirty:
        status = "DIRTY"
    else:
        status = "PASS"
    return Gate(
        name=name,
        status=status,
        command=" ".join(argv),
        started=result.started,
        ended=result.ended,
        exit_code=str(result.code),
        evidence=str(log.relative_to(ROOT)),
        sha256=sha256(log),
        note="exit 0; clean tree" if status == "PASS" else f"exit {result.code}; tree {'dirty' if dirty else 'clean'}",
    )


def parse_kernel_gate(path: Path, head: str) -> Gate:
    text = read_text(path)
    if not text:
        return Gate(
            "kernel-gate", "MISSING", "read docs/kernel-gate.md", utc(), utc(), "",
            str(path.relative_to(ROOT)), "", "recorded gate document is missing"
        )
    current = re.search(r"Generated for current master `([0-9a-f]{7,40})`", text)
    full = re.search(r"\(([0-9a-f]{40})\)", text)
    legacy = re.search(
        r"Generated by .*? at (\S+) from commit `([0-9a-f]{7,40})`", text
    )
    recorded = ""
    if current:
        recorded = current.group(1)
    elif full:
        recorded = full.group(1)
    elif legacy:
        recorded = legacy.group(2)
    ran_at = re.search(r"\bat (\d{4}-\d{2}-\d{2}T\S+Z)\b", text)
    stamp = ran_at.group(1) if ran_at else (legacy.group(1) if legacy else "")
    passes = len(re.findall(r"\|\s*\*\*pass\*\*\s*\|", text))
    result = "PASS" if re.search(r"Result: \*\*PASS\*\*", text) else "FAIL"
    if not recorded:
        return Gate(
            "kernel-gate", "MISSING", "read docs/kernel-gate.md", stamp, utc(), "",
            str(path.relative_to(ROOT)), sha256(path), "document has no commit stamp"
        )
    if result != "PASS" or passes != 27:
        status = "FAIL"
        note = f"result={result}; {passes}/27 pass"
    elif not commit_is_current(recorded, head):
        status = "STALE"
        note = f"recorded at {recorded}; HEAD is {head}; {passes}/27 pass recorded"
    else:
        status = "PASS"
        note = f"27/27 pass at {recorded} (current)"
    return Gate(
        "kernel-gate", status, "read docs/kernel-gate.md", stamp, utc(), "",
        str(path.relative_to(ROOT)), sha256(path), note
    )


def parse_posix(path: Path, head: str) -> Gate:
    text = read_text(path)
    if not text:
        return Gate(
            "posix", "MISSING", "read docs/posix/CONFORMANCE.md", utc(), utc(), "",
            str(path.relative_to(ROOT)), "", "host-only report is missing"
        )
    date = re.search(r"^- Date: (\S+)", text, re.MULTILINE)
    result = re.search(r"^- Result: \*\*(.+?)\*\*", text, re.MULTILINE)
    counts = re.search(r"^- Counts: (\d+) pass, (\d+) fail, (\d+) skip", text, re.MULTILINE)
    commit = re.search(r"^- Commit: `?([0-9a-fA-F]{7,40})`?", text, re.MULTILINE)
    stamp = date.group(1) if date else ""
    verdict = result.group(1) if result else "UNKNOWN"
    count = counts.group(0).removeprefix("- Counts: ") if counts else "unknown"
    recorded = commit.group(1) if commit else ""
    green_counts = bool(counts) and int(counts.group(2)) == 0 and int(counts.group(1)) > 0
    if "PASS" not in verdict:
        status = "FAIL"
        note = f"{count}; {verdict}"
    elif not green_counts:
        status = "STALE"
        note = f"{count}; {verdict}; counts are not green, rerun required"
    elif not recorded:
        status = "STALE"
        note = f"{count}; {verdict}; report has no commit stamp, rerun required"
    elif commit_is_current(recorded, head):
        status = "PASS"
        note = f"{count}; {verdict}; commit {recorded[:12]} is current"
    else:
        status = "STALE"
        note = (
            f"{count}; {verdict}; recorded commit {recorded[:12]} is not HEAD "
            f"{head[:12] or 'unknown'}; rerun required"
        )
    return Gate(
        "posix", status, "read docs/posix/CONFORMANCE.md", stamp, utc(), "",
        str(path.relative_to(ROOT)), sha256(path), note
    )


def parse_libc(head: str) -> Gate:
    log = EVIDENCE / "libc-verdict.log"
    result = run_command(["cargo", "run", "-q", "-p", "libc-gate", "--", "verdict"], log)
    text = read_text(log)
    verdict = re.search(r"^libc gate: (.+)$", text, re.MULTILINE)
    status_text = verdict.group(1) if verdict else ""
    met = len(re.findall(r"\|\s*MET\s*\|", text))
    not_met = len(re.findall(r"\|\s*NOT MET\s*\|", text))
    blocked = len(re.findall(r"\|\s*BLOCKED\s*\|", text))
    if result.code == 0 and status_text == "MET":
        status = "PASS"
    elif status_text in {"NOT MET", "MET"}:
        status = "FAIL" if status_text != "MET" else "PASS"
    else:
        status = "MISSING"
    note = f"{status_text or 'no verdict'}; MET={met} NOT MET={not_met} BLOCKED={blocked}"
    return Gate(
        "libc-gate", status, "cargo run -q -p libc-gate -- verdict",
        result.started, result.ended, str(result.code),
        str(log.relative_to(ROOT)), sha256(log), note
    )


def parse_transcript_commit(path: Path) -> str:
    text = read_text(path)
    match = re.search(r"RavenBSD kernel 0\.1 \([^)]+\) ([0-9a-f]{7,40})(?:-dirty)?", text)
    return match.group(1) if match else ""


def check_transcript(path: Path) -> bool:
    if not path.is_file():
        return False
    result = subprocess.run(
        ["sh", "tests/fixtures/release/check-transcript.sh", str(path)],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    return result.returncode == 0


def release_login_gate(head: str, run_qtests: bool) -> Gate:
    transcript = ROOT / "build/release/qemu-release.log"
    fallback = ROOT / "build/qtest/release-login.log"
    if not transcript.is_file() and fallback.is_file():
        transcript = fallback
    if run_qtests:
        log = EVIDENCE / "release-login.log"
        result = run_command(["sh", "scripts/qemu-release.sh"], log)
        code = str(result.code)
        if result.code != 0:
            return Gate(
                "release-login", "FAIL", "sh scripts/qemu-release.sh",
                result.started, result.ended, code, str(log.relative_to(ROOT)),
                sha256(log), f"exit {result.code}"
            )
    else:
        log = transcript
        code = "recorded"
        if not transcript.is_file():
            return Gate(
                "release-login", "MISSING", "read build/qtest/release-login.log",
                utc(), utc(), "recorded", str(transcript.relative_to(ROOT)), "",
                "release-login transcript is missing; run --run-qtests"
            )
    recorded = parse_transcript_commit(transcript)
    if not recorded:
        return Gate(
            "release-login", "MISSING", "read release-login transcript", utc(), utc(),
            code, str(transcript.relative_to(ROOT)), sha256(transcript),
            "transcript has no kernel commit banner"
        )
    if not commit_matches(recorded, head):
        return Gate(
            "release-login", "STALE", "read release-login transcript", utc(), utc(),
            code, str(transcript.relative_to(ROOT)), sha256(transcript),
            f"recorded at {recorded}; HEAD is {head}"
        )
    if not check_transcript(transcript):
        return Gate(
            "release-login", "FAIL", "tests/fixtures/release/check-transcript.sh", utc(), utc(),
            code, str(transcript.relative_to(ROOT)), sha256(transcript),
            "transcript shape check failed"
        )
    return Gate(
        "release-login", "PASS", "release-login transcript", utc(), utc(), code,
        str(transcript.relative_to(ROOT)), sha256(transcript),
        f"transcript at {recorded}"
    )


def dynamic_gate(arch: str, head: str, run_qtests: bool) -> Gate:
    name = f"dynamic-userland-{arch}"
    evidence = EVIDENCE / f"{name}.log"
    qtest = ROOT / "build/qtest/dynamic-userland.log"
    if run_qtests:
        result = run_command(
            ["cargo", "xtask", "qtest", "--case", "dynamic-userland", "--arch", arch],
            evidence,
        )
        if qtest.is_file():
            evidence.write_bytes(qtest.read_bytes())
        code = str(result.code)
        if result.code != 0:
            return Gate(
                name, "FAIL", f"cargo xtask qtest --case dynamic-userland --arch {arch}",
                result.started, result.ended, code, str(evidence.relative_to(ROOT)),
                sha256(evidence), f"exit {result.code}"
            )
    else:
        code = "recorded"
        if not evidence.is_file():
            return Gate(
                name, "MISSING", f"read build/qtest/dynamic-userland.log",
                utc(), utc(), "recorded", str(evidence.relative_to(ROOT)), "",
                f"no {arch} log; run --run-qtests"
            )
    recorded = parse_transcript_commit(evidence)
    if not recorded:
        return Gate(
            name, "MISSING", "read dynamic-userland log", utc(), utc(), code,
            str(evidence.relative_to(ROOT)), sha256(evidence),
            "log has no kernel commit banner"
        )
    if not commit_matches(recorded, head):
        return Gate(
            name, "STALE", "read dynamic-userland log", utc(), utc(), code,
            str(evidence.relative_to(ROOT)), sha256(evidence),
            f"recorded at {recorded}; HEAD is {head}"
        )
    return Gate(
        name, "PASS", "dynamic-userland log", utc(), utc(), code,
        str(evidence.relative_to(ROOT)), sha256(evidence),
        f"transcript at {recorded}"
    )


def release_images(path: Path) -> tuple[str, dict[str, str]]:
    """Read the recorded product commit and image hashes from the IMG1 document."""
    text = read_text(path)
    commit = re.search(r"\|\s*Product commit\s*\|\s*`([0-9a-f]{7,40})`", text)
    if not commit:
        commit = re.search(r"product commit `([0-9a-f]{7,40})`", text, re.IGNORECASE)
    hashes: dict[str, str] = {}
    sections = (
        ("x86_64", r"## 2\. x86_64 release image"),
        ("aarch64", r"## 3\. Pi 4 FAT\+UFS SD image"),
    )
    for kind, heading in sections:
        match = re.search(heading, text)
        if not match:
            continue
        rest = text[match.end():]
        end = re.search(r"\n## ", rest)
        section = rest[: end.start()] if end else rest
        digest = re.search(r"`([0-9a-f]{64})`", section)
        if digest:
            hashes[kind] = digest.group(1)
    return (commit.group(1) if commit else ""), hashes


def image_kind(name: str, path: Path) -> str:
    text = f"{name} {path}".lower()
    if "aarch64" in text or "pi4" in text:
        return "aarch64"
    if "x86" in text:
        return "x86_64"
    return ""


def image_gate(name: str, path: Path, head: str, images_doc: Path = RELEASE_IMAGES) -> Gate:
    digest = sha256(path) if path.is_file() else ""
    recorded_commit, recorded_hashes = release_images(images_doc)
    recorded = recorded_hashes.get(image_kind(name, path), "")
    if recorded and recorded_commit and commit_is_current(recorded_commit, head):
        note = f"sha256={recorded}; product commit={recorded_commit} (current)"
        if digest and digest != recorded:
            note += f"; local build sha256={digest}"
        return Gate(
            name, "PINNED", "read docs/release-images.md", utc(), utc(), "",
            str(images_doc.relative_to(ROOT)), recorded, note
        )
    if not path.is_file():
        if recorded and recorded_commit:
            note = (
                f"recorded sha256={recorded}; product commit={recorded_commit} is not "
                f"HEAD {head[:12] or 'unknown'}"
            )
            return Gate(
                name, "UNPINNED", "read docs/release-images.md", utc(), utc(), "",
                str(images_doc.relative_to(ROOT)), recorded, note
            )
        return Gate(
            name, "MISSING", f"sha256 {path.relative_to(ROOT)}", utc(), utc(), "",
            str(path.relative_to(ROOT)), "", "image is missing"
        )
    manifest = path.parent / "manifest.txt"
    manifest_commit = ""
    if manifest.is_file():
        match = re.search(r"^commit=([0-9a-f]{7,40})(?:-dirty)?$", read_text(manifest), re.MULTILINE)
        manifest_commit = match.group(1) if match else ""
    if manifest_commit and commit_is_current(manifest_commit, head):
        status = "PASS"
        note = f"sha256={digest}; manifest commit={manifest_commit}"
    else:
        status = "UNPINNED"
        note = (
            f"sha256={digest}; manifest commit={manifest_commit or 'missing'}; "
            f"release-images commit={recorded_commit or 'missing'}"
            + (f" sha256={recorded}" if recorded else "")
        )
    return Gate(
        name, status, f"sha256 {path.relative_to(ROOT)}", utc(), utc(), "",
        str(path.relative_to(ROOT)), digest, note
    )


def board_gate(log: Path) -> Gate:
    if not log.is_file():
        return Gate(
            "pi4-board", "MISSING", f"read {log.relative_to(ROOT)}", utc(), utc(), "",
            str(log.relative_to(ROOT)), "", "no board log; Pi 4 remains UNVERIFIED"
        )
    result = subprocess.run(
        ["sh", "scripts/board-pi4-check.sh", "--log", str(log)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    text = result.stdout
    if result.returncode == 0:
        status = "MARKERS COMPLETE"
        note = "transcript markers complete; no board PASS claimed, owner attestation required"
    elif result.returncode == 2:
        status = "REFUSED"
        note = "board checker refused the log as virt/synthetic or non-board"
    else:
        status = "INCOMPLETE"
        note = "board checker found missing markers"
    return Gate(
        "pi4-board", status, "sh scripts/board-pi4-check.sh --log", utc(), utc(),
        str(result.returncode), str(log.relative_to(ROOT)), sha256(log), note,
        required=True,
    )


def packet_text(gates: list[Gate], head: str, dirty: bool, owner_attestation: bool) -> str:
    required = [gate for gate in gates if gate.required]
    board = next((gate for gate in gates if gate.name == "pi4-board"), None)
    ready = (
        all(gate.status in {"PASS", "PINNED"} for gate in required if gate.name != "pi4-board")
        and board is not None
        and board.status == "MARKERS COMPLETE"
        and owner_attestation
    )
    verdict = "READY FOR OWNER TAG REVIEW (no tag issued)" if ready else "NO-GO"
    lines = [
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->",
        "<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->",
        "",
        "# RavenBSD 0.1.0 release evidence packet",
        "",
        f"- Generated: {utc()}",
        f"- Commit: `{head}`",
        f"- Working tree: `{'dirty' if dirty else 'clean'}`",
        f"- Verdict: **{verdict}**",
        "- Tag: **none** (this packet does not tag)",
        "- Scope: generated release evidence. Bob's readiness matrix in",
        "  `docs/releases/0.1.0-readiness.md` remains a separate audit and is not replaced.",
        "",
        "## Gate table",
        "",
        "| Gate | Status | Exit | Recorded/UTC | Evidence | Note |",
        "|---|---|---|---|---|---|",
    ]
    for gate in gates:
        evidence = gate.evidence
        if gate.sha256:
            evidence = f"`{gate.evidence}` sha256=`{gate.sha256}`"
        note = gate.note.replace("|", "\\|")
        lines.append(
            f"| {gate.name} | **{gate.status}** | {gate.exit_code or '-'} | "
            f"{gate.started or '-'} | {evidence} | {note} |"
        )
    lines.extend(
        [
            "",
            "## Missing, stale and refused",
            "",
        ]
    )
    bad = [gate for gate in gates if gate.status not in {"PASS", "PINNED", "MARKERS COMPLETE"}]
    if bad:
        for gate in bad:
            lines.append(f"- `{gate.name}`: **{gate.status}** — {gate.note}")
    else:
        lines.append("- None.")
    lines.extend(
        [
            "",
            "## Reproduce",
            "",
            "```sh",
            "sh scripts/release-evidence.sh",
            "sh scripts/release-evidence.sh --self-test",
            "sh scripts/release-evidence.sh --check",
            "```",
            "",
            "`--run-qtests` refreshes the dynamic-userland x86_64/aarch64 and release-login logs;",
            "`--board-log LOG` supplies the owner's Pi 4 serial capture. A board log with a",
            "virt/synthetic marker is refused, and markers-complete is not a board PASS.",
            "",
            f"Overall: **{verdict}**. No tag is issued by this script.",
            "",
        ]
    )
    return "\n".join(lines)


def collect(args: argparse.Namespace) -> int:
    head = args.commit or head_commit()
    dirty = tree_dirty()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    gates = []
    if not args.no_make_check:
        gates.append(run_gate("make-check", ["make", "check"], head, dirty))
    gates.append(parse_posix(args.posix_report, head))
    gates.append(parse_libc(head))
    gates.append(parse_kernel_gate(args.kernel_gate, head))
    gates.append(dynamic_gate("x86_64", head, args.run_qtests))
    gates.append(dynamic_gate("aarch64", head, args.run_qtests))
    gates.append(release_login_gate(head, args.run_qtests))
    gates.append(image_gate("image-x86_64", args.image_x86, head))
    gates.append(image_gate("image-aarch64", args.image_aarch64, head))
    board_log = args.board_log or PI4_DEFAULT
    gates.append(board_gate(board_log))
    packet = packet_text(gates, head, dirty, args.owner_attestation)
    args.packet.parent.mkdir(parents=True, exist_ok=True)
    args.packet.write_text(packet, encoding="utf-8")
    ready = "READY FOR OWNER TAG REVIEW" in packet
    print(f"release-evidence: packet={args.packet}")
    for gate in gates:
        print(f"release-evidence: {gate.name:<20} {gate.status:<18} {gate.note}")
    print(f"release-evidence: verdict={'READY FOR OWNER TAG REVIEW' if ready else 'NO-GO'}")
    return 0 if ready else 1


def check(args: argparse.Namespace) -> int:
    missing = []
    if not args.packet.is_file():
        missing.append(str(args.packet.relative_to(ROOT)))
    text = read_text(args.packet)
    if "**NO-GO**" in text:
        for line in text.splitlines():
            if line.startswith("- `") and "**" in line:
                missing.append(line)
        if not missing:
            missing.append("packet verdict is NO-GO")
    if missing:
        print("release-evidence: blocked: missing " + ", ".join(missing[:8]))
        return 1
    print(f"release-evidence: packet current: {args.packet}")
    return 0


def qemu_mode(args: argparse.Namespace) -> int:
    head = args.commit or head_commit()
    gate = release_login_gate(head, True)
    print(f"release-evidence: release-login {gate.status}: {gate.note}")
    return 0 if gate.status == "PASS" else 1


def record_qemu_mode(args: argparse.Namespace) -> int:
    head = args.commit or head_commit()
    dirty = tree_dirty()
    gate = release_login_gate(head, True)
    image = args.image_x86
    transcript = ROOT / "build/release/qemu-release.log"
    out = EVIDENCE / "evidence-qemu.txt"
    out.parent.mkdir(parents=True, exist_ok=True)
    image_hash = sha256(image) if image.is_file() else "MISSING"
    lines = [
        f"version={VERSION}",
        f"commit={head}",
        f"working_tree={'dirty' if dirty else 'clean'}",
        f"image={image.relative_to(ROOT)}",
        f"image_sha256={image_hash}",
        f"provenance_sha256={sha256(ROOT / 'PROVENANCE.toml')}",
        f"qemu_transcript={transcript.relative_to(ROOT)}",
        f"qemu_transcript_sha256={sha256(transcript)}",
        f"qemu_gate={gate.status} ({gate.note})",
        "pi4_gate=UNVERIFIED (board log missing or owner attestation required)",
        "tag=none",
    ]
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"release-evidence: QEMU evidence: {out.relative_to(ROOT)}")
    print(f"release-evidence: release-login {gate.status}: {gate.note}")
    return 0 if gate.status == "PASS" and image_hash != "MISSING" else 1


def pi4_mode(args: argparse.Namespace) -> int:
    gate = board_gate(Path(args.pi4_log))
    net_status = 0
    if args.pi4_net_log:
        net = Path(args.pi4_net_log)
        result = subprocess.run(
            ["sh", "scripts/pi4-net-evidence.sh", str(net)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        net_status = result.returncode
        print(result.stdout.rstrip())
    print(f"release-evidence: board {gate.status}: {gate.note}")
    if gate.status == "MARKERS COMPLETE":
        return 0 if net_status == 0 else 1
    return 2 if gate.status == "REFUSED" else 1


def self_test() -> int:
    fixtures = ROOT / "tests/release/evidence/fixtures"
    head = "1" * 40
    good = parse_kernel_gate(fixtures / "kernel-gate-good.md", head)
    if good.status != "PASS" or "27/27" not in good.note:
        raise AssertionError(f"good kernel gate: {good}")
    stale = parse_kernel_gate(fixtures / "kernel-gate-stale.md", head)
    if stale.status != "STALE":
        raise AssertionError(f"stale kernel gate: {stale}")
    missing = parse_kernel_gate(fixtures / "does-not-exist.md", head)
    if missing.status != "MISSING":
        raise AssertionError(f"missing kernel gate: {missing}")
    current = parse_kernel_gate(fixtures / "kernel-gate-current.md", head)
    if current.status != "PASS" or "27/27" not in current.note:
        raise AssertionError(f"current kernel gate: {current}")
    partial = parse_kernel_gate(fixtures / "kernel-gate-partial.md", head)
    if partial.status != "FAIL":
        raise AssertionError(f"partial kernel gate: {partial}")

    good_posix = parse_posix(fixtures / "posix-good.md", head)
    if good_posix.status != "PASS":
        raise AssertionError(f"good posix: {good_posix}")
    stale_posix = parse_posix(fixtures / "posix-stale.md", head)
    if (
        stale_posix.status != "STALE"
        or "222222222222" not in stale_posix.note
        or "111111111111" not in stale_posix.note
    ):
        raise AssertionError(f"stale posix: {stale_posix}")
    no_commit = parse_posix(fixtures / "posix-no-commit.md", head)
    if no_commit.status != "STALE" or "no commit stamp" not in no_commit.note:
        raise AssertionError(f"unstamped posix: {no_commit}")
    failed_posix = parse_posix(fixtures / "posix-fail.md", head)
    if failed_posix.status != "FAIL":
        raise AssertionError(f"failed posix: {failed_posix}")
    if commit_is_current("2" * 40, head):
        raise AssertionError("mismatched commits must not be current")
    parent = git("rev-parse", "HEAD^")
    if parent and not commit_is_current(parent, head_commit()):
        raise AssertionError(f"ancestor commit {parent} must be current")

    pinned = image_gate(
        "image-x86_64", fixtures / "missing.img", head, fixtures / "release-images-good.md"
    )
    if pinned.status != "PINNED" or "aaaaaaaa" not in pinned.note:
        raise AssertionError(f"pinned image: {pinned}")
    stale_image = image_gate(
        "image-x86_64", fixtures / "missing.img", head, fixtures / "release-images-stale.md"
    )
    if stale_image.status != "UNPINNED":
        raise AssertionError(f"stale image: {stale_image}")

    good_board = board_gate(fixtures / "board-good.log")
    if good_board.status != "MARKERS COMPLETE":
        raise AssertionError(f"good board: {good_board}")
    virt = board_gate(fixtures / "board-virt.log")
    if virt.status != "REFUSED":
        raise AssertionError(f"virt board: {virt}")
    synthetic = board_gate(fixtures / "board-synthetic.log")
    if synthetic.status != "REFUSED":
        raise AssertionError(f"synthetic board: {synthetic}")
    incomplete = board_gate(fixtures / "board-incomplete.log")
    if incomplete.status != "INCOMPLETE":
        raise AssertionError(f"incomplete board: {incomplete}")

    missing_image = image_gate("image-test", ROOT / "tests/release/evidence/fixtures/missing.img", head)
    if missing_image.status != "MISSING":
        raise AssertionError(f"missing image: {missing_image}")
    fixture_image = fixtures / "board-good.log"
    unpinned = image_gate("image-test", fixture_image, head)
    if unpinned.status not in {"UNPINNED", "PASS"} or not unpinned.sha256:
        raise AssertionError(f"image hash: {unpinned}")

    print(
        "release-evidence self-test: PASS (kernel good/stale/missing/current/partial, "
        "posix good/stale/no-stamp/fail, board good/virt/synthetic/incomplete, "
        "image missing/hash/pinned/stale)"
    )
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="scripts/release-evidence.sh",
        description="Collect and verify the RavenBSD 0.1.0 release evidence packet",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--collect", action="store_true", help="collect evidence and write the packet")
    mode.add_argument("--check", action="store_true", help="check the existing packet; no commands")
    mode.add_argument("--self-test", action="store_true", help="run fixture refusal tests")
    mode.add_argument("--qemu", action="store_true", help="run the release-login qtest only")
    mode.add_argument("--pi4-log", metavar="LOG", help="validate one Pi 4 board log")
    mode.add_argument("--record", action="store_true", help="alias for --collect")
    mode.add_argument("--record-qemu", action="store_true", help="alias for --qemu")
    parser.add_argument("--commit", help="evidence commit (default HEAD)")
    parser.add_argument("--packet", type=Path, default=PACKET)
    parser.add_argument("--kernel-gate", type=Path, default=KERNEL_GATE)
    parser.add_argument("--posix-report", type=Path, default=POSIX_REPORT)
    parser.add_argument("--image-x86", type=Path, default=IMAGE_X86_DEFAULT)
    parser.add_argument("--image-aarch64", type=Path, default=IMAGE_AARCH64_DEFAULT)
    parser.add_argument("--board-log", type=Path, help="Pi 4 serial log for the packet")
    parser.add_argument("--pi4-net-log", type=Path, help="GENET net log for --pi4-log")
    parser.add_argument("--run-qtests", action="store_true", help="run dynamic-userland and release-login")
    parser.add_argument("--no-make-check", action="store_true", help="do not run make check")
    parser.add_argument(
        "--owner-attestation",
        action="store_true",
        help="owner attested the board log (markers-complete still is not a script PASS)",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.pi4_log:
        return pi4_mode(args)
    if args.check:
        return check(args)
    if args.record_qemu:
        return record_qemu_mode(args)
    if args.qemu:
        return qemu_mode(args)
    return collect(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except AssertionError as error:
        print(f"release-evidence: SELF-TEST FAILED: {error}", file=sys.stderr)
        raise SystemExit(1) from error
