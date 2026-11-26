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

import mgmt

ROOT = Path(__file__).resolve().parents[3]
VERSION = "0.1.0"
PACKET_REL = "tickets/release/0.1.0-evidence.md"
EVIDENCE = ROOT / "build/release" / VERSION
MGMT = mgmt.resolve_root(ROOT)
RELEASE_IMAGES = MGMT / "architecture/product-data/release-images.md"
SMP_SOAK = MGMT / "architecture/product-data/smp-boot-soak.md"
SMP_CLOSEOUT = MGMT / "architecture/product-data/smp-closeout.md"
RELEASE_NOTES = MGMT / "architecture/product-data/release-notes-0.1.0.md"
LIBC_REPORT = ROOT / "libc/ravenlibc/GATE-REPORT.md"
LIBC_DECISIONS = MGMT / "architecture/product-data/libc-decisions-0.1.0.md"
FREEZE_CHECKLIST = MGMT / "architecture/product-data/freeze-checklist-0.1.0.md"
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


def display(path: Path) -> str:
    """Path for the packet: product-relative when inside the tree, else absolute."""
    return mgmt.display(ROOT, path)


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
        evidence=display(log),
        sha256=sha256(log),
        note="exit 0; clean tree" if status == "PASS" else f"exit {result.code}; tree {'dirty' if dirty else 'clean'}",
    )


def parse_kernel_gate(path: Path, head: str) -> Gate:
    text = read_text(path)
    source = f"read {display(path)}"
    if not text:
        return Gate(
            "kernel-gate", "MISSING", source, utc(), utc(), "",
            display(path), "", "recorded gate document is missing"
        )
    current = re.search(r"current master `([0-9a-f]{7,40})`", text)
    full = re.search(r"\(`?([0-9a-f]{40})`?\)", text)
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
            "kernel-gate", "MISSING", source, stamp, utc(), "",
            display(path), sha256(path), "document has no commit stamp"
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
        "kernel-gate", status, source, stamp, utc(), "",
        display(path), sha256(path), note
    )


def parse_posix(path: Path, head: str) -> Gate:
    text = read_text(path)
    source = f"read {display(path)}"
    if not text:
        return Gate(
            "posix", "MISSING", source, utc(), utc(), "",
            display(path), "", "host-only report is missing"
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
        "posix", status, source, stamp, utc(), "",
        display(path), sha256(path), note
    )


def libc_committed_counts(path: Path) -> tuple[int, int, int] | None:
    text = read_text(path)
    match = re.search(r"MET=(\d+)\s+NOT MET=(\d+)\s+BLOCKED=(\d+)", text)
    if not match:
        return None
    return int(match.group(1)), int(match.group(2)), int(match.group(3))


def libc_deferred_counts(path: Path) -> tuple[int, int, int] | None:
    """DR-60 defers the internal libc gate; return its recorded aggregate if present."""
    text = read_text(path)
    if "DR-60" not in text or "DEFERRED" not in text:
        return None
    return libc_committed_counts(path)


def libc_gate_from_log(
    log: Path,
    result_code: int,
    started: str,
    ended: str,
    report: Path = LIBC_REPORT,
    decisions: Path = LIBC_DECISIONS,
) -> Gate:
    """Classify a libc-gate verdict log, separating absent local evidence from a real verdict."""
    text = read_text(log)
    verdict = re.search(r"^libc gate: (.+)$", text, re.MULTILINE)
    status_text = verdict.group(1) if verdict else ""
    met = len(re.findall(r"\|\s*MET\s*\|", text))
    not_met = len(re.findall(r"\|\s*NOT MET\s*\|", text))
    blocked = len(re.findall(r"\|\s*BLOCKED\s*\|", text))
    missing = text.count("evidence file missing")
    deferred = libc_deferred_counts(decisions)
    if deferred:
        note = (
            f"DR-60 deferred to 0.2.0; internal gate NOT MET; "
            f"MET={deferred[0]} NOT MET={deferred[1]} BLOCKED={deferred[2]} "
            f"(decision page); POSIX is the public libc gate"
        )
        return Gate(
            "libc-gate", "DEFERRED", f"read {display(LIBC_DECISIONS)}",
            started, ended, str(result_code),
            display(decisions), sha256(decisions), note
        )
    if missing >= 10 and met == 0 and not_met >= 10:
        counts = libc_committed_counts(decisions)
        count_text = (
            f"MET={counts[0]} NOT MET={counts[1]} BLOCKED={counts[2]}"
            if counts
            else "committed counts unavailable"
        )
        note = (
            f"local evidence files missing ({missing} rows); committed GATE-REPORT "
            f"NOT MET; {count_text} (decision page); rerun with build/libc-gate evidence"
        )
        return Gate(
            "libc-gate", "STALE", "cargo run -q -p libc-gate -- verdict",
            started, ended, str(result_code),
            display(report), sha256(report), note
        )
    if result_code == 0 and status_text == "MET":
        status = "PASS"
    elif status_text in {"NOT MET", "MET"}:
        status = "FAIL" if status_text != "MET" else "PASS"
    else:
        status = "MISSING"
    note = f"{status_text or 'no verdict'}; MET={met} NOT MET={not_met} BLOCKED={blocked}"
    return Gate(
        "libc-gate", status, "cargo run -q -p libc-gate -- verdict",
        started, ended, str(result_code),
        display(log), sha256(log), note
    )


