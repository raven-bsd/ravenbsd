#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K264 TCP provenance audit and reproducible evidence packet.

The TCP port wave (K254-K261) sends one exact PROVENANCE.toml entry per NetBSD-ported file.
This tool is the independent audit and the evidence runner:

  --audit     check the crate SPDX surface, the NetBSD imports and the port-claim intake
  --unit      run `cargo nextest run -p raven-tcpip` and record the test counts
  --matrix    run the K253 conformance matrix (Linux baseline + Raven target) if it is in tree
  --all       audit + unit + matrix (default)
  --selftest  pure, fast self-test; no nextest and no QEMU

Every command is recorded in a timestamped packet under tests/evidence/tcp/. A missing K253
harness or a non-root host is recorded as `blocked`, never as a pass. A port report marked DONE
without a matching import is an audit failure; an import for a file whose claim is still pending
is also a failure (no premature provenance).
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
import platform
import re
import shlex
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = "crates/raven-tcpip"
CLAIMS_PATH = "tests/evidence/tcp/port-claims.json"
DEFAULT_CONFORMANCE = "scripts/net-conformance.sh"
NETBSD_REV = "36eddebcc36cb0775aa2962005b9c05419a6ff01"
BSD3 = "BSD-3-Clause"
UNIT_TIMEOUT = 180
SCENARIO_TIMEOUT = 300
SCENARIOS = ("handshake", "zero-window", "window-update", "retransmit", "fin-rst")
ALLOWED_LICENSES = {
    "BSD-3-Clause",
    "BSD-2-Clause",
    "0BSD",
    "MIT",
    "Apache-2.0",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "CC0-1.0",
}
VERSION_TIMEOUT = 10


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def sha256_paths(paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in sorted(set(paths)):
        digest.update(str(path.relative_to(ROOT)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def toml_value(text: str):
    text = text.strip()
    if text == "true":
        return True
    if text == "false":
        return False
    if len(text) >= 2 and text[0] == '"' and text[-1] == '"':
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            return text[1:-1]
    return text


def parse_imports(text: str) -> list[dict]:
    """Parse the flat `[[import]]` tables used by PROVENANCE.toml.

    Only the scalar keys the audit needs are collected. Notes are ignored, so a multi-line note
    cannot affect the result.
    """
    wanted = {
        "path",
        "upstream",
        "upstream_path",
        "upstream_rev",
        "license",
        "tier",
        "copied",
        "modified",
    }
    entries: list[dict] = []
    current: dict | None = None
    for raw in text.splitlines():
        line = raw.strip()
        if line == "[[import]]":
            if current is not None:
                entries.append(current)
            current = {}
            continue
        if current is None or "=" not in line or line.startswith("#"):
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        if key in wanted:
            current[key] = toml_value(value)
    if current is not None:
        entries.append(current)
    return entries


def load_claims(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def matches(path: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def source_files(root: Path) -> list[Path]:
    base = root / CRATE
    files = sorted(base.glob("src/**/*.rs")) + sorted(base.glob("tests/**/*.rs"))
    return [path for path in files if path.is_file()]


def first_lines(path: Path, count: int) -> str:
    try:
        return "\n".join(path.read_text(encoding="utf-8", errors="replace").splitlines()[:count])
    except OSError:
        return ""


def report_state(path: Path) -> dict:
    if not path.is_file():
        return {"present": False, "result": None, "last_status": None}
    text = path.read_text(encoding="utf-8", errors="replace")
    result = None
    match = re.search(r"(?m)^Result:\s*(DONE|PARTIAL|BLOCKED|WORKING)\b", text)
    if match:
        result = match.group(1)
    stamps = re.findall(r"(?m)^-\s+(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ)", text)
    return {"present": True, "result": result, "last_status": stamps[-1] if stamps else None}


def donor_licence(text: str) -> tuple[str, str]:
    """Classify a donor header from its first 120 lines.

    NRL/UCB 4-clause advertising text is NO-COPY under the workspace donor rules. A three-clause
    block is BSD-3-Clause; a two-clause block is BSD-2-Clause.
    """
    head = "\n".join(text.splitlines()[:120])
    if re.search(r"(?i)NRL grants permission|all advertising materials", head):
        return "NO-COPY", "NRL/UCB 4-clause advertising clause"
    if re.search(r"(?m)^\s*\*?\s*3\.\s+Neither the name", head):
        return "BSD-3-Clause", "3-clause BSD"
    if re.search(r"(?m)^\s*\*?\s*2\.\s+Redistributions in binary form", head):
        return "BSD-2-Clause", "2-clause BSD"
    return "UNKNOWN", "no recognised BSD clause block"


def audit(root: Path, claims_path: Path, reports_root: Path | None, donor_root: Path | None = None) -> dict:
    violations: list[str] = []
    warnings: list[str] = []

    provenance_path = root / "PROVENANCE.toml"
    if not provenance_path.is_file():
        return {
            "result": "fail",
            "violations": ["PROVENANCE.toml is missing"],
            "warnings": [],
            "imports": [],
            "claims": [],
            "files_scanned": 0,
            "donor_audit": [],
        }

    imports = [
        entry for entry in parse_imports(provenance_path.read_text(encoding="utf-8"))
        if str(entry.get("path", "")).startswith(CRATE + "/")
    ]
    netbsd_imports = [entry for entry in imports if entry.get("upstream") == "netbsd"]
    other_imports = [entry for entry in imports if entry.get("upstream") != "netbsd"]

    try:
        claims_doc = load_claims(claims_path)
    except (OSError, json.JSONDecodeError) as error:
        return {
            "result": "fail",
            "violations": [f"claims file unreadable: {error}"],
            "warnings": [],
            "imports": imports,
            "claims": [],
            "files_scanned": 0,
            "donor_audit": [],
        }
    claims = claims_doc.get("claims", [])

    files = source_files(root)
    if not files:
        violations.append(f"no Rust files under {CRATE}")
    bad_spdx: list[str] = []
    missing_spdx: list[str] = []
    for path in files:
        header = first_lines(path, 12)
        match = re.search(r"SPDX-License-Identifier:\s*(\S+)", header)
        if not match:
            missing_spdx.append(str(path.relative_to(root)))
        elif match.group(1) not in ALLOWED_LICENSES:
            bad_spdx.append(f"{path.relative_to(root)}: {match.group(1)}")
    if missing_spdx:
        violations.append("missing SPDX header: " + ", ".join(missing_spdx))
    if bad_spdx:
        violations.append("forbidden/unknown SPDX: " + ", ".join(bad_spdx))

    cargo = root / CRATE / "Cargo.toml"
    if cargo.is_file():
        cargo_text = cargo.read_text(encoding="utf-8")
        match = re.search(r"(?m)^license\s*=\s*\"([^\"]+)\"", cargo_text)
        if not match and re.search(r"(?m)^license\.workspace\s*=\s*true\b", cargo_text):
            root_cargo = root / "Cargo.toml"
            if root_cargo.is_file():
                root_text = root_cargo.read_text(encoding="utf-8")
                section = re.search(r"(?ms)^\[workspace\.package\]\s*$.*?(?=^\[|\Z)", root_text)
                if section:
                    match = re.search(r'(?m)^license\s*=\s*"([^"]+)"', section.group(0))
        if not match:
            warnings.append(f"{CRATE}/Cargo.toml has no resolved license field")
        elif match.group(1) != BSD3:
            violations.append(f"{CRATE}/Cargo.toml license is {match.group(1)}, not {BSD3}")

    port_claims = [claim for claim in claims if claim.get("kind") == "port"]
    claim_status = {claim.get("ticket"): claim.get("status") for claim in claims}

    for entry in imports:
        path = str(entry.get("path", ""))
        target = root / path
        if not target.is_file():
            violations.append(f"import path does not exist: {path}")
            continue
        license_name = str(entry.get("license", ""))
        if license_name not in ALLOWED_LICENSES:
            violations.append(f"{path}: license {license_name!r} is not in the allowed set")
        header = first_lines(target, 12)
        if not re.search(rf"SPDX-License-Identifier:\s*{re.escape(license_name)}\b", header):
            violations.append(f"{path}: header does not carry the recorded {license_name} SPDX")

        if entry.get("upstream") != "netbsd":
            if not entry.get("upstream_rev"):
                violations.append(f"{path}: non-NetBSD import has no upstream_rev")
            continue

        matching = [
            claim for claim in port_claims
            if matches(path, claim.get("paths", []))
        ]
        if not matching:
            violations.append(f"{path}: import has no port claim")
            expected = BSD3
        else:
            copies = {str(claim.get("copy")) for claim in matching}
            tickets = ", ".join(str(claim.get("ticket")) for claim in matching)
            if "clean-room" in copies:
                violations.append(
                    f"{path}: clean-room claim ({tickets}) must not have a NetBSD import"
                )
            expected = "BSD-2-Clause" if "permitted-bsd2" in copies else BSD3
        if license_name != expected:
            violations.append(
                f"{path}: license {license_name!r} does not match the claim donor licence {expected}"
            )
        if entry.get("tier") != "A":
            violations.append(f"{path}: tier is {entry.get('tier')!r}, expected 'A'")
        if entry.get("upstream_rev") != NETBSD_REV:
            violations.append(
                f"{path}: revision {entry.get('upstream_rev')!r} is not the wave pin {NETBSD_REV}"
            )
        donor_path = str(entry.get("upstream_path", ""))
        if not re.search(r"sys/(netinet|kern|sys)/.+\.(c|h)", donor_path):
            violations.append(f"{path}: upstream_path is not a NetBSD TCP/kernel path: {donor_path!r}")

        if matching and not any(claim.get("status") == "landed" for claim in matching):
            tickets = ", ".join(str(claim.get("ticket")) for claim in matching)
            if not any(claim.get("copy") == "clean-room" for claim in matching):
                violations.append(f"{path}: import is premature; matching claims are pending ({tickets})")

    for claim in claims:
        ticket = claim.get("ticket", "?")
        status = claim.get("status")
        kind = claim.get("kind")
        copy = claim.get("copy")
        paths = claim.get("paths", [])
        if status not in ("pending", "pending-merge", "landed", "clean-room", "blocked"):
            violations.append(f"{ticket}: unknown claim status {status!r}")
            continue
        if kind not in ("port", "test-only"):
            violations.append(f"{ticket}: unknown claim kind {kind!r}")
            continue
        if kind == "port" and copy not in ("clean-room", "permitted", "permitted-bsd2"):
            violations.append(f"{ticket}: port claim copy decision is {copy!r}")

        matching_all = [entry for entry in imports if matches(str(entry.get("path", "")), paths)]
        matching_imports = [entry for entry in matching_all if entry.get("upstream") == "netbsd"]
        if kind == "test-only" and matching_all:
            violations.append(
                f"{ticket}: test-only claim has product imports: "
                + ", ".join(str(entry.get("path")) for entry in matching_all)
            )
        if kind == "port" and copy == "clean-room" and matching_imports:
            violations.append(
                f"{ticket}: clean-room claim has NetBSD imports: "
                + ", ".join(str(entry.get("path")) for entry in matching_imports)
            )
        if kind == "port" and copy != "clean-room" and status == "clean-room":
            violations.append(f"{ticket}: only a clean-room claim may use status=clean-room")
        if kind == "port" and copy == "clean-room" and status == "landed":
            violations.append(f"{ticket}: a clean-room claim cannot be landed (no import is allowed)")
        if kind == "port" and status == "landed" and copy != "clean-room":
            missing: list[str] = []
            for path in paths:
                full = root / path
                if full.is_file() and full.suffix == ".rs":
                    if not any(entry.get("path") == path for entry in netbsd_imports):
                        missing.append(path)
                elif any(char in path for char in "*?["):
                    for existing in root.glob(path):
                        if existing.is_file() and existing.suffix == ".rs":
                            rel = str(existing.relative_to(root))
                            if not any(entry.get("path") == rel for entry in netbsd_imports):
                                missing.append(rel)
            if missing:
                violations.append(f"{ticket}: landed but missing imports: " + ", ".join(missing))
        if (
            kind == "port"
            and status in ("pending", "pending-merge")
            and copy != "clean-room"
            and matching_imports
        ):
            violations.append(
                f"{ticket}: pending claim already has imports: "
                + ", ".join(str(entry.get("path")) for entry in matching_imports)
            )
        for rejected in claim.get("rejected_entries", []):
            rejected_path = str(rejected.get("path"))
            if any(entry.get("path") == rejected_path for entry in netbsd_imports):
                violations.append(f"{ticket}: rejected entry was added: {rejected_path}")

        if reports_root is not None and claim.get("report"):
            report_path = reports_root / str(claim["report"])
            report = report_state(report_path)
            claim["report_result"] = report["result"]
            claim["report_last_status"] = report["last_status"]
            if not report["present"]:
                warnings.append(f"{ticket}: report is missing: {report_path}")
            elif report["result"] == "DONE":
                if kind == "port":
                    if copy == "clean-room" and status != "clean-room":
                        violations.append(
                            f"{ticket}: DONE clean-room report but status is {status}; "
                            "set status=clean-room and do not add an entry"
                        )
                    elif copy != "clean-room" and status != "landed":
                        violations.append(
                            f"{ticket}: DONE permitted port but status is {status}; "
                            "add the exact entry and set status=landed"
                        )
                elif kind == "test-only" and status == "pending-merge":
                    warnings.append(f"{ticket}: test-only report is DONE; merge is still pending")
                elif status == "blocked":
                    warnings.append(f"{ticket}: claim is blocked but the report is DONE")
            elif status == "landed" and report["result"] != "DONE":
                warnings.append(f"{ticket}: claim is landed but report is {report['result']}")
            elif status == "blocked" and report["result"] not in ("BLOCKED", "PARTIAL", None):
                warnings.append(f"{ticket}: claim is blocked but report is {report['result']}")

    for path in files:
        top = first_lines(path, 60)
        if re.search(r"(?i)\bported\s+from\s+(the\s+)?netbsd\b", top):
            rel = str(path.relative_to(root))
            if not any(entry.get("path") == rel for entry in netbsd_imports):
                violations.append(f"{rel}: says 'ported from NetBSD' but has no import")
        if re.search(r"(?i)(copyright.*netbsd\s+foundation|copyright.*wide\s+project)", top):
            rel = str(path.relative_to(root))
            if not any(entry.get("path") == rel for entry in netbsd_imports):
                violations.append(f"{rel}: carries a NetBSD/WIDE copyright but has no import")

    donor_audit: list[dict] = []
    claim_licences: dict[str, list[str]] = {}
    if donor_root is not None:
        for claim in claims:
            ticket = str(claim.get("ticket"))
            for check in claim.get("donor_checks", []):
                rel = str(check.get("file"))
                full = donor_root / rel
                row = {
                    "ticket": ticket,
                    "file": rel,
                    "symbols": [],
                    "missing": [],
                    "licence": None,
                    "licence_reason": None,
                }
                if not full.is_file():
                    violations.append(f"{ticket}: donor file missing: {rel}")
                    row["missing"] = list(check.get("symbols", []))
                else:
                    text = full.read_text(encoding="utf-8", errors="replace")
                    row["licence"], row["licence_reason"] = donor_licence(text)
                    claim_licences.setdefault(ticket, []).append(str(row["licence"]))
                    for symbol in check.get("symbols", []):
                        found = symbol in text
                        row["symbols"].append({"name": symbol, "found": found})
                        if not found:
                            row["missing"].append(symbol)
                            violations.append(
                                f"{ticket}: donor symbol missing in {rel}: {symbol}"
                            )
                donor_audit.append(row)
            divergence = claim.get("divergence")
            if divergence:
                warnings.append(
                    f"{ticket}: divergence needs reviewer approval: {divergence}"
                )
            rejected = claim.get("rejected_entries", [])
            if rejected:
                warnings.append(
                    f"{ticket}: {len(rejected)} proposed provenance entr"
                    f"{'y' if len(rejected) == 1 else 'ies'} rejected by the licence audit"
                )
        for claim in claims:
            if claim.get("kind") != "port":
                continue
            ticket = str(claim.get("ticket"))
            licences = claim_licences.get(ticket, [])
            if not licences:
                continue
            if "NO-COPY" in licences or "UNKNOWN" in licences:
                expected = "clean-room"
            elif "BSD-2-Clause" in licences:
                expected = "permitted-bsd2"
            else:
                expected = "permitted"
            claim["donor_licence"] = sorted(set(licences))
            claim["expected_copy"] = expected
            if claim.get("copy") != expected:
                violations.append(
                    f"{ticket}: claim copy decision {claim.get('copy')!r} does not match "
                    f"the donor licence audit {expected!r} ({sorted(set(licences))})"
                )
    else:
        warnings.append("donor root not provided; donor symbols and licences were not verified")

    donors_lock = root.parent / "ravenbsd-resources" / "donors.lock"
    lock_state = {"path": str(donors_lock), "present": donors_lock.is_file(), "netbsd": None}
    if donors_lock.is_file():
        for line in donors_lock.read_text(encoding="utf-8", errors="replace").splitlines():
            if "netbsd" in line.lower():
                lock_state["netbsd"] = line.strip()
                break
        if lock_state["netbsd"] is None:
            warnings.append("donors.lock has no NetBSD entry")
    else:
        warnings.append(f"donors lock not found at {donors_lock}; revision check skipped")

    return {
        "result": "pass" if not violations else "fail",
        "root": str(root),
        "netbsd_revision": NETBSD_REV,
        "imports": imports,
        "netbsd_imports": netbsd_imports,
        "other_imports": other_imports,
        "claims": claims,
        "claim_status": claim_status,
        "files_scanned": len(files),
        "violations": violations,
        "warnings": warnings,
        "donors_lock": lock_state,
        "donor_audit": donor_audit,
        "rejected_entries": sum(len(claim.get("rejected_entries", [])) for claim in claims),
        "no_copy_claims": sorted(
            str(claim.get("ticket")) for claim in claims
            if claim.get("expected_copy") == "clean-room"
        ),
    }


def conformance_path(root: Path) -> Path:
    value = os.environ.get("TCP_EVIDENCE_CONFORMANCE", DEFAULT_CONFORMANCE)
    path = Path(value)
    return path if path.is_absolute() else root / path


def matrix_blocker(root: Path) -> str | None:
    path = conformance_path(root)
    if not path.is_file():
        return f"{path.relative_to(root) if path.is_relative_to(root) else path} is absent (K253 harness not merged)"
    harness = root / "tests/net-conformance/harness.py"
    if not harness.is_file():
        return "tests/net-conformance/harness.py is absent (K253 harness incomplete)"
    return None


def run_command(command: list[str], timeout: int, log: Path, cwd: Path, env: dict | None = None) -> dict:
    started = utc_now()
    start = time.monotonic()
    merged = os.environ.copy()
    merged.update({"CARGO_TERM_COLOR": "never", "NO_COLOR": "1"})
    if env:
        merged.update(env)
    log.parent.mkdir(parents=True, exist_ok=True)
    try:
        proc = subprocess.run(
            command,
            cwd=cwd,
            env=merged,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout,
        )
        stdout, stderr, code, timed_out = proc.stdout, proc.stderr, proc.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout.decode(errors="replace") if isinstance(error.stdout, bytes) else (error.stdout or "")
        stderr = error.stderr.decode(errors="replace") if isinstance(error.stderr, bytes) else (error.stderr or "")
        code, timed_out = 124, True
    duration = round(time.monotonic() - start, 3)
    with log.open("w", encoding="utf-8") as handle:
        handle.write(f"$ {shlex.join(command)}\n")
        handle.write(f"started: {started}\n")
        handle.write(f"timeout: {timeout}s\n")
        handle.write(f"exit: {code}{' (timeout)' if timed_out else ''}\n")
        handle.write(f"duration: {duration}s\n")
        handle.write("--- stdout ---\n")
        handle.write(stdout)
        if stdout and not stdout.endswith("\n"):
            handle.write("\n")
        handle.write("--- stderr ---\n")
        handle.write(stderr)
        if stderr and not stderr.endswith("\n"):
            handle.write("\n")
    return {
        "command": shlex.join(command),
        "timeout_s": timeout,
        "started": started,
        "duration_s": duration,
        "exit": code,
        "timed_out": timed_out,
        "log": str(log),
        "stdout": stdout,
        "stderr": stderr,
    }


def parse_unit(log_text: str) -> dict:
    match = re.search(
        r"Summary \[\s*([0-9.]+)s\]\s+(\d+) tests run:\s+(\d+) passed,\s+(\d+) skipped(?:,\s+(\d+) failed)?",
        log_text,
    )
    if not match:
        return {"duration_s": None, "run": None, "passed": None, "skipped": None, "failed": None}
    return {
        "duration_s": float(match.group(1)),
        "run": int(match.group(2)),
        "passed": int(match.group(3)),
        "skipped": int(match.group(4)),
        "failed": int(match.group(5) or 0),
    }


def phase_audit(
    root: Path,
    packet: Path,
    claims: Path,
    reports_root: Path | None,
    donor_root: Path | None,
) -> dict:
    result = audit(root, claims, reports_root, donor_root)
    (packet / "audit.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    lines = [
        f"tcp-provenance: {result['result']}",
        f"netbsd_revision: {result['netbsd_revision']}",
        f"files_scanned: {result['files_scanned']}",
        f"imports: {len(result['imports'])} "
        f"(netbsd: {len(result['netbsd_imports'])}, other: {len(result['other_imports'])})",
        f"claims: {len(result['claims'])}",
        f"donor_files: {len(result['donor_audit'])}",
        f"no_copy_claims: {len(result.get('no_copy_claims', []))}",
        f"rejected_entries: {result.get('rejected_entries', 0)}",
        f"violations: {len(result['violations'])}",
        f"warnings: {len(result['warnings'])}",
    ]
    for claim in result["claims"]:
        report = claim.get("report_result")
        extra = f" report={report}" if report else ""
        lines.append(f"  claim {claim.get('ticket')}: {claim.get('kind')}/{claim.get('status')}{extra}")
    for violation in result["violations"]:
        lines.append(f"  VIOLATION: {violation}")
    for warning in result["warnings"]:
        lines.append(f"  warning: {warning}")
    text = "\n".join(lines) + "\n"
    (packet / "audit.txt").write_text(text, encoding="utf-8")
    return {
        "name": "audit",
        "status": result["result"],
        "imports": len(result["imports"]),
        "netbsd_imports": len(result["netbsd_imports"]),
        "other_imports": len(result["other_imports"]),
        "claims": len(result["claims"]),
        "donor_files": len(result["donor_audit"]),
        "no_copy_claims": len(result.get("no_copy_claims", [])),
        "rejected_entries": result.get("rejected_entries", 0),
        "violations": result["violations"],
        "warnings": result["warnings"],
    }


def phase_unit(root: Path, packet: Path) -> dict:
    run = run_command(
        ["cargo", "nextest", "run", "-p", "raven-tcpip"],
        UNIT_TIMEOUT,
        packet / "logs" / "unit.log",
        root,
    )
    counts = parse_unit(run["stdout"] + run["stderr"])
    if run["timed_out"]:
        status = "blocked"
    elif run["exit"] == 0 and (counts["failed"] in (0, None)):
        status = "pass"
    else:
        status = "fail"
    data = {
        "name": "unit",
        "status": status,
        "command": run["command"],
        "exit": run["exit"],
        "timed_out": run["timed_out"],
        "duration_s": run["duration_s"],
        "counts": counts,
        "log": "logs/unit.log",
    }
    (packet / "unit.json").write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return data


def phase_matrix(root: Path, packet: Path) -> dict:
    blocker = matrix_blocker(root)
    if blocker is not None:
        data = {
            "name": "matrix",
            "status": "blocked",
            "reason": blocker,
            "scenarios": [],
            "log": None,
        }
        (packet / "matrix.json").write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        (packet / "matrix-blocked.txt").write_text(
            f"tcp-evidence matrix: BLOCKED\nreason: {blocker}\n"
            "task: K253 (packet-level TCP conformance harness) must land before this phase can run.\n",
            encoding="utf-8",
        )
        return data

    conformance = conformance_path(root)
    rows: list[dict] = []
    built = False
    for target in ("linux", "raven"):
        for scenario in SCENARIOS:
            log = packet / "logs" / f"matrix-{target}-{scenario}.log"
            pcap = packet / "pcaps" / f"{target}-{scenario}.pcap"
            serial = packet / "serial" / f"{target}-{scenario}.log"
            pcap.parent.mkdir(parents=True, exist_ok=True)
            serial.parent.mkdir(parents=True, exist_ok=True)
            command = ["sh", str(conformance), "--scenario", scenario]
            if target == "linux":
                command += ["--baseline", "linux"]
            else:
                command += ["--target", "raven"]
                if built:
                    command += ["--no-build"]
            command += ["--pcap", str(pcap), "--serial", str(serial)]
            run = run_command(command, SCENARIO_TIMEOUT, log, root)
            if target == "raven" and run["exit"] == 0:
                built = True
            if run["exit"] == 0:
                status = "pass"
            elif run["exit"] in (77, 124):
                status = "blocked"
            else:
                status = "fail"
            detail = ""
            for line in reversed((run["stdout"] + "\n" + run["stderr"]).splitlines()):
                if line.strip():
                    detail = line.strip()[:300]
                    break
            rows.append(
                {
                    "target": target,
                    "scenario": scenario,
                    "status": status,
                    "exit": run["exit"],
                    "duration_s": run["duration_s"],
                    "detail": detail,
                    "log": str(log.relative_to(packet)),
                    "pcap": str(pcap.relative_to(packet)) if pcap.is_file() else None,
                    "serial": str(serial.relative_to(packet)) if serial.is_file() else None,
                }
            )
    if any(row["status"] == "fail" for row in rows):
        status = "fail"
    elif any(row["status"] == "blocked" for row in rows):
        status = "blocked"
    else:
        status = "pass"
    data = {"name": "matrix", "status": status, "reason": None, "scenarios": rows}
    (packet / "matrix.json").write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return data


def host_info(root: Path) -> dict:
    info = {
        "utc": utc_now(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "commit": None,
        "branch": None,
        "dirty": None,
    }
    for key, command in (
        ("commit", ["git", "rev-parse", "HEAD"]),
        ("branch", ["git", "branch", "--show-current"]),
        ("dirty", ["git", "status", "--porcelain"]),
        ("cargo", ["cargo", "--version"]),
        ("rustc", ["rustc", "--version"]),
        ("qemu", ["qemu-system-x86_64", "--version"]),
    ):
        try:
            proc = subprocess.run(
                command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                text=True, timeout=VERSION_TIMEOUT,
            )
            value = proc.stdout.strip()
            if key == "dirty":
                lines = [
                    line for line in value.splitlines()
                    if not line[3:].startswith("tests/evidence/tcp/run-")
                ]
                value = "yes" if lines else "no"
            info[key] = value.splitlines()[0] if value else ""
        except (OSError, subprocess.TimeoutExpired):
            info[key] = None
    return info


def input_paths(root: Path, claims: Path) -> list[Path]:
    paths = [
        root / "PROVENANCE.toml",
        root / "Cargo.lock",
        root / CRATE / "Cargo.toml",
        claims,
        root / "scripts" / "tcp-evidence.py",
        root / "scripts" / "tcp-evidence.sh",
        root / "tests" / "evidence" / "tcp" / "README.md",
    ]
    paths += source_files(root)
    return [path for path in paths if path.is_file()]


def write_command_log(packet: Path, phases: list[dict]) -> None:
    lines = [f"tcp-evidence packet: {packet.name}", f"generated: {utc_now()}"]
    for phase in phases:
        lines.append(
            f"phase {phase['name']}: {phase['status']}"
            + (f" command={phase['command']!r}" if phase.get("command") else "")
            + (f" reason={phase['reason']!r}" if phase.get("reason") else "")
        )
    (packet / "command.log").write_text("\n".join(lines) + "\n", encoding="utf-8")


def summarize(packet: Path, info: dict, phases: list[dict]) -> int:
    statuses = [phase["status"] for phase in phases]
    if "fail" in statuses:
        overall, exit_code = "fail", 1
    elif "blocked" in statuses:
        overall, exit_code = "partial", 2
    else:
        overall, exit_code = "pass", 0
    summary = {
        "result": overall,
        "exit": exit_code,
        "generated": utc_now(),
        "commit": info.get("commit"),
        "dirty": info.get("dirty"),
        "phases": phases,
    }
    (packet / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    lines = [
        f"tcp-evidence: {overall}",
        f"commit: {info.get('commit')}",
        f"dirty: {info.get('dirty')}",
    ]
    for phase in phases:
        extra = ""
        if phase["name"] == "unit" and phase.get("counts"):
            counts = phase["counts"]
            if counts.get("run") is not None:
                extra = f" {counts['passed']}/{counts['run']} passed, {counts['failed']} failed"
        if phase["name"] == "matrix" and phase.get("reason"):
            extra = f" ({phase['reason']})"
        lines.append(f"{phase['name']}: {phase['status']}{extra}")
    lines.append(f"packet: {packet}")
    lines.append(f"exit: {exit_code}")
    text = "\n".join(lines) + "\n"
    (packet / "summary.txt").write_text(text, encoding="utf-8")
    return exit_code


def write_hashes(packet: Path) -> None:
    rows = []
    for path in sorted(packet.rglob("*")):
        if path.is_file() and path.name != "sha256sums.txt":
            rows.append(f"{sha256_file(path)}  {path.relative_to(packet)}")
    (packet / "sha256sums.txt").write_text("\n".join(rows) + "\n", encoding="utf-8")


def command_run(root: Path, args) -> int:
    claims = Path(args.claims)
    if not claims.is_absolute():
        claims = root / claims
    reports_root = Path(args.reports_root) if args.reports_root else None
    donor_root = Path(args.donor_root) if args.donor_root else None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    packet = Path(args.out) if args.out else root / "tests" / "evidence" / "tcp" / f"run-{stamp}"
    if packet.exists() and any(packet.iterdir()):
        print(f"tcp-evidence: refusing to overwrite non-empty packet {packet}", file=sys.stderr)
        return 1
    packet.mkdir(parents=True, exist_ok=True)

    info = host_info(root)
    manifest = {
        "tool": "scripts/tcp-evidence.py",
        "packet": packet.name,
        "generated": utc_now(),
        "host": info,
        "reports_root": str(reports_root) if reports_root else None,
        "donor_root": str(donor_root) if donor_root else None,
        "claims": str(claims),
        "inputs_sha256": sha256_paths(input_paths(root, claims)),
        "netbsd_revision": NETBSD_REV,
    }
    (packet / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    phases: list[dict] = []
    if args.audit or args.all:
        phases.append(phase_audit(root, packet, claims, reports_root, donor_root))
    if args.unit or args.all:
        phases.append(phase_unit(root, packet))
    if args.matrix or args.all:
        phases.append(phase_matrix(root, packet))

    write_command_log(packet, phases)
    exit_code = summarize(packet, info, phases)
    write_hashes(packet)
    print((packet / "summary.txt").read_text(encoding="utf-8"), end="")
    print(f"tcp-evidence: packet {packet}")
    return exit_code


def selftest(root: Path) -> int:
    sample = """
[[import]]
path = "a.rs"
upstream = "netbsd"
upstream_path = "sys/netinet/tcp_output.c"
upstream_rev = "36eddebcc36cb0775aa2962005b9c05419a6ff01"
license = "BSD-3-Clause"
tier = "A"
copied = "2026-11-20"
modified = true

[[import]]
path = "b.rs"
upstream = "netbsd"
"""
    entries = parse_imports(sample)
    assert len(entries) == 2, entries
    assert entries[0]["path"] == "a.rs" and entries[1]["path"] == "b.rs"
    assert entries[0]["modified"] is True
    assert matches("crates/raven-tcpip/src/x.rs", ["crates/raven-tcpip/src/*.rs"])
    assert not matches("crates/raven-tcpip/src/x.rs", ["tests/**"])

    claims = root / CLAIMS_PATH
    result = audit(root, claims, None)
    if result["result"] != "pass":
        print("tcp-evidence-selftest: FAIL audit", file=sys.stderr)
        for violation in result["violations"]:
            print(f"  {violation}", file=sys.stderr)
        return 1
    if result["netbsd_imports"]:
        print("tcp-evidence-selftest: FAIL unexpected TCP NetBSD imports", file=sys.stderr)
        return 1
    if not claims.is_file():
        print("tcp-evidence-selftest: FAIL claims file missing", file=sys.stderr)
        return 1
    doc = load_claims(claims)
    tickets = {claim.get("ticket") for claim in doc.get("claims", [])}
    needed = {f"K{number}" for number in range(253, 264)}
    missing = sorted(needed - tickets)
    if missing:
        print(f"tcp-evidence-selftest: FAIL claims missing {missing}", file=sys.stderr)
        return 1

    saved = os.environ.get("TCP_EVIDENCE_CONFORMANCE")
    os.environ["TCP_EVIDENCE_CONFORMANCE"] = "/nonexistent/net-conformance.sh"
    blocker = matrix_blocker(root)
    if blocker is None:
        print("tcp-evidence-selftest: FAIL missing matrix blocker not detected", file=sys.stderr)
        return 1
    if saved is None:
        del os.environ["TCP_EVIDENCE_CONFORMANCE"]
    else:
        os.environ["TCP_EVIDENCE_CONFORMANCE"] = saved

    digest = sha256_bytes(b"k264")
    if digest != hashlib.sha256(b"k264").hexdigest():
        print("tcp-evidence-selftest: FAIL sha256", file=sys.stderr)
        return 1
    print(
        "tcp-evidence-selftest: PASS "
        f"netbsd_imports=0 other_imports={len(result['other_imports'])} "
        f"claims={len(doc.get('claims', []))} matrix_blocker=detected"
    )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--audit", action="store_true", help="run the provenance/claims audit")
    parser.add_argument("--unit", action="store_true", help="run cargo nextest -p raven-tcpip")
    parser.add_argument("--matrix", action="store_true", help="run the K253 conformance matrix")
    parser.add_argument("--all", action="store_true", help="audit + unit + matrix")
    parser.add_argument("--selftest", action="store_true", help="fast self-test; no nextest/QEMU")
    parser.add_argument("--out", help="packet directory (default tests/evidence/tcp/run-<UTC>)")
    parser.add_argument("--claims", default=CLAIMS_PATH, help="port claims JSON")
    parser.add_argument("--reports-root", help="tickets/ root for the report DONE/pending check")
    parser.add_argument("--donor-root", help="NetBSD donor checkout for the exact symbol checks")
    parser.add_argument("--reports-strict", action="store_true", help="(reserved) fail on missing reports")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = ROOT
    if args.selftest:
        return selftest(root)
    if not (args.audit or args.unit or args.matrix or args.all):
        args.all = True
    return command_run(root, args)


if __name__ == "__main__":
    sys.exit(main())
