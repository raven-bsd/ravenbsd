#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Donor port test register (K450).

Enumerates every donor-derived source file under ``kernel/``, ``libc/``, ``crates/`` and
``userland/`` and maps it to the test files that name it.  A mapping is COVERED only when a
test file or test fixture names the donor file's path, basename or module stem; a crate-level
test is INDIRECT; otherwise the row is NONE and carries an owner lane and a proposed ticket.

Usage::

    python3 tests/release/port-register/run.py --write   # regenerate register.tsv
    python3 tests/release/port-register/run.py --check   # fail if a donor file has no row

The check is a reducer gate: adding a donor-marked file without regenerating the register
makes it exit non-zero.  No product source or test is changed by this script.  The rendered
summary is lead-maintained in ``ravenbsd-prjmgmt`` because the product documentation tree is
frozen (AGENTS.md rule 31); this tool writes only the machine-readable TSV.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple

ROOT = Path(__file__).resolve().parents[3]
REGISTER = Path("tests/release/port-register/register.tsv")

AREAS = ("kernel", "libc", "crates", "userland")
SOURCE_EXTS = {".rs", ".c", ".h", ".s", ".ld", ".sh", ".toml", ".py", ".inc"}
TEST_EXTS = {".rs", ".sh", ".toml", ".c", ".h"}
SKIP_DIRS = {"addons", "donors", "target", ".git", "vendor"}

# These are whole vendor/worktree imports in PROVENANCE.toml, not transpiled Raven files.
# `addons/` and `donors/` are excluded by the ticket; brush/uutils/toolchain are unbuilt
# vendor trees (brush and uutils are excluded from the Cargo workspace).
EXCLUDED_PREFIXES = (
    "addons/",
    "userland/brush/",
    "userland/uutils/",
    "toolchain/",
)

DONOR_MARKER = re.compile(
    r"(RavenBSD:\s*(?:transpiled|adapted|ported|derived)\s+from"
    r"|The Regents"
    r"|(?:Copyright|\(c\))[^\n]*(?:NetBSD|FreeBSD|OpenBSD|DragonFly|Illumos|Sun Microsystems)"
    r"|SPDX-License-Identifier:\s*(?:BSD-2-Clause|BSD-4-Clause|CDDL))",
    re.IGNORECASE,
)

OWNER = {
    "kernel": "stuart",
    "libc": "kevin",
    "crates": "bob",
    "userland": "kevin",
}


def owner_for(path: str) -> str:
    if path.startswith("crates/ufs-core/"):
        return "mel"
    if path.startswith("crates/raven-tcpip/"):
        return "carl"
    return OWNER[path.split("/", 1)[0]]