def parse_libc(head: str) -> Gate:
    log = EVIDENCE / "libc-verdict.log"
    result = run_command(["cargo", "run", "-q", "-p", "libc-gate", "--", "verdict"], log)
    return libc_gate_from_log(log, result.code, result.started, result.ended)


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
                result.started, result.ended, code, display(log),
                sha256(log), f"exit {result.code}"
            )
    else:
        log = transcript
        code = "recorded"
        if not transcript.is_file():
            return Gate(
                "release-login", "MISSING", "read build/qtest/release-login.log",
                utc(), utc(), "recorded", display(transcript), "",
                "release-login transcript is missing; run --run-qtests"
            )
    recorded = parse_transcript_commit(transcript)
    if not recorded:
        return Gate(
            "release-login", "MISSING", "read release-login transcript", utc(), utc(),
            code, display(transcript), sha256(transcript),
            "transcript has no kernel commit banner"
        )
    if not commit_is_current(recorded, head):
        return Gate(
            "release-login", "STALE", "read release-login transcript", utc(), utc(),
            code, display(transcript), sha256(transcript),
            f"recorded at {recorded}; HEAD is {head}"
        )
    if not check_transcript(transcript):
        return Gate(
            "release-login", "FAIL", "tests/fixtures/release/check-transcript.sh", utc(), utc(),
            code, display(transcript), sha256(transcript),
            "transcript shape check failed"
        )
    return Gate(
        "release-login", "PASS", "release-login transcript", utc(), utc(), code,
        display(transcript), sha256(transcript),
        f"transcript at {recorded}"
    )


SMP_KVM_ROW = re.compile(
    r"\|\s*KVM\s*`-smp (\d)`\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([^|]*)\|"
)
SMP_TCG_ROW = re.compile(
    r"\|\s*TCG\s*`-smp (\d)`\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([^|]*)\|"
)


def latest_soak_table(
    text: str,
) -> tuple[dict[int, tuple[int, int, str]], dict[int, tuple[int, int, str]], str] | None:
    """Return the last campaign table (KVM, TCG, section) with both row kinds."""
    latest = None
    for section in re.split(r"(?m)^## ", text):
        kvm = {
            int(match.group(1)): (
                int(match.group(2)),
                int(match.group(3)),
                match.group(4).strip(),
            )
            for match in SMP_KVM_ROW.finditer(section)
        }
        tcg = {
            int(match.group(1)): (
                int(match.group(2)),
                int(match.group(3)),
                match.group(4).strip(),
            )
            for match in SMP_TCG_ROW.finditer(section)
        }
        if kvm and tcg:
            latest = (kvm, tcg, section)
    return latest


def release_notes_soak(
    text: str,
) -> tuple[dict[int, tuple[int, int, str]], dict[int, tuple[int, int, str]], list[str]] | None:
    """Read the latest S107 campaign summary from the release notes.

    The dedicated round-5 document can lag the release notes; when the notes
    record a TCG4 failure, that failure is authoritative for the packet.
    """
    row = next((line for line in text.splitlines() if "KVM/TCG boot soak" in line), "")
    if not row:
        return None
    kvm = {
        int(match.group(1)): (int(match.group(3)), int(match.group(2)), "none")
        for match in re.finditer(r"KVM\s*`-smp (\d)`\s*\*\*(\d+)/(\d+)\*\*", row)
    }
    tcg = re.search(r"TCG\s*`-smp (\d)`\s*(?:is\s*)?\*\*(\d+)/(\d+)", row)
    if not kvm or not tcg:
        return None
    requested, passed = int(tcg.group(3)), int(tcg.group(2))
    failure = "none"
    extra: list[str] = []
    if passed < requested:
        failure = "login timeout"
        run = re.search(r"run\s+(\d+)", row)
        if run:
            failure = f"run {run.group(1)}/{requested} login timeout"
        if "S126" in row and "readiness timeout" in row:
            extra.append("S126 TCG4 readiness timeout (nulld/ramfsd)")
    return kvm, {int(tcg.group(1)): (requested, passed, failure)}, extra


