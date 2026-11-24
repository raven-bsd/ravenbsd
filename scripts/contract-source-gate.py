#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Resolve every ticket Contract-source citation against ravenbsd-resources.

The ticket header field ``Contract-source:`` names contract sections and the
donor files/functions behind them. This gate reads each ticket under
``tickets/<lane>/{todo,wip,done}/*.md`` and resolves every backticked path and
symbol in that field:

* a path that exists in the product repository is a Raven path;
* otherwise it must exist at the locked revision of a donor checkout named by
  the citation (or of a locked donor when the citation names none);
* a parenthetical group that is only a list of backticked symbols must have
  every symbol in the cited file;
* every donor named by a citation must be in ``donors.lock``, have a checkout
  in ``ravenbsd-resources`` and have that checkout at the locked revision.

The gate fails closed: a missing tickets tree, resources tree, ``donors.lock``,
donor checkout or locked revision exits 2. An unresolved path, symbol, donor or
revision, or a current (todo/wip) ticket without a ``Contract-source`` header,
exits 1. A clean citation set exits 0; legacy done tickets written before the
field existed are counted as skipped, not read as citations.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

DONOR_ALIASES = (
    (re.compile(r"\bFreeBSD\b"), "freebsd", "freebsd"),
    (re.compile(r"\bNetBSD\b"), "netbsd", "netbsd"),
    (re.compile(r"\bOpenBSD\b"), "openbsd", "openbsd"),
    (re.compile(r"\bDragonFly(?:BSD)?\b"), "dragonfly", "dragonfly"),
    (re.compile(r"\bIllumos\b"), "illumos-gate", "illumos-gate"),
    (re.compile(r"\bSolaris\b"), "illumos-gate", "illumos-gate"),
    (re.compile(r"\bOpenSSH\b"), "openssh-portable", "openssh"),
    (re.compile(r"\bOpenZFS\b"), "openzfs", "openzfs"),
    (re.compile(r"(?<![\w-])glibc(?![\w-])"), "glibc", "glibc"),
    (re.compile(r"\bmusl\b"), "musl", "musl"),
    (re.compile(r"\bLinux\b"), "linux", "linux"),
)

LOCK_ALIASES = {
    "openssh-portable": ("openssh", "openssh-portable"),
}

PATH_SUFFIXES = (
    ".c",
    ".h",
    ".cc",
    ".cpp",
    ".hpp",
    ".s",
    ".S",
    ".rs",
    ".go",
    ".py",
    ".pl",
    ".sh",
    ".md",
    ".txt",
    ".9",
    ".in",
    ".am",
    ".mk",
    ".toml",
)

BACKTICK = re.compile(r"`([^`]+)`")
SYMBOL = re.compile(r"`([A-Za-z_][A-Za-z0-9_]*)`")
HEADER = re.compile(r"^Contract-source:\s*(.*)$")


class GateError(Exception):
    """A prerequisite or tool failure rather than a citation mismatch."""


class Donor:
    def __init__(self, lock_name, directory, revision, resources):
        self.lock_name = lock_name
        self.directory = directory
        self.revision = revision
        self.root = resources / directory
        self.head = None
        self.error = None
        self.checked = False

    def verify(self):
        if self.checked:
            return self.error is None
        self.checked = True
        if not self.root.is_dir():
            self.error = f"donor checkout {self.directory} is absent"
            return False
        present = run_git(self.root, "cat-file", "-e", f"{self.revision}^{{commit}}")
        if present.returncode != 0:
            self.error = (
                f"donor {self.lock_name} revision {self.revision} is absent from "
                f"{self.directory}"
            )
            return False
        head = run_git(self.root, "rev-parse", "HEAD")
        if head.returncode != 0:
            self.error = f"cannot read HEAD of donor {self.directory}: {head.stderr.strip()}"
            return False
        self.head = head.stdout.strip()
        if self.head != self.revision:
            self.error = (
                f"donor {self.lock_name} HEAD {self.head} does not match locked "
                f"revision {self.revision}"
            )
            return False
        return True

    def has_path(self, path):
        result = run_git(self.root, "cat-file", "-e", f"{self.revision}:{path}")
        return result.returncode == 0

    def read_path(self, path):
        result = run_git(self.root, "show", f"{self.revision}:{path}")
        if result.returncode != 0:
            raise GateError(
                f"cannot read {self.directory}:{path} at {self.revision}: "
                f"{result.stderr.strip()}"
            )
        return result.stdout


def run_git(cwd, *args):
    try:
        return subprocess.run(
            ["git", "-C", str(cwd), *args],
            capture_output=True,
            text=True,
            timeout=30,
        )
    except FileNotFoundError as error:
        raise GateError("git is required by the contract-source gate") from error
    except subprocess.TimeoutExpired as error:
        raise GateError(f"git {' '.join(args)} timed out in {cwd}") from error


def discover(root, relative):
    for parent in (root, *root.parents):
        candidate = parent / relative
        if candidate.exists():
            return candidate
    return None


def read_lock(path):
    entries = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) < 2:
            raise GateError(f"malformed donors.lock line: {line!r}")
        entries[fields[0]] = fields[1]
    if not entries:
        raise GateError(f"donors.lock at {path} has no entries")
    return entries


def donor_directory(lock_name):
    if lock_name in LOCK_ALIASES:
        return LOCK_ALIASES[lock_name][0]
    return lock_name


def named_donors(text):
    found = []
    for pattern, lock_name, directory in DONOR_ALIASES:
        if pattern.search(text) and lock_name not in [item[0] for item in found]:
            found.append((lock_name, directory))
    return found


def is_path_token(token):
    token = token.strip()
    if not token or " " in token or "<" in token or ">" in token:
        return False
    if "/" in token:
        return True
    return token.endswith(PATH_SUFFIXES)


def safe_relative(token):
    path = Path(token)
    return not path.is_absolute() and ".." not in path.parts


def parenthetical(text, end):
    index = end
    while index < len(text) and text[index].isspace():
        index += 1
    if index >= len(text) or text[index] != "(":
        return None
    depth = 0
    start = index
    while index < len(text):
        char = text[index]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
        index += 1
    return None


def symbol_list(group):
    if group is None:
        return []
    symbols = SYMBOL.findall(group)
    rest = SYMBOL.sub("", group)
    if symbols and not rest.strip(" \t\r\n,"):
        return symbols
    return []


def ticket_files(tickets):
    files = []
    for lane in sorted(tickets.iterdir()):
        if not lane.is_dir():
            continue
        for status in ("todo", "wip", "done"):
            directory = lane / status
            if directory.is_dir():
                files.extend((path, status) for path in sorted(directory.glob("*.md")))
    return files


def citation_text(path):
    lines = path.read_text(encoding="utf-8").splitlines()
    for number, line in enumerate(lines, start=1):
        match = HEADER.match(line)
        if match:
            return number, match.group(1)
    return None, None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--tickets", type=Path)
    parser.add_argument("--resources", type=Path)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--text-out", type=Path)
    args = parser.parse_args(argv)

    root = args.root.resolve()
    tickets = args.tickets
    if tickets is None:
        tickets = discover(root, "ravenbsd-prjmgmt/tickets")
    resources = args.resources
    if resources is None:
        resources = discover(root, "ravenbsd-resources")
    if os.environ.get("RAVENBSD_TICKETS"):
        tickets = Path(os.environ["RAVENBSD_TICKETS"])
    if os.environ.get("RAVENBSD_RESOURCES"):
        resources = Path(os.environ["RAVENBSD_RESOURCES"])
    if tickets is None:
        raise GateError(
            "tickets tree not found; pass --tickets or set RAVENBSD_TICKETS"
        )
    if resources is None:
        raise GateError(
            "resources tree not found; pass --resources or set RAVENBSD_RESOURCES"
        )
    tickets = tickets.resolve()
    resources = resources.resolve()

    if not root.is_dir():
        raise GateError(f"repository root {root} is absent")
    if not tickets.is_dir():
        raise GateError(f"tickets tree {tickets} is absent")
    if not resources.is_dir():
        raise GateError(f"resources tree {resources} is absent")
    lock_path = resources / "donors.lock"
    if not lock_path.is_file():
        raise GateError(f"donors.lock is absent at {lock_path}")
    lock = read_lock(lock_path)

    files = ticket_files(tickets)
    if not files:
        raise GateError(f"no ticket files found under {tickets}/*/{{todo,wip,done}}")

    donors = {}
    mismatches = []
    citations = 0
    legacy_skipped = 0
    donor_rows = {}

    def donor(lock_name, directory):
        if lock_name not in lock:
            raise GateError(f"donor {lock_name} is not in donors.lock")
        key = (lock_name, directory)
        if key not in donors:
            donors[key] = Donor(lock_name, directory, lock[lock_name], resources)
        return donors[key]

    def record(ticket, line, kind, detail):
        mismatches.append(
            {
                "ticket": str(ticket.relative_to(tickets)),
                "line": line,
                "kind": kind,
                "detail": detail,
            }
        )

    for ticket, status in files:
        line, text = citation_text(ticket)
        if text is None:
            if status == "done":
                legacy_skipped += 1
            else:
                record(ticket, 0, "header", "missing Contract-source header")
            continue

        named = named_donors(text)
        usable = []
        for lock_name, directory in named:
            try:
                item = donor(lock_name, directory)
            except GateError as error:
                record(ticket, line, "donor", str(error))
                continue
            if not item.verify():
                record(ticket, line, "revision", item.error)
                continue
            donor_rows[lock_name] = {
                "directory": item.directory,
                "revision": item.revision,
                "head": item.head,
                "status": "verified",
            }
            usable.append(item)

        candidates = usable
        if not named:
            for lock_name in lock:
                directory = donor_directory(lock_name)
                if not (resources / directory).is_dir():
                    continue
                item = donor(lock_name, directory)
                if item.verify():
                    candidates.append(item)

        for match in BACKTICK.finditer(text):
            token = match.group(1).strip()
            if not is_path_token(token) or not safe_relative(token):
                continue
            citations += 1
            resolved = None
            content = None

            repo_path = root / token
            if repo_path.is_file():
                resolved = f"repo:{token}"
                content = repo_path.read_text(encoding="utf-8", errors="replace")
            else:
                for item in candidates:
                    if item.has_path(token):
                        resolved = f"{item.directory}@{item.revision}:{token}"
                        content = item.read_path(token)
                        break
            if resolved is None:
                record(
                    ticket,
                    line,
                    "path",
                    f"path {token} is absent from the repository and every locked donor",
                )
                continue

            for symbol in symbol_list(parenthetical(text, match.end())):
                if not re.search(rf"\b{re.escape(symbol)}\b", content):
                    record(
                        ticket,
                        line,
                        "symbol",
                        f"symbol {symbol} is absent from {resolved}",
                    )

    result = {
        "result": "pass" if not mismatches else "fail",
        "root": str(root),
        "tickets": str(tickets),
        "resources": str(resources),
        "tickets_scanned": len(files),
        "citations": citations,
        "legacy_skipped": legacy_skipped,
        "mismatches": mismatches,
        "donors": donor_rows,
    }

    text_lines = [
        f"contract-source-gate: scanned {len(files)} tickets, {citations} path citations",
    ]
    if legacy_skipped:
        text_lines.append(
            f"contract-source-gate: skipped {legacy_skipped} legacy done tickets without "
            "a Contract-source header"
        )
    for item in mismatches:
        text_lines.append(
            f"FAIL {item['ticket']}:{item['line']}: {item['kind']}: {item['detail']}"
        )
    if donor_rows:
        for name, row in sorted(donor_rows.items()):
            text_lines.append(
                f"donor {name} {row['directory']} at {row['revision']} verified"
            )
    if mismatches:
        text_lines.append(
            f"contract-source-gate: FAIL ({len(mismatches)} mismatch"
            f"{'' if len(mismatches) == 1 else 'es'})"
        )
    else:
        text_lines.append("contract-source-gate: PASS")
    text = "\n".join(text_lines) + "\n"

    payload = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.text_out:
        args.text_out.write_text(text, encoding="utf-8")
    if args.json_out:
        args.json_out.write_text(payload, encoding="utf-8")
    if args.json:
        sys.stdout.write(payload)
    else:
        sys.stdout.write(text)
    return 1 if mismatches else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except GateError as error:
        print(f"contract-source-gate: error: {error}", file=sys.stderr)
        sys.exit(2)
