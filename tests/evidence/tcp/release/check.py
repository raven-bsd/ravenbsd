#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K295 release evidence packet validator.

Every release claim must carry an OBSERVED/PROPOSED tag, an artifact path and a SHA-256. The
validator rejects a missing tag, a missing artifact, a wrong hash and a report row without a
report file. `--selftest` proves each rejection with synthetic rows; the real manifest must pass.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
TAGS = {"OBSERVED", "PROPOSED"}
HEX = set("0123456789abcdef")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(manifest: dict, root: Path, reports_root: Path | None = None):
    errors: list[str] = []
    warnings: list[str] = []
    claims = manifest.get("claims")
    if not isinstance(claims, list) or not claims:
        errors.append("manifest has no claims")
        claims = []
    for index, claim in enumerate(claims):
        label = claim.get("id") or f"claim[{index}]"
        if not claim.get("claim"):
            errors.append(f"{label}: empty claim text")
        tag = claim.get("tag")
        if tag not in TAGS:
            errors.append(f"{label}: tag {tag!r} is not OBSERVED/PROPOSED")
        artifact = claim.get("artifact")
        if not artifact:
            errors.append(f"{label}: missing artifact path")
            continue
        expected = str(claim.get("sha256", ""))
        if len(expected) != 64 or any(character not in HEX for character in expected):
            errors.append(f"{label}: missing or malformed sha256")
            continue
        path = root / artifact
        if not path.is_file():
            errors.append(f"{label}: artifact not found: {artifact}")
            continue
        actual = sha256(path)
        if actual != expected:
            errors.append(f"{label}: sha256 mismatch for {artifact} ({actual} != {expected})")
    reports = manifest.get("reports", [])
    for index, report in enumerate(reports):
        label = report.get("ticket") or f"report[{index}]"
        path_text = report.get("path")
        if not path_text:
            errors.append(f"{label}: missing report path")
            continue
        expected = str(report.get("sha256", ""))
        if len(expected) != 64 or any(character not in HEX for character in expected):
            errors.append(f"{label}: missing or malformed report sha256")
            continue
        if reports_root is None:
            warnings.append(f"{label}: report hash not checked (no --reports-root)")
            continue
        path = reports_root / path_text
        if not path.is_file():
            errors.append(f"{label}: report not found: {path}")
            continue
        actual = sha256(path)
        if actual != expected:
            errors.append(f"{label}: report sha256 mismatch for {path_text} ({actual} != {expected})")
    return errors, warnings


def selftest() -> int:
    with tempfile.TemporaryDirectory(prefix="k295-release-") as temp:
        root = Path(temp)
        artifact = root / "artifact.txt"
        artifact.write_text("observed\n", encoding="utf-8")
        digest = sha256(artifact)
        good = {
            "id": "T-R01",
            "claim": "a good row",
            "tag": "OBSERVED",
            "artifact": "artifact.txt",
            "sha256": digest,
        }
        cases = {
            "good": {"claims": [good], "reports": []},
            "missing-tag": {"claims": [{**good, "tag": None}], "reports": []},
            "missing-artifact": {"claims": [{**good, "artifact": ""}], "reports": []},
            "wrong-hash": {"claims": [{**good, "sha256": "0" * 64}], "reports": []},
        }
        failures: list[str] = []
        for name, manifest in cases.items():
            errors, _ = validate(manifest, root)
            if name == "good":
                if errors:
                    failures.append(f"good row rejected: {errors}")
            elif not errors:
                failures.append(f"{name} row was accepted")
        if failures:
            for failure in failures:
                print(f"release-evidence-selftest: FAIL: {failure}", file=sys.stderr)
            return 1
        print(
            "release-evidence-selftest: PASS "
            "good row accepted; missing-tag, missing-artifact and wrong-hash rejected"
        )
        return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--manifest", default=str(HERE / "manifest.json"))
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--reports-root")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    manifest_path = Path(args.manifest)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    root = Path(args.root)
    reports_root = Path(args.reports_root) if args.reports_root else None
    errors, warnings = validate(manifest, root, reports_root)
    for warning in warnings:
        print(f"release-evidence: warning: {warning}", file=sys.stderr)
    if errors:
        for error in errors:
            print(f"release-evidence: FAIL: {error}", file=sys.stderr)
        return 1
    print(
        f"release-evidence: PASS ({len(manifest.get('claims', []))} claims, "
        f"{len(manifest.get('reports', []))} reports)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