def campaign_parts(
    kvm: dict[int, tuple[int, int, str]], tcg: dict[int, tuple[int, int, str]]
) -> tuple[list[str], list[str]]:
    parts: list[str] = []
    failures: list[str] = []
    for mode in (1, 2, 4):
        if mode not in kvm:
            failures.append(f"KVM{mode} missing")
            continue
        requested, passed, failure = kvm[mode]
        parts.append(f"KVM{mode} {passed}/{requested}")
        if requested == 0 or passed != requested or failure.lower() not in {"none", "none.", ""}:
            failures.append(f"KVM{mode} {passed}/{requested} {failing_text(failure)}")
    if 4 not in tcg:
        failures.append("TCG4 missing")
    else:
        requested, passed, failure = tcg[4]
        parts.append(f"TCG4 {passed}/{requested}")
        if not (requested == 10 and passed == 10 and failure.lower() in {"none", "none.", ""}):
            failures.append(f"TCG4 {passed}/{requested} {failing_text(failure)}")
    return parts, failures


def closeout_result(text: str) -> tuple[str, str]:
    """Classify the S126 closeout document: pass/fail/not_run/ambiguous/missing."""
    if not text:
        return "missing", ""
    passed = re.search(r"(?im)^\s*Result:\s*\*\*PASS\*\*", text)
    failed = re.search(r"(?im)^\s*Result:\s*\*\*FAIL[^*]*\*\*(.*)$", text)
    failed_line = re.search(r"(?im)^\s*FAILED\b(.*)$", text)
    not_run = re.search(r"(?i)not\s+run\s+yet|matrix is \*\*not\*\* run|zero samples", text)
    if failed:
        return "fail", (failed.group(1).strip(" :-—\t") or "closeout result FAIL")
    if failed_line:
        return "fail", failing_text(failed_line.group(1))
    if passed:
        return "pass", ""
    if not_run:
        return "not_run", "closeout matrix has not run"
    return "ambiguous", "closeout result is missing"


def failing_text(text: str) -> str:
    value = text.strip(" :-—\t")
    return value or "failure recorded"


def markdown_section(text: str, heading: str) -> str:
    """Return the body after a level-3 heading, up to the next level-3 heading."""
    match = re.search(heading, text, re.MULTILINE)
    if not match:
        return ""
    rest = text[match.end():]
    end = re.search(r"(?m)^### ", rest)
    return rest[: end.start()] if end else rest


def libc_1cpu_gate(notes: Path, head: str, freeze: Path | None = None) -> Gate:
    """Read the S132 1-CPU libc crash, which is separate from the SMP/TLB rows."""
    if freeze:
        freeze_text = read_text(freeze)
        fixed = re.search(
            r"1-CPU crash is \*\*FIXED by S132\*\*|S132.*1-CPU 3/3", freeze_text
        )
        if fixed:
            return Gate(
                "libc-1cpu", "MET",
                f"read {display(freeze)}", utc(), utc(), "",
                display(freeze), sha256(freeze),
                "S132 FIXED; 1-CPU 3/3; libc-threads PASS (freeze checklist)",
            )
    text = read_text(notes)
    if not text:
        return Gate(
            "libc-1cpu", "MISSING", f"read {display(notes)}", utc(), utc(), "",
            display(notes), "", "release notes are missing"
        )
    section = markdown_section(text, r"(?m)^### libc thread 1-CPU crash.*$")
    if not section:
        return Gate(
            "libc-1cpu", "MISSING", f"read {display(notes)}", utc(), utc(), "",
            display(notes), sha256(notes), "S132 1-CPU section is missing"
        )
    fault = re.search(r"page fault at (0x[0-9a-fA-F]+)", section)
    child = re.search(r"child exited \(?(-?\d+)\)?", section)
    code = re.search(r"exit status (\d+)", section)
    before = "before `malloc ok`" in section or "before malloc ok" in section
    clean = "BLOCKED" not in section and bool(
        re.search(r"(?i)\bS132\b.*\b(DONE|PASS|fixed)\b|\b1-CPU\b.*\b(PASS|DONE|fixed)\b", section)
    )
    if fault:
        status = "FAIL"
        note = (
            f"smp-libc-concurrency-kvm1: page fault at {fault.group(1)}; "
            f"child exited {child.group(1) if child else 'unknown'}; "
            f"exit {code.group(1) if code else 'unknown'}; "
            f"{'before malloc ok; ' if before else ''}owner S132/Tim; not SMP"
        )
    elif clean:
        status = "MET"
        note = "S132 1-CPU evidence is MET"
    else:
        status = "STALE"
        note = "S132 1-CPU section is ambiguous; no fault or clean result"
    return Gate(
        "libc-1cpu", status, f"read {display(notes)}", utc(), utc(), "",
        display(notes), sha256(notes), note
    )