PROPOSED = {
    "libc": "K451",
    "kernel": "K452",
    "crates": "K453",
    "userland": "K454",
}
CLOSED_BY = {
    "libc/ravenlibc/abi/misc.toml": "K454",
    "libc/ravenlibc/abi/poll.toml": "K454",
    "libc/ravenlibc/abi/stat.toml": "K454",
    "libc/ravenlibc/abi/time.toml": "K454",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/misc.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/mman.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/poll.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/stat.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/sysconf.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-abi/src/generated/time.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-compat/src/pty/shared.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_x86_64.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_x86_64.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-stdio/src/backend_fd.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-stdio/src/closeall.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/bsd_compat.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/explicit_bzero.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/mem/mod.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/memccpy.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/memmem.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/mempcpy.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/memset_s.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/strdup.rs": "K459",
    "libc/ravenlibc/crates/raven-libc-str/src/raw/swab.rs": "K459",
    "crates/raven-tcpip/src/pmtu.rs": "K465",
    "crates/raven-tcpip/src/sack.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-compat/src/humanize/expand.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-compat/src/humanize/scaled.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/big5.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/gb18030.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/gb18030utf.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/hkscs.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/jis0208.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/ksc.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-iconv-cjk/src/revjis.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cos.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/powf.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp2.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tan.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/ceil.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fabs.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fdim.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fmax.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fmin.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fmod.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/rint.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/sqrt.rs": "K465",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acos.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acosf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acoshf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asin.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asinf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asinhf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atan2f.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atanf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atanhf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cosf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cosh.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/coshf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/erf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/erff.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp10.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp2f.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/expf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/expm1f.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/generic/fma.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log10f.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log1pf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log2.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log2f.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/logf.rs": "K479",
    "libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tgammaf.rs": "K479",
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def excluded(path: str) -> bool:
    return any(path == prefix[:-1] or path.startswith(prefix) for prefix in EXCLUDED_PREFIXES)


def walk_area(area: str) -> Iterable[Tuple[Path, str]]:
    base = ROOT / area
    for root, dirs, files in os.walk(base):
        root_rel = rel(Path(root))
        pruned = []
        for name in sorted(dirs):
            if name in SKIP_DIRS:
                continue
            child = f"{root_rel}/{name}" if root_rel else name
            if excluded(child + "/"):
                continue
            pruned.append(name)
        dirs[:] = pruned
        for name in sorted(files):
            path = Path(root) / name
            path_rel = rel(path)
            if excluded(path_rel):
                continue
            yield path, path_rel


def read_head(path: Path, lines: int = 20) -> str:
    try:
        with path.open(encoding="utf-8", errors="replace") as handle:
            return "".join(next(handle, "") for _ in range(lines))
    except OSError:
        return ""


def provenance_entries() -> Dict[str, Dict[str, str]]:
    path = ROOT / "PROVENANCE.toml"
    if not path.is_file():
        return {}
    text = path.read_text(encoding="utf-8", errors="replace")
    entries: Dict[str, Dict[str, str]] = {}
    for block in re.split(r"(?m)^\[\[import\]\]\s*$", text)[1:]:
        match = re.search(r'(?m)^path\s*=\s*"([^"]+)"', block)
        if not match:
            continue
        entry_path = match.group(1)
        if not entry_path.startswith(AREAS) or excluded(entry_path):
            continue
        if not (ROOT / entry_path).is_file():
            continue
        upstream = re.search(r'(?m)^upstream\s*=\s*"([^"]+)"', block)
        license_name = re.search(r'(?m)^license\s*=\s*"([^"]+)"', block)
        entries[entry_path] = {
            "upstream": upstream.group(1) if upstream else "-",
            "license": license_name.group(1) if license_name else "-",
        }
    return entries


def donor_files() -> Dict[str, Dict[str, str]]:
    provenance = provenance_entries()
    donors: Dict[str, Dict[str, str]] = {}
    for entry_path, meta in provenance.items():
        donors[entry_path] = {"source": "provenance", **meta}
    for area in AREAS:
        for path, path_rel in walk_area(area):
            if path.suffix.lower() not in SOURCE_EXTS:
                continue
            if not DONOR_MARKER.search(read_head(path)):
                continue
            if path_rel in donors:
                donors[path_rel]["source"] = "provenance+header"
            else:
                donors[path_rel] = {"source": "header", "upstream": "-", "license": "-"}
    return donors


class TestFile:
    __slots__ = ("path", "text", "crate_dir", "is_unit")

    def __init__(self, path: str, text: str, crate_dir: Optional[str], is_unit: bool):
        self.path = path
        self.text = text
        self.crate_dir = crate_dir
        self.is_unit = is_unit


CRATE_CACHE: Dict[str, Optional[str]] = {}


def crate_dir_for(path: str) -> Optional[str]:
    if path in CRATE_CACHE:
        return CRATE_CACHE[path]
    original = path
    current = (ROOT / path).parent
    while current != ROOT:
        manifest = current / "Cargo.toml"
        if manifest.is_file():
            text = manifest.read_text(encoding="utf-8", errors="replace")
            result = rel(current) if re.search(r'(?m)^name\s*=\s*"', text) else None
            CRATE_CACHE[original] = result
            return result
        current = current.parent
    CRATE_CACHE[original] = None
    return None


def test_paths() -> List[str]:
    found = set()
    for area in AREAS:
        for path, path_rel in walk_area(area):
            if path.suffix.lower() not in TEST_EXTS:
                continue
            if path.name.endswith("_ut.rs") or "/tests/" in path_rel:
                found.add(path_rel)
    tests_root = ROOT / "tests"
    for root, dirs, files in os.walk(tests_root):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for name in sorted(files):
            path = Path(root) / name
            if path.suffix.lower() in TEST_EXTS:
                found.add(rel(path))
    return sorted(found)


NAME_TOKEN = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.-]{3,}")
WORD_TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]{4,}")


