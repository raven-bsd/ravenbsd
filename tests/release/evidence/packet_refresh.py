#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Refresh the 0.1.0 release evidence packet from measured sources.

The release gate board is the gate list.  The kernel gate report, the POSIX
conformance report, the TCP RACK register and the SSH gate TSV are the only
measured sources used for the board's measured rows.  A row whose measured
source is not at the current product HEAD is UNVERIFIED, never copied from an
older packet.  Packet-local checks (fast gate, served-copy check, evidence
index check) are recorded separately.

`--write` writes only ``$RAVEN_MGMT_ROOT/tickets/evidence/release``.
`--check` only reads and compares.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path

import mgmt

BOARD_REL = "tickets/release/0.1.0-gates.tsv"
KERNEL_REL = "architecture/product-data/kernel-gate.md"
POSIX_REL = "architecture/product-data/posix-conformance.md"
TCP_REGISTER_REL = "tickets/evidence/tcp/release/k295/logs/k295-r3-rack-register.log"
TCP_MANIFEST_REL = "tickets/evidence/tcp/release/manifest.json"
SSH_REL = "tickets/ssh/gate.tsv"
SERVED_REL = "architecture/product-data/SERVED.tsv"
PACKET_REL = "tickets/evidence/release"
GENERATOR_REL = "tests/release/evidence/packet_refresh.py"
SERVED_SCRIPT_REL = "scripts/served-copies.sh"
INDEX_SCRIPT_REL = "scripts/evidence-index.sh"

PACKET_FILES = (
    "README.md",
    "gates.tsv",
    "check-fast.log",
    "served-copies-check.log",
    "evidence-index-check.log",
)