def cold_start_gate(notes: Path, head: str) -> Gate:
    """Read the dynamic cold-start/interactive-login row; pending until K139."""
    text = read_text(notes)
    if not text:
        return Gate(
            "cold-start", "MISSING", f"read {display(notes)}", utc(), utc(), "",
            display(notes), "", "release notes are missing"
        )
    heading = re.search(r"(?m)^### Dynamic cold start / interactive login.*$", text)
    section = markdown_section(text, r"(?m)^### Dynamic cold start / interactive login.*$")
    if not section:
        section = markdown_section(text, r"(?m)^### Interactive shell.*$")
    if not section:
        return Gate(
            "cold-start", "MISSING", f"read {display(notes)}", utc(), utc(), "",
            display(notes), sha256(notes), "cold-start section is missing"
        )
    spawn = re.search(
        r"`?spawn`?\s*(?:→|->)\s*`?main`?\s+takes\s+\**([0-9.]+)\s*(m?s)",
        section,
        re.IGNORECASE,
    )
    login = re.search(
        r"`?root`?\s*(?:→|->)\s*`?Password:?`?\s+takes\s+\**([0-9.]+)\s*(m?s)",
        section,
        re.IGNORECASE,
    )
    pending = "PENDING K139" in section or bool(
        heading and "PENDING K139" in heading.group(0)
    )
    if pending:
        status = "PENDING K139"
        before = []
        if spawn:
            before.append(f"spawn->main {spawn.group(1)} {spawn.group(2)}")
        if login:
            before.append(f"login {login.group(1)} {login.group(2)}")
        note = (("before " + ", ".join(before) + "; ") if before else "") + (
            "command scripts/login-cold-start.sh (K143) or "
            "cargo xtask qtest --case login-latency (K139); sample=0; "
            "acceptance 5/5 <1 s/step, target 300 ms"
        )
    elif re.search(r"\bMET\b", section):
        status = "MET"
        note = "after-fix cold-start evidence is MET"
    else:
        status = "STALE"
        note = "cold-start state is ambiguous"
    return Gate(
        "cold-start", status, f"read {display(notes)}", utc(), utc(), "",
        display(notes), sha256(notes), note
    )