class TestIndex:
    """Fast lookup: which tests mention a donor basename or module stem."""

    __slots__ = ("tests", "paths", "name_index", "stem_index", "crate_tests")

    def __init__(self, tests: List[TestFile]):
        self.tests = tests
        self.paths = {test.path for test in tests}
        self.name_index: Dict[str, set] = {}
        self.stem_index: Dict[str, set] = {}
        self.crate_tests: Dict[str, List[TestFile]] = {}
        for index, test in enumerate(tests):
            for token in set(NAME_TOKEN.findall(test.text)):
                self.name_index.setdefault(token, set()).add(index)
            for token in set(WORD_TOKEN.findall(test.text)):
                self.stem_index.setdefault(token, set()).add(index)
            if test.crate_dir:
                self.crate_tests.setdefault(test.crate_dir, []).append(test)
        for bucket in self.crate_tests.values():
            bucket.sort(key=lambda test: (not test.is_unit, test.path))

    def candidates(self, name: str, stem: str) -> Iterable[TestFile]:
        indexes = set()
        if len(name) >= 4:
            indexes.update(self.name_index.get(name, ()))
        if len(stem) >= 5:
            indexes.update(self.stem_index.get(stem, ()))
        return (self.tests[index] for index in sorted(indexes))


def load_tests() -> TestIndex:
    tests: List[TestFile] = []
    for test_path in test_paths():
        try:
            text = (ROOT / test_path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        tests.append(
            TestFile(
                test_path,
                text,
                crate_dir_for(test_path),
                test_path.endswith("_ut.rs"),
            )
        )
    return TestIndex(tests)


def basename_and_stem(path: str) -> Tuple[str, str]:
    name = Path(path).name
    return name, Path(name).stem


def path_suffix(path: str) -> str:
    for prefix in ("libc/ravenlibc/", "libc/", "kernel/", "crates/", "userland/"):
        if path.startswith(prefix):
            return path[len(prefix):]
    return path


def best_named_test(
    path: str, index: TestIndex
) -> Optional[Tuple[TestFile, str]]:
    """Return the best test that names this file, with the naming basis."""
    name, stem = basename_and_stem(path)
    suffix = path_suffix(path)
    crate = crate_dir_for(path)
    # A Rust source file must be named by a test in its own crate, or by a test that carries
    # its full path/suffix. A bare basename/stem match in an unrelated crate is not evidence.
    cross_name_ok = path.endswith((".h", ".hpp", ".inc", ".toml")) or "/include/" in path
    best: Optional[Tuple[int, TestFile, str]] = None
    for test in index.candidates(name, stem):
        score = 0
        basis = ""
        if test.text.find(path) >= 0:
            score, basis = 40, "path"
        elif len(suffix) >= 6 and test.text.find(suffix) >= 0:
            score, basis = 30, "suffix"
        elif test.text.find(name) >= 0:
            score, basis = 20, "basename"
        elif stem in test.text:
            score, basis = 10, "stem"
        if not basis:
            continue
        same_crate = bool(crate and test.crate_dir == crate)
        if not same_crate and basis in ("basename", "stem") and not cross_name_ok:
            continue
        if same_crate:
            score += 8
        if test.is_unit:
            score += 5
        if test.path.startswith("tests/"):
            score += 2
        candidate = (score, test, basis)
        if best is None or candidate[0] > best[0] or (
            candidate[0] == best[0] and test.path < best[1].path
        ):
            best = candidate
    if best is None:
        return None
    return best[1], best[2]


def indirect_test(path: str, index: TestIndex) -> Optional[TestFile]:
    crate = crate_dir_for(path)
    if not crate:
        return None
    bucket = index.crate_tests.get(crate)
    return bucket[0] if bucket else None


def classify(path: str, index: TestIndex) -> Tuple[str, str, str]:
    name, stem = basename_and_stem(path)
    sibling = str(Path(path).parent / (stem + "_ut.rs"))
    if path.endswith("_ut.rs") or "/tests/" in path:
        return "COVERED", path, "self"
    if sibling in index.paths:
        return "COVERED", sibling, "sibling"
    named = best_named_test(path, index)
    if named:
        return "COVERED", named[0].path, named[1]
    indirect = indirect_test(path, index)
    if indirect:
        return "INDIRECT", indirect.path, "crate-tests"
    return "NONE", "-", "none"


def build_rows() -> List[Dict[str, str]]:
    donors = donor_files()
    index = load_tests()
    rows = []
    for path in sorted(donors):
        area = path.split("/", 1)[0]
        status, evidence, basis = classify(path, index)
        meta = donors[path]
        owner = owner_for(path)
        proposed = CLOSED_BY.get(path) or (PROPOSED[area] if status == "NONE" else "-")
        rows.append(
            {
                "file": path,
                "area": area,
                "status": status,
                "basis": basis,
                "evidence": evidence,
                "owner": owner,
                "proposed_ticket": proposed,
                "source": meta["source"],
                "upstream": meta["upstream"],
                "license": meta["license"],
            }
        )
    return rows


COLUMNS = (
    "file",
    "area",
    "status",
    "basis",
    "evidence",
    "owner",
    "proposed_ticket",
    "source",
    "upstream",
    "license",
)


def render_register(rows: List[Dict[str, str]]) -> str:
    lines = ["\t".join(COLUMNS)]
    for row in rows:
        lines.append("\t".join(row[column] for column in COLUMNS))
    return "\n".join(lines) + "\n"


def counts(rows: List[Dict[str, str]]) -> List[Tuple[str, int, int, int, int]]:
    table = []
    for area in AREAS:
        selected = [row for row in rows if row["area"] == area]
        covered = sum(row["status"] == "COVERED" for row in selected)
        indirect = sum(row["status"] == "INDIRECT" for row in selected)
        none = sum(row["status"] == "NONE" for row in selected)
        table.append((area, len(selected), covered, indirect, none))
    return table


def read_register(path: Path) -> Optional[List[str]]:
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8").splitlines()


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true", help="regenerate register.tsv")
    group.add_argument("--check", action="store_true", help="fail when the register is stale")
    args = parser.parse_args(argv)

    rows = build_rows()
    register_text = render_register(rows)
    register_path = ROOT / REGISTER

    if args.write:
        register_path.parent.mkdir(parents=True, exist_ok=True)
        register_path.write_text(register_text, encoding="utf-8")
        for area, files, covered, indirect, none in counts(rows):
            print(f"port-register: {area}: {files} files, {covered} covered, {indirect} indirect, {none} none")
        return 0

    problems = []
    current = read_register(register_path)
    if current is None:
        problems.append(f"missing {REGISTER}; run --write")
    else:
        expected = register_text.splitlines()
        if current != expected:
            current_files = {line.split("\t", 1)[0] for line in current[1:] if line}
            expected_files = {row["file"] for row in rows}
            missing = sorted(expected_files - current_files)
            stale = sorted(current_files - expected_files)
            if missing:
                problems.append(f"{len(missing)} donor file(s) have no register row: {', '.join(missing[:5])}")
            if stale:
                problems.append(f"{len(stale)} register row(s) are stale: {', '.join(stale[:5])}")
            if not missing and not stale:
                problems.append("register rows are stale (status/evidence changed); run --write")

    if problems:
        for problem in problems:
            print(f"port-register: FAIL: {problem}", file=sys.stderr)
        return 1
    for area, files, covered, indirect, none in counts(rows):
        print(f"port-register: {area}: {files} files, {covered} covered, {indirect} indirect, {none} none")
    print(f"port-register: PASS ({len(rows)} donor files mapped)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
