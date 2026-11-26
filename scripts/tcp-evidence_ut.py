#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K264 provenance-checker tests: negative fixtures and mutation checks.

The checker must fail when a required FreeBSD/DragonFly entry is missing and when a NetBSD/NRL
TCP import is injected. The mutation harness removes each rule from a temporary copy of
`scripts/tcp-evidence.py`; the same fixtures must then pass, proving the test catches the
regression. The real checker must reject both fixtures and accept a valid FreeBSD import.

Run: `timeout 30 python3 scripts/tcp-evidence_ut.py`
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts" / "tcp-evidence.py"
FREEBSD_REV = "133344405f532bd54ff3629a77d4c4e062f01fdb"
NETBSD_REV = "36eddebcc36cb0775aa2962005b9c05419a6ff01"


def load_checker(path: Path):
    spec = importlib.util.spec_from_file_location(f"tcp_evidence_{path.stem}", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def write_fixture(root: Path, kind: str) -> Path:
    crate = root / "crates" / "raven-tcpip"
    (crate / "src").mkdir(parents=True)
    (crate / "src" / "x.rs").write_text(
        "// SPDX-License-Identifier: BSD-3-Clause\n"
        "// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\n"
        "pub fn x() {}\n",
        encoding="utf-8",
    )
    (crate / "Cargo.toml").write_text(
        '[package]\nname = "raven-tcpip"\nversion = "0.1.0"\nlicense.workspace = true\n',
        encoding="utf-8",
    )
    (root / "Cargo.toml").write_text(
        '[workspace]\n[workspace.package]\nlicense = "BSD-3-Clause"\n',
        encoding="utf-8",
    )
    claims_dir = root / "tests" / "evidence" / "tcp"
    claims_dir.mkdir(parents=True)
    claims_path = claims_dir / "port-claims.json"
    if kind == "missing-entry":
        claims = {
            "claims": [
                {
                    "ticket": "K999",
                    "lane": "test",
                    "kind": "port",
                    "status": "landed",
                    "copy": "permitted",
                    "paths": ["crates/raven-tcpip/src/x.rs"],
                }
            ]
        }
        (root / "PROVENANCE.toml").write_text("# no imports\n", encoding="utf-8")
    elif kind == "netbsd-import":
        claims = {"claims": []}
        (root / "PROVENANCE.toml").write_text(
            "[[import]]\n"
            'path = "crates/raven-tcpip/src/x.rs"\n'
            'upstream = "netbsd"\n'
            'upstream_path = "sys/netinet/tcp_output.c"\n'
            f'upstream_rev = "{NETBSD_REV}"\n'
            'license = "BSD-3-Clause"\n'
            'tier = "A"\n'
            'copied = "2026-11-21"\n'
            "modified = true\n",
            encoding="utf-8",
        )
    elif kind == "good-freebsd":
        claims = {
            "claims": [
                {
                    "ticket": "K999",
                    "lane": "test",
                    "kind": "port",
                    "status": "landed",
                    "copy": "permitted",
                    "paths": ["crates/raven-tcpip/src/x.rs"],
                }
            ]
        }
        (root / "PROVENANCE.toml").write_text(
            "[[import]]\n"
            'path = "crates/raven-tcpip/src/x.rs"\n'
            'upstream = "freebsd"\n'
            'upstream_path = "sys/netinet/tcp_output.c"\n'
            f'upstream_rev = "{FREEBSD_REV}"\n'
            'license = "BSD-3-Clause"\n'
            'tier = "A"\n'
            'copied = "2026-11-21"\n'
            "modified = true\n",
            encoding="utf-8",
        )
    else:
        raise AssertionError(f"unknown fixture {kind}")
    claims_path.write_text(json.dumps(claims, indent=2) + "\n", encoding="utf-8")
    return claims_path


def run_audit(module, root: Path, claims: Path) -> dict:
    return module.audit(root, claims, None, None, None)


def main() -> int:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="k264-checker-") as temp:
        base = Path(temp)
        cases = {}
        for kind in ("missing-entry", "netbsd-import", "good-freebsd"):
            root = base / kind
            root.mkdir()
            claims = write_fixture(root, kind)
            cases[kind] = (root, claims)

        real = load_checker(CHECKER)
        missing = run_audit(real, *cases["missing-entry"])
        if missing["result"] != "fail" or not any(
            "landed but missing FreeBSD/DragonFly imports" in item for item in missing["violations"]
        ):
            failures.append("missing-entry fixture was not rejected with the missing-import message")
        else:
            print("tcp-evidence-tests: negative missing-entry -> FAIL (expected)")

        netbsd = run_audit(real, *cases["netbsd-import"])
        if netbsd["result"] != "fail" or not any(
            "NRL 4-clause NO-COPY" in item for item in netbsd["violations"]
        ):
            failures.append("NetBSD fixture was not rejected with the NRL message")
        else:
            print("tcp-evidence-tests: negative netbsd-import -> FAIL (expected)")

        good = run_audit(real, *cases["good-freebsd"])
        if good["result"] != "pass":
            failures.append(f"valid FreeBSD fixture was rejected: {good['violations']}")
        else:
            print("tcp-evidence-tests: valid freebsd-import -> PASS (expected)")

        source = CHECKER.read_text(encoding="utf-8")
        mutations = (
            (
                "netbsd-rule",
                'if upstream == "netbsd":',
                "if False:",
                "netbsd-import",
            ),
            (
                "missing-entry-rule",
                '            if missing:\n                violations.append(f"{ticket}: landed but missing FreeBSD/DragonFly imports: " + ", ".join(missing))',
                '            if False:\n                violations.append(f"{ticket}: landed but missing FreeBSD/DragonFly imports: " + ", ".join(missing))',
                "missing-entry",
            ),
        )
        for name, old, new, fixture in mutations:
            if old not in source:
                failures.append(f"mutation {name}: target rule not found")
                continue
            mutant_path = base / f"mutant-{name}.py"
            mutant_path.write_text(source.replace(old, new, 1), encoding="utf-8")
            mutant = load_checker(mutant_path)
            result = run_audit(mutant, *cases[fixture])
            if result["result"] == "pass":
                print(f"tcp-evidence-tests: mutation {name} -> caught (fixture passes under the mutant)")
            else:
                failures.append(f"mutation {name}: the fixture is still rejected; the test would not catch it")

    if failures:
        for failure in failures:
            print(f"tcp-evidence-tests: FAIL: {failure}", file=sys.stderr)
        return 1
    print("tcp-evidence-tests: PASS (2 negative fixtures, 1 positive fixture, 2 mutations caught)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