def smp_soak_gate(
    soak: Path,
    closeout: Path,
    head: str,
    notes: Path = RELEASE_NOTES,
    freeze: Path | None = None,
) -> Gate:
    soak_text = read_text(soak)
    closeout_text = read_text(closeout)
    if not soak_text:
        return Gate(
            "smp-soak", "MISSING", f"read {display(soak)}", utc(), utc(), "",
            display(soak), "", "soak document is missing"
        )
    table = latest_soak_table(soak_text)
    if not table:
        return Gate(
            "smp-soak", "STALE", f"read {display(soak)}", utc(), utc(), "",
            display(soak), sha256(soak), "soak campaign table is missing or ambiguous"
        )
    notes_text = read_text(notes)
    notes_result = release_notes_soak(notes_text)
    if notes_result:
        kvm, tcg, extra_failures = notes_result
        source = f"{display(notes)} (S107/S139)"
        evidence_path = notes
        evidence_sha = sha256(notes)
        campaign = ""
    else:
        kvm, tcg, section = table
        extra_failures = []
        source = display(soak)
        evidence_path = soak
        evidence_sha = sha256(soak)
        campaign = re.search(r"Campaign at `([0-9a-f]{7,40})`", section)
    parts, failures = campaign_parts(kvm, tcg)
    failures.extend(extra_failures)
    counts = ", ".join(parts)
    notes_closeout = re.search(r"(?m)^### SMP closeout — MET \(20/20\)", notes_text)
    freeze_text = read_text(freeze) if freeze else ""
    freeze_closeout = re.search(
        r"full closeout at `([0-9a-f]{7,40})` is \*\*20/20 PASS\*\*"
        r"|closeout is \*\*MET 20/20\*\*",
        freeze_text,
    )
    closeout_note = ""
    if notes_closeout or freeze_closeout:
        closeout_state, closeout_failure = "pass", ""
        closeout_note = "notes S139 20/20 at 3ce80cae"
    else:
        closeout_state, closeout_failure = closeout_result(closeout_text)
    if closeout_state == "fail":
        failures.append(f"closeout: {closeout_failure}")
    if failures:
        status = "FAIL"
        note = counts + "; " + "; ".join(failures)
    elif closeout_state == "pass":
        status = "PASS"
        note = counts + "; closeout clean" + (
            f" ({closeout_note})" if closeout_note else ""
        )
    else:
        status = "STALE"
        note = counts + f"; closeout {closeout_state}" + (
            f" ({closeout_failure})" if closeout_failure else ""
        )
    if campaign:
        note += f"; campaign {campaign.group(1)}"
    note += f"; source {source}"
    return Gate(
        "smp-soak", status, f"read {display(evidence_path)}", utc(), utc(), "",
        display(evidence_path), evidence_sha, note
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
                result.started, result.ended, code, display(evidence),
                sha256(evidence), f"exit {result.code}"
            )
    else:
        code = "recorded"
        if not evidence.is_file():
            return Gate(
                name, "MISSING", f"read build/qtest/dynamic-userland.log",
                utc(), utc(), "recorded", display(evidence), "",
                f"no {arch} log; run --run-qtests"
            )
    recorded = parse_transcript_commit(evidence)
    if not recorded:
        return Gate(
            name, "MISSING", "read dynamic-userland log", utc(), utc(), code,
            display(evidence), sha256(evidence),
            "log has no kernel commit banner"
        )
    if not commit_is_current(recorded, head):
        return Gate(
            name, "STALE", "read dynamic-userland log", utc(), utc(), code,
            display(evidence), sha256(evidence),
            f"recorded at {recorded}; HEAD is {head}"
        )
    return Gate(
        name, "PASS", "dynamic-userland log", utc(), utc(), code,
        display(evidence), sha256(evidence),
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
            name, "PINNED", f"read {display(images_doc)}", utc(), utc(), "",
            display(images_doc), recorded, note
        )
    if not path.is_file():
        if recorded and recorded_commit:
            note = (
                f"recorded sha256={recorded}; product commit={recorded_commit} is not "
                f"HEAD {head[:12] or 'unknown'}"
            )
            return Gate(
                name, "UNPINNED", f"read {display(images_doc)}", utc(), utc(), "",
                display(images_doc), recorded, note
            )
        return Gate(
            name, "MISSING", f"sha256 {display(path)}", utc(), utc(), "",
            display(path), "", "image is missing"
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
        name, status, f"sha256 {display(path)}", utc(), utc(), "",
        display(path), digest, note
    )


def board_gate(log: Path) -> Gate:
    if not log.is_file():
        return Gate(
            "pi4-board", "MISSING", f"read {display(log)}", utc(), utc(), "",
            display(log), "", "no board log; Pi 4 remains UNVERIFIED"
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
        str(result.returncode), display(log), sha256(log), note,
        required=True,
    )