SOURCE_KEYS = {
    "Kernel gate": KERNEL_REL,
    "POSIX gate": POSIX_REL,
    "TCP stack release-grade": TCP_REGISTER_REL,
    "SSH remote login": SSH_REL,
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def size(path: Path) -> int:
    return path.stat().st_size


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def git_head(root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip() or "unknown"


def is_current(recorded: str, head: str) -> bool:
    if not recorded or recorded in {"-", "unknown"} or not head or head == "unknown":
        return False
    return head.startswith(recorded) or recorded.startswith(head)


def clean(value: str) -> str:
    return " ".join(value.replace("\t", " ").split())


def parse_kernel(path: Path) -> tuple[str, str]:
    text = read(path)
    commit = re.search(r"`([0-9a-f]{40})`", text)
    result = re.search(r"^Result: \*\*(.+?)\*\* .*? (.+)$", text, re.MULTILINE)
    if result:
        detail = f"{result.group(1)}; {clean(result.group(2))}"
    else:
        detail = "kernel gate report present"
    return (commit.group(1) if commit else "unknown"), detail


def parse_posix(path: Path) -> tuple[str, str]:
    text = read(path)
    commit = re.search(r"^- Commit: `([^`]*)`", text, re.MULTILINE)
    result = re.search(r"^- Result: \*\*([^*]+)\*\*", text, re.MULTILINE)
    counts = re.search(r"^- Counts: (.+)$", text, re.MULTILINE)
    bits = []
    if result:
        bits.append(result.group(1).strip())
    if counts:
        bits.append(clean(counts.group(1)))
    return (commit.group(1) if commit else "unknown"), "; ".join(bits) or "POSIX report present"


def parse_tcp(manifest: Path, register: Path) -> tuple[str, str]:
    commit = "unknown"
    if manifest.is_file():
        try:
            data = json.loads(read(manifest))
            commit = str(data.get("commit", "unknown"))
        except (OSError, ValueError):
            commit = "unknown"
    if register.is_file():
        line = clean(read(register).strip().splitlines()[-1]) if read(register).strip() else ""
        detail = line or "TCP register present"
    else:
        detail = "TCP register missing"
    return commit, detail


def parse_ssh(path: Path) -> tuple[str, str]:
    commits: Counter[str] = Counter()
    statuses: Counter[str] = Counter()
    for line in read(path).splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) < 3 or fields[0] == "row":
            continue
        statuses[fields[1]] += 1
        if fields[2]:
            commits[fields[2]] += 1
    commit = commits.most_common(1)[0][0] if commits else "unknown"
    detail = ", ".join(f"{key} {value}" for key, value in sorted(statuses.items()))
    if len(commits) > 1:
        detail += "; commits " + ", ".join(sorted(commits))
    return commit, detail or "SSH gate present"


def board_rows(path: Path) -> list[tuple[str, str, str, str]]:
    rows: list[tuple[str, str, str, str]] = []
    for line in read(path).splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if fields[0] == "gate":
            continue
        if len(fields) < 4:
            continue
        rows.append((fields[0], fields[1], fields[2], fields[3]))
    return rows


def packet_check(
    gate: str, path: Path, head: str, pattern: re.Pattern[str], note_default: str
) -> dict[str, str]:
    if not path.is_file():
        return {
            "gate": gate,
            "status": "MISSING",
            "verification": "UNVERIFIED",
            "commit": "-",
            "source": f"tickets/evidence/release/{path.name}",
            "sha256": "-",
            "note": "packet log missing",
        }
    text = read(path)
    match = pattern.search(text)
    note = clean(match.group(0)) if match else note_default
    return {
        "gate": gate,
        "status": "PASS",
        "verification": "CURRENT",
        "commit": head,
        "source": f"tickets/evidence/release/{path.name}",
        "sha256": sha256(path),
        "note": note,
    }


def build_rows(root: Path, mgmt_root: Path, packet_dir: Path) -> list[dict[str, str]]:
    head = git_head(root)

    board = mgmt_root / BOARD_REL
    kernel = mgmt_root / KERNEL_REL
    posix = mgmt_root / POSIX_REL
    tcp_manifest = mgmt_root / TCP_MANIFEST_REL
    tcp_register = mgmt_root / TCP_REGISTER_REL
    ssh = mgmt_root / SSH_REL

    board_sha = sha256(board)
    kernel_commit, kernel_note = parse_kernel(kernel)
    posix_commit, posix_note = parse_posix(posix)
    tcp_commit, tcp_note = parse_tcp(tcp_manifest, tcp_register)
    ssh_commit, ssh_note = parse_ssh(ssh)

    rows: list[dict[str, str]] = []
    for gate, status, evidence, owner in board_rows(board):
        if gate == "Kernel gate":
            source = KERNEL_REL
            source_path = kernel
            commit = kernel_commit
            note = f"{kernel_note}; board status {status}; {owner}"
        elif gate == "POSIX gate":
            source = POSIX_REL
            source_path = posix
            commit = posix_commit
            note = f"{posix_note}; board status {status}; {owner}"
        elif gate == "TCP stack release-grade":
            source = TCP_REGISTER_REL
            source_path = tcp_register
            commit = tcp_commit
            note = f"{tcp_note}; board status {status}; {owner}"
        elif gate == "SSH remote login":
            source = SSH_REL
            source_path = ssh
            commit = ssh_commit
            note = f"{ssh_note}; board status {status}; {owner}"
        else:
            source = BOARD_REL
            source_path = board
            commit = "-"
            note = f"board-only; no current measurement; {clean(evidence)}; {owner}"
        verification = "CURRENT" if source != BOARD_REL and is_current(commit, head) else "UNVERIFIED"
        rows.append(
            {
                "gate": gate,
                "status": status,
                "verification": verification,
                "commit": commit,
                "source": source,
                "sha256": sha256(source_path),
                "note": note,
            }
        )

    rows.append(
        packet_check(
            "packet:host-fast",
            packet_dir / "check-fast.log",
            head,
            re.compile(r"check-fast: PASSED|check-fast: FAILED"),
            "fast gate log present",
        )
    )
    rows.append(
        packet_check(
            "packet:served-copies",
            packet_dir / "served-copies-check.log",
            head,
            re.compile(r"served-copies: PASS.*"),
            "served-copy check log present",
        )
    )

    return rows


def render_gates(rows: list[dict[str, str]]) -> str:
    lines = [
        "# SPDX-License-Identifier: BSD-3-Clause",
        "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        "# GENERATED by tests/release/evidence/packet_refresh.py --write; do not edit.",
        "gate\tboard_status\tverification\tmeasured_commit\tsource\tsource_sha256\tnote",
    ]
    for row in rows:
        lines.append(
            "\t".join(
                clean(row[key])
                for key in ("gate", "status", "verification", "commit", "source", "sha256", "note")
            )
        )
    return "\n".join(lines) + "\n"


def render_readme(rows: list[dict[str, str]], head: str, dirty: bool) -> str:
    unverified = [row for row in rows if row["verification"] != "CURRENT"]
    blockers = [
        row
        for row in rows
        if row["status"] in {"BLOCKED", "HARDWARE", "MISSING", "NO-GO"}
    ]
    lines = [
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->",
        "<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->",
        "",
        "# RavenBSD 0.1.0 release evidence packet (refreshed)",
        "",
        f"- Product HEAD: `{head}`",
        f"- Working tree: `{'dirty' if dirty else 'clean'}`",
        "- Verdict: **NO-GO** (no tag; stale or blocked gates remain)",
        "- Scope: the release gate board measured against the current product HEAD.",
        "",
        "## Gate table",
        "",
        "| Gate | Board | Verification | Measured commit | Source | Source SHA-256 | Note |",
        "|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        note = clean(row["note"])
        if len(note) > 180:
            note = note[:177] + "..."
        escaped_note = note.replace("|", "\\|")
        lines.append(
            f"| {row['gate']} | {row['status']} | {row['verification']} | "
            f"`{row['commit']}` | `{row['source']}` | `{row['sha256']}` | {escaped_note} |"
        )
    lines.extend(
        [
            "",
            "## UNVERIFIED",
            "",
            f"{len(unverified)} of {len(rows)} rows have no current measurement at `{head}`.",
            "A measurement counts as current only when its recorded commit is the current HEAD;",
            "an ancestor or a missing stamp is not current evidence for this tree.",
            "",
        ]
    )
    mapped = [row for row in rows if row["source"] in SOURCE_KEYS.values()]
    for row in mapped:
        lines.append(
            f"- `{row['gate']}`: board **{row['status']}**, measured commit "
            f"`{row['commit']}`, source `{row['source']}`"
        )
    board_only = [
        row for row in rows if row["source"] == BOARD_REL and row["verification"] != "CURRENT"
    ]
    lines.append(
        f"- {len(board_only)} board-only gates retain their board status; no current "
        "measurement artifact is recorded for them."
    )
    if blockers:
        lines.append(
            "- Board blockers retained: "
            + ", ".join(f"`{row['gate']}` ({row['status']})" for row in blockers)
            + "."
        )
    lines.extend(
        [
            "",
            "## Packet index",
            "",
            "See `INDEX.tsv` (path, SHA-256, size, commit) and `SHA256SUMS`.",
            "",
            "## Reproduce",
            "",
            "```sh",
            "python3 tests/release/evidence/packet_refresh.py --write \\",
            "  --check-fast-log /tmp/opencode/k554-check-fast.log \\",
            "  --served-copies-log /tmp/opencode/k554-served-check2.log \\",
            "  --evidence-index-log /tmp/opencode/k554-evidence-check2.log",
            "python3 tests/release/evidence/packet_refresh.py --check",
            "```",
            "",
            f"Packet verdict: **NO-GO**. {len(unverified)} row(s) are UNVERIFIED.",
            "",
        ]
    )
    return "\n".join(lines)


def build_index(
    root: Path,
    mgmt_root: Path,
    packet_dir: Path,
    rows: list[dict[str, str]],
    head: str,
) -> str:
    def source_commit(rel: str) -> str:
        if rel in {BOARD_REL, SERVED_REL}:
            return head
        if rel == TCP_MANIFEST_REL:
            for row in rows:
                if row["source"] == TCP_REGISTER_REL:
                    return row["commit"]
        for row in rows:
            if row["source"] == rel:
                return row["commit"]
        return head

    entries: list[tuple[str, str, Path, str]] = []
    for name in PACKET_FILES:
        path = packet_dir / name
        if path.is_file():
            entries.append(("packet", f"tickets/evidence/release/{name}", path, head))
    for rel in (BOARD_REL, KERNEL_REL, POSIX_REL, TCP_REGISTER_REL, TCP_MANIFEST_REL, SSH_REL, SERVED_REL):
        path = mgmt_root / rel
        if path.is_file():
            entries.append(("source", rel, path, source_commit(rel)))
    for rel in (SERVED_SCRIPT_REL, INDEX_SCRIPT_REL, GENERATOR_REL):
        path = root / rel
        if path.is_file():
            entries.append(("generator", f"product/{rel}", path, head))
    lines = [
        "# SPDX-License-Identifier: BSD-3-Clause",
        "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        "# GENERATED by tests/release/evidence/packet_refresh.py --write; do not edit.",
        "role\tpath\tsha256\tbytes\tcommit",
    ]
    for role, path_text, path, commit in entries:
        lines.append(f"{role}\t{path_text}\t{sha256(path)}\t{size(path)}\t{commit}")
    return "\n".join(lines) + "\n"


def write_packet(
    root: Path,
    mgmt_root: Path,
    packet_dir: Path,
    check_fast_log: Path | None,
    served_log: Path | None,
    evidence_log: Path | None,
) -> int:
    packet_dir.mkdir(parents=True, exist_ok=True)
    for source, name in (
        (check_fast_log, "check-fast.log"),
        (served_log, "served-copies-check.log"),
        (evidence_log, "evidence-index-check.log"),
    ):
        if source is not None:
            if not source.is_file():
                print(f"packet-refresh: source log missing: {source}", file=sys.stderr)
                return 1
            shutil.copyfile(source, packet_dir / name)
    rows = build_rows(root, mgmt_root, packet_dir)
    head = git_head(root)
    dirty = bool(
        subprocess.run(
            ["git", "-C", str(root), "status", "--porcelain"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
        ).stdout.strip()
    )
    (packet_dir / "gates.tsv").write_text(render_gates(rows), encoding="utf-8")
    (packet_dir / "README.md").write_text(render_readme(rows, head, dirty), encoding="utf-8")
    (packet_dir / "INDEX.tsv").write_text(
        build_index(root, mgmt_root, packet_dir, rows, head), encoding="utf-8"
    )
    sums = [
        f"{sha256(packet_dir / name)}  {name}"
        for name in PACKET_FILES
        if (packet_dir / name).is_file()
    ]
    (packet_dir / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")
    print(f"packet-refresh: wrote {packet_dir} ({len(rows)} gate rows)")
    return 0


def verify_packet(root: Path, mgmt_root: Path, packet_dir: Path) -> int:
    rows = build_rows(root, mgmt_root, packet_dir)
    head = git_head(root)
    dirty = bool(
        subprocess.run(
            ["git", "-C", str(root), "status", "--porcelain"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
        ).stdout.strip()
    )
    expected = {
        "gates.tsv": render_gates(rows),
        "README.md": render_readme(rows, head, dirty),
    }
    for name, text in expected.items():
        path = packet_dir / name
        if not path.is_file():
            print(f"packet-refresh: MISSING {path}; run --write", file=sys.stderr)
            return 1
        if read(path) != text:
            print(f"packet-refresh: STALE {path}; run --write", file=sys.stderr)
            return 1
    index = packet_dir / "INDEX.tsv"
    if not index.is_file():
        print(f"packet-refresh: MISSING {index}; run --write", file=sys.stderr)
        return 1
    for line in read(index).splitlines():
        if not line or line.startswith("#") or line.startswith("role\t"):
            continue
        role, path_text, digest, bytes_text, commit = line.split("\t")
        if path_text.startswith("product/"):
            path = root / path_text[len("product/") :]
        elif path_text.startswith("tickets/evidence/release/"):
            path = packet_dir / path_text[len("tickets/evidence/release/") :]
        else:
            path = mgmt_root / path_text
        if not path.is_file():
            print(f"packet-refresh: INDEX missing {path_text}", file=sys.stderr)
            return 1
        if sha256(path) != digest or size(path) != int(bytes_text):
            print(f"packet-refresh: INDEX stale {path_text}", file=sys.stderr)
            return 1
    sums = packet_dir / "SHA256SUMS"
    if not sums.is_file():
        print(f"packet-refresh: MISSING {sums}; run --write", file=sys.stderr)
        return 1
    for line in read(sums).splitlines():
        if not line:
            continue
        digest, name = line.split("  ", 1)
        path = packet_dir / name
        if not path.is_file() or sha256(path) != digest:
            print(f"packet-refresh: SHA256SUMS stale {name}", file=sys.stderr)
            return 1
    print(f"packet-refresh: PASS ({len(expected)} generated files, INDEX.tsv, SHA256SUMS)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Refresh the release evidence packet")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    parser.add_argument("--root", type=Path, help="product checkout")
    parser.add_argument("--mgmt", type=Path, help="management checkout")
    parser.add_argument("--packet-dir", type=Path, help="packet directory")
    parser.add_argument("--check-fast-log", type=Path)
    parser.add_argument("--served-copies-log", type=Path)
    parser.add_argument("--evidence-index-log", type=Path)
    args = parser.parse_args(argv)

    root = args.root.resolve() if args.root else Path(__file__).resolve().parents[3]
    mgmt_root = args.mgmt.resolve() if args.mgmt else mgmt.resolve_root(root)
    packet_dir = args.packet_dir.resolve() if args.packet_dir else mgmt_root / PACKET_REL
    if args.write:
        return write_packet(
            root,
            mgmt_root,
            packet_dir,
            args.check_fast_log,
            args.served_copies_log,
            args.evidence_index_log,
        )
    return verify_packet(root, mgmt_root, packet_dir)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