def packet_text(gates: list[Gate], head: str, dirty: bool, owner_attestation: bool) -> str:
    required = [gate for gate in gates if gate.required]
    board = next((gate for gate in gates if gate.name == "pi4-board"), None)
    ready = (
        all(
            gate.status in {"PASS", "PINNED", "MET", "DEFERRED"}
            for gate in required
            if gate.name != "pi4-board"
        )
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
        "  The readiness matrix remains a separate audit and is not replaced.",
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
    bad = [
        gate
        for gate in gates
        if gate.status not in {"PASS", "PINNED", "MET", "DEFERRED", "MARKERS COMPLETE"}
    ]
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
    global RELEASE_IMAGES, SMP_SOAK, SMP_CLOSEOUT, RELEASE_NOTES, LIBC_DECISIONS, FREEZE_CHECKLIST
    head = args.commit or head_commit()
    dirty = tree_dirty()
    if args.packet is None:
        args.packet = mgmt.require_root(ROOT) / PACKET_REL
    posix_report = args.posix_report or mgmt.require_file(
        ROOT, "architecture/product-data/posix-conformance.md"
    )
    kernel_gate = args.kernel_gate or mgmt.require_file(
        ROOT, "architecture/product-data/kernel-gate.md"
    )
    RELEASE_IMAGES = mgmt.require_file(ROOT, "architecture/product-data/release-images.md")
    SMP_SOAK = mgmt.require_file(ROOT, "architecture/product-data/smp-boot-soak.md")
    SMP_CLOSEOUT = mgmt.require_file(ROOT, "architecture/product-data/smp-closeout.md")
    RELEASE_NOTES = mgmt.require_file(ROOT, "architecture/product-data/release-notes-0.1.0.md")
    LIBC_DECISIONS = mgmt.require_file(
        ROOT, "architecture/product-data/libc-decisions-0.1.0.md"
    )
    FREEZE_CHECKLIST = mgmt.require_file(
        ROOT, "architecture/product-data/freeze-checklist-0.1.0.md"
    )
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    gates = []
    if not args.no_make_check:
        gates.append(run_gate("make-check", ["make", "check"], head, dirty))
    gates.append(parse_posix(posix_report, head))
    gates.append(parse_libc(head))
    gates.append(parse_kernel_gate(kernel_gate, head))
    gates.append(smp_soak_gate(SMP_SOAK, SMP_CLOSEOUT, head, RELEASE_NOTES, FREEZE_CHECKLIST))
    gates.append(libc_1cpu_gate(RELEASE_NOTES, head, FREEZE_CHECKLIST))
    gates.append(dynamic_gate("x86_64", head, args.run_qtests))
    gates.append(dynamic_gate("aarch64", head, args.run_qtests))
    gates.append(release_login_gate(head, args.run_qtests))
    gates.append(cold_start_gate(RELEASE_NOTES, head))
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
    if args.packet is None:
        args.packet = mgmt.require_root(ROOT) / PACKET_REL
    missing = []
    if not args.packet.is_file():
        missing.append(display(args.packet))
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
        f"image={display(image)}",
        f"image_sha256={image_hash}",
        f"provenance_sha256={sha256(ROOT / 'PROVENANCE.toml')}",
        f"qemu_transcript={display(transcript)}",
        f"qemu_transcript_sha256={sha256(transcript)}",
        f"qemu_gate={gate.status} ({gate.note})",
        "pi4_gate=UNVERIFIED (board log missing or owner attestation required)",
        "tag=none",
    ]
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"release-evidence: QEMU evidence: {display(out)}")
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


def resolver_self_test() -> None:
    """Prove the one resolver policy: env override, sibling default, loud miss."""
    import tempfile

    relative = "architecture/product-data/posix-conformance.md"
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        product = tmp / "ravenbsd"
        product.mkdir()
        mgmt_dir = tmp / "ravenbsd-prjmgmt"
        target = mgmt_dir / relative
        target.parent.mkdir(parents=True)
        target.write_text("- Counts: 1 pass, 0 fail, 0 skip\n", encoding="utf-8")

        overridden = mgmt.require_file(product, relative, env_value=str(mgmt_dir))
        if overridden != target:
            raise AssertionError(f"env override resolved {overridden}, expected {target}")

        missing_env = tmp / "missing"
        try:
            mgmt.require_file(product, relative, env_value=str(missing_env))
        except SystemExit as error:
            if "RAVEN_MGMT_ROOT" not in str(error):
                raise AssertionError(f"missing env root does not name RAVEN_MGMT_ROOT: {error}")
        else:
            raise AssertionError("missing env root was accepted")

        no_sibling = tmp / "no-sibling"
        product2 = no_sibling / "ravenbsd"
        product2.mkdir(parents=True)
        try:
            mgmt.require_file(product2, relative, env_value="")
        except SystemExit as error:
            if "RAVEN_MGMT_ROOT" not in str(error):
                raise AssertionError(f"missing sibling does not name RAVEN_MGMT_ROOT: {error}")
        else:
            raise AssertionError("missing sibling was accepted")

        sibling = no_sibling / "ravenbsd-prjmgmt"
        sibling_target = sibling / relative
        sibling_target.parent.mkdir(parents=True)
        sibling_target.write_text("sibling\n", encoding="utf-8")
        if mgmt.require_file(product2, relative, env_value="") != sibling_target:
            raise AssertionError("default sibling path was not used")


def self_test() -> int:
    resolver_self_test()
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
    current_stale = parse_kernel_gate(fixtures / "kernel-gate-current-stale.md", head)
    if (
        current_stale.status != "STALE"
        or "222222222222" not in current_stale.note
        or "111111111111" not in current_stale.note
    ):
        raise AssertionError(f"stale current-format kernel gate: {current_stale}")
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
    local_mismatch = image_gate(
        "image-x86_64",
        fixtures / "board-good.log",
        head,
        fixtures / "release-images-good.md",
    )
    if local_mismatch.status != "PINNED" or "local build sha256=" not in local_mismatch.note:
        raise AssertionError(f"local image mismatch: {local_mismatch}")

    no_notes = fixtures / "does-not-exist-notes.md"
    clean_soak = smp_soak_gate(
        fixtures / "smp-boot-soak-clean.md", fixtures / "smp-closeout-pass.md", head, no_notes
    )
    if clean_soak.status != "PASS":
        raise AssertionError(f"clean soak: {clean_soak}")
    failed_soak = smp_soak_gate(
        fixtures / "smp-boot-soak-tcg4-fail.md",
        fixtures / "smp-closeout-pass.md",
        head,
        no_notes,
    )
    if (
        failed_soak.status != "FAIL"
        or "TCG4 4/10" not in failed_soak.note
        or "run 5/10 login timeout" not in failed_soak.note
    ):
        raise AssertionError(f"TCG4 failure soak: {failed_soak}")
    failed_closeout = smp_soak_gate(
        fixtures / "smp-boot-soak-clean.md",
        fixtures / "smp-closeout-fail.md",
        head,
        no_notes,
    )
    if failed_closeout.status != "FAIL" or "readiness timeout" not in failed_closeout.note:
        raise AssertionError(f"failed closeout: {failed_closeout}")
    not_run = smp_soak_gate(
        fixtures / "smp-boot-soak-clean.md",
        fixtures / "smp-closeout-not-run.md",
        head,
        no_notes,
    )
    if not_run.status != "STALE":
        raise AssertionError(f"not-run closeout: {not_run}")
    missing_soak = smp_soak_gate(
        fixtures / "does-not-exist-smp.md", fixtures / "smp-closeout-pass.md", head, no_notes
    )
    if missing_soak.status != "MISSING":
        raise AssertionError(f"missing soak: {missing_soak}")
    ambiguous_soak = smp_soak_gate(
        fixtures / "smp-boot-soak-ambiguous.md",
        fixtures / "smp-closeout-pass.md",
        head,
        no_notes,
    )
    if ambiguous_soak.status != "STALE":
        raise AssertionError(f"ambiguous soak: {ambiguous_soak}")
    notes_failure = smp_soak_gate(
        fixtures / "smp-boot-soak-clean.md",
        fixtures / "does-not-exist-closeout.md",
        head,
        fixtures / "release-notes-soak-fail.md",
    )
    if (
        notes_failure.status != "FAIL"
        or "TCG4 4/10" not in notes_failure.note
        or "run 5/10" not in notes_failure.note
        or "S126" not in notes_failure.note
    ):
        raise AssertionError(f"release-notes failure: {notes_failure}")
    stamp = "2026-11-18T00:00:00Z"
    missing_libc = libc_gate_from_log(
        fixtures / "libc-verdict-missing-evidence.log",
        1,
        stamp,
        stamp,
        fixtures / "libc-gate-report.md",
        fixtures / "libc-decisions.md",
    )
    if (
        missing_libc.status != "STALE"
        or "MET=4 NOT MET=20" not in missing_libc.note
        or "evidence files missing" not in missing_libc.note
    ):
        raise AssertionError(f"missing-evidence libc: {missing_libc}")
    real_libc = libc_gate_from_log(
        fixtures / "libc-verdict-real.log",
        1,
        stamp,
        stamp,
        fixtures / "libc-gate-report.md",
        fixtures / "libc-decisions.md",
    )
    if (
        real_libc.status != "FAIL"
        or "MET=4 NOT MET=20" not in real_libc.note
        or "evidence files missing" in real_libc.note
    ):
        raise AssertionError(f"real libc verdict: {real_libc}")
    deferred_libc = libc_gate_from_log(
        fixtures / "libc-verdict-missing-evidence.log",
        1,
        stamp,
        stamp,
        fixtures / "libc-gate-report.md",
        fixtures / "libc-decisions-dr60.md",
    )
    if (
        deferred_libc.status != "DEFERRED"
        or "MET=4 NOT MET=20" not in deferred_libc.note
        or "DR-60" not in deferred_libc.note
    ):
        raise AssertionError(f"deferred libc: {deferred_libc}")
    libc_1cpu_fail = libc_1cpu_gate(fixtures / "libc-1cpu-fail.md", head)
    if (
        libc_1cpu_fail.status != "FAIL"
        or "0xca3a10" not in libc_1cpu_fail.note
        or "-11" not in libc_1cpu_fail.note
        or "35" not in libc_1cpu_fail.note
        or "not SMP" not in libc_1cpu_fail.note
    ):
        raise AssertionError(f"1-CPU failure: {libc_1cpu_fail}")
    libc_1cpu_clean = libc_1cpu_gate(fixtures / "libc-1cpu-clean.md", head)
    if libc_1cpu_clean.status != "MET":
        raise AssertionError(f"1-CPU clean: {libc_1cpu_clean}")
    libc_1cpu_ambiguous = libc_1cpu_gate(fixtures / "libc-1cpu-ambiguous.md", head)
    if libc_1cpu_ambiguous.status != "STALE":
        raise AssertionError(f"1-CPU ambiguous: {libc_1cpu_ambiguous}")
    libc_1cpu_missing = libc_1cpu_gate(fixtures / "does-not-exist-notes.md", head)
    if libc_1cpu_missing.status != "MISSING":
        raise AssertionError(f"1-CPU missing: {libc_1cpu_missing}")
    libc_1cpu_fixed = libc_1cpu_gate(
        fixtures / "libc-1cpu-fail.md",
        head,
        fixtures / "freeze-checklist-s132-met.md",
    )
    if libc_1cpu_fixed.status != "MET" or "S132" not in libc_1cpu_fixed.note:
        raise AssertionError(f"1-CPU freeze fix: {libc_1cpu_fixed}")
    cold_pending = cold_start_gate(fixtures / "cold-start-pending.md", head)
    if (
        cold_pending.status != "PENDING K139"
        or "1.48" not in cold_pending.note
        or "1.6" not in cold_pending.note
    ):
        raise AssertionError(f"cold-start pending: {cold_pending}")
    cold_met = cold_start_gate(fixtures / "cold-start-met.md", head)
    if cold_met.status != "MET":
        raise AssertionError(f"cold-start MET: {cold_met}")
    cold_ambiguous = cold_start_gate(fixtures / "cold-start-ambiguous.md", head)
    if cold_ambiguous.status != "STALE":
        raise AssertionError(f"cold-start ambiguous: {cold_ambiguous}")
    cold_missing = cold_start_gate(fixtures / "does-not-exist-notes.md", head)
    if cold_missing.status != "MISSING":
        raise AssertionError(f"cold-start missing: {cold_missing}")

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
        "release-evidence self-test: PASS (kernel good/stale/missing/current/"
        "current-stale/partial, posix good/stale/no-stamp/fail, "
        "board good/virt/synthetic/incomplete, image missing/hash/pinned/stale/local-hash, "
        "smp-soak clean/tcg4-fail/closeout-fail/not-run/missing/ambiguous/notes-fail, "
        "libc missing-evidence/real/deferred, "
        "libc-1cpu fail/met/ambiguous/missing/freeze-fixed, "
        "cold-start pending/met/ambiguous/missing)"
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
    parser.add_argument(
        "--packet",
        type=Path,
        default=None,
        help="packet path (default RAVEN_MGMT_ROOT/tickets/release/0.1.0-evidence.md)",
    )
    parser.add_argument(
        "--kernel-gate",
        type=Path,
        default=None,
        help="override the mgmt architecture/product-data/kernel-gate.md",
    )
    parser.add_argument(
        "--posix-report",
        type=Path,
        default=None,
        help="override the mgmt architecture/product-data/posix-conformance.md",
    )
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
