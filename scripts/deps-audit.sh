#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# SC3 weekly dependency audit loop (DR-64, process/SUPPLY-CHAIN.md section 5).
#
# Usage:
#   sh scripts/deps-audit.sh            # run the audit and write metrics/deps.json
#   sh scripts/deps-audit.sh --write    # same
#   sh scripts/deps-audit.sh --check    # run the audit and compare the stable
#                                       # metrics with the committed JSON
#
# The loop never edits dependencies and never merges anything: it emits the
# inventory, the advisory state, the ranked decision queue and the recorded
# tool coverage. Unavailable scanners are recorded with a reason, never
# silently skipped and never counted as a pass.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)

mode=write
case "${1:-}" in
    ""|--write) mode=write ;;
    --check) mode=check ;;
    -h|--help)
        sed -n '6,13p' "$0"
        exit 0
        ;;
    *)
        printf 'usage: %s [--write|--check]\n' "$0" >&2
        exit 2
        ;;
esac

exec python3 - "$root" "$mode" <<'PY'
import datetime
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
mode = sys.argv[2]
metrics_path = root / "metrics/deps.json"
audit_toml = root / "audit.toml"
vendor_manifest = root / "vendor/manifest.toml"

RUSTSEC = re.compile(r"RUSTSEC-\d{4}-\d{4}")


def run(args):
    proc = subprocess.run(args, cwd=root, text=True, capture_output=True, check=False)
    return proc.returncode, proc.stdout, proc.stderr


def fail(message):
    print(f"deps-audit: {message}", file=sys.stderr)
    raise SystemExit(1)


def parse_ignores():
    try:
        text = audit_toml.read_text(encoding="utf-8")
    except OSError:
        return []
    match = re.search(r"\[advisories\](.*?)(?:\n\[|\Z)", text, re.S)
    if not match:
        return []
    return sorted(set(RUSTSEC.findall(match.group(1))))


def parse_vendor():
    if not vendor_manifest.is_file():
        return {"status": "absent"}
    values = {}
    for line in vendor_manifest.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"')
    return {
        "crates": int(values.get("vendored_crates", "0")),
        "generated_utc": values.get("generated_utc"),
        "root_lock_sha256": values.get("root_lock_sha256"),
        "sbom_sha256": values.get("sbom_sha256"),
        "artifact": values.get("artifact"),
    }


def tool_version(name):
    candidates = [[name, "--version"]]
    if name.startswith("cargo-"):
        candidates.append(["cargo", name[len("cargo-") :], "--version"])
    for args in candidates:
        code, out, _ = run(args)
        if code == 0 and out.strip():
            return out.splitlines()[0].strip()
    return None


def metadata():
    code, out, err = run(["cargo", "metadata", "--format-version", "1"])
    if code != 0:
        fail(f"cargo metadata failed: {err.strip()[:400]}")
    return json.loads(out)


def shipped_graph(meta):
    packages = {package["id"]: package for package in meta["packages"]}
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    roots = list(meta["workspace_members"])
    members = set(roots)

    def edges(node, kinds):
        for dep in node.get("deps", []):
            if any(
                (kind.get("kind") or "normal") in kinds
                for kind in dep.get("dep_kinds", [])
            ):
                yield dep["pkg"]

    def reach(kinds):
        found = set()
        queue = list(roots)
        while queue:
            current = queue.pop()
            if current in found or current not in nodes:
                continue
            found.add(current)
            queue.extend(edges(nodes[current], kinds))
        return found

    shipped = reach({"normal", "build"})
    dev = reach({"dev"})
    shipped_third_party = {pid for pid in shipped if pid not in members}

    direct = set()
    optional = set()
    for member in roots:
        for dep in packages[member].get("dependencies", []):
            kind = dep.get("kind") or "normal"
            if kind not in {"normal", "build"}:
                continue
            for pid in shipped_third_party:
                if packages[pid]["name"] == dep["name"]:
                    direct.add(pid)
                    if dep.get("optional"):
                        optional.add(pid)

    names = {}
    for pid in shipped_third_party:
        package = packages[pid]
        names.setdefault(package["name"], set()).add(package["version"])
    duplicates = [
        {"name": name, "versions": sorted(versions)}
        for name, versions in sorted(names.items())
        if len(versions) > 1
    ]
    return {
        "workspace_members": len(members),
        "locked_packages": len(meta["packages"]) - len(members),
        "shipped": len(shipped_third_party),
        "shipped_third_party": len(shipped_third_party),
        "direct": len(direct),
        "transitive": len(shipped_third_party) - len(direct),
        "optional_direct": len(optional),
        "dev_only": len(dev - shipped - members),
        "duplicates": duplicates,
    }


def audit():
    code, out, err = run(["cargo", "audit", "--json"])
    if code not in {0, 1}:
        fail(f"cargo audit failed: {err.strip()[:400]}")
    if not out.strip():
        fail(f"cargo audit produced no JSON: {err.strip()[:400]}")
    data = json.loads(out)
    vulnerabilities = data.get("vulnerabilities", {})
    return {
        "exit": code,
        "database": {
            "advisory_count": data.get("database", {}).get("advisory-count"),
            "last_updated": data.get("database", {}).get("last-updated"),
            "last_commit": data.get("database", {}).get("last-commit"),
        },
        "lockfile_dependency_count": data.get("lockfile", {}).get("dependency-count"),
        "found": bool(vulnerabilities.get("found")),
        "count": vulnerabilities.get("count", 0),
        "list": vulnerabilities.get("list", []),
        "warnings": data.get("warnings", {}),
    }


def deny_advisories():
    version = tool_version("cargo-deny")
    if version is None:
        return {
            "status": "unavailable",
            "reason": "cargo-deny is not installed on this host; deny.toml remains the policy source",
        }
    code, out, err = run(["cargo", "deny", "check", "advisories"])
    summary = (out + err).strip().splitlines()
    return {
        "status": "ok" if code == 0 else "failed",
        "version": version,
        "exit": code,
        "summary": summary[-1] if summary else "",
    }


def osv():
    if shutil.which("osv-scanner") is None:
        return {
            "status": "unavailable",
            "reason": "osv-scanner is not installed on this host; RustSec coverage via cargo-audit",
        }
    version = tool_version("osv-scanner")
    return {"status": "available", "version": version}


def udeps():
    version = tool_version("cargo-udeps")
    if version is None:
        return {
            "status": "unavailable",
            "reason": "cargo-udeps is not installed on this host",
        }
    return {
        "status": "unavailable-on-stable",
        "version": version,
        "reason": "cargo-udeps requires binary dependency info from a nightly-only compiler flag; the pinned stable toolchain rejects it, so the unmaintained warnings and the decision queue cover the dead-crate view",
    }


def unmaintained(advisories):
    found = []
    for key, value in advisories.get("warnings", {}).items():
        if not isinstance(value, list):
            continue
        for item in value:
            advisory = item.get("advisory", {})
            advisory_id = advisory.get("id")
            if advisory_id:
                found.append(
                    {
                        "id": advisory_id,
                        "kind": key,
                        "package": item.get("package", {}).get("name"),
                    }
                )
    return found


def tree_edges():
    reports = {}
    for name, kinds in (("shipped", "normal,build"), ("dev", "dev")):
        code, out, err = run(
            ["cargo", "tree", "--edges", kinds, "--prefix", "none", "--no-dedupe"]
        )
        if code != 0:
            reports[name] = {"status": "unavailable", "reason": err.strip()[:200]}
        else:
            reports[name] = {
                "status": "ok",
                "lines": len([line for line in out.splitlines() if line.strip()]),
            }
    return reports


def decision_queue(ignored, advisories):
    queue = [
        {
            "id": "RUSTSEC-2018-0002",
            "kind": "false-positive",
            "state": "resolved",
            "decision": "remove",
            "owner": "kevin",
            "review": "2026-12-05",
            "detail": "first-party tar 0.1.0 name collision; SC2 renamed it to raven-tar (3fcb1ea9), audit.toml ignore list now empty",
        },
        {
            "id": "RUSTSEC-2021-0080",
            "kind": "false-positive",
            "state": "resolved",
            "decision": "remove",
            "owner": "kevin",
            "review": "2026-12-05",
            "detail": "first-party tar 0.1.0 name collision; SC2 renamed it to raven-tar (3fcb1ea9)",
        },
        {
            "id": "RUSTSEC-2026-0067",
            "kind": "false-positive",
            "state": "resolved",
            "decision": "remove",
            "owner": "kevin",
            "review": "2026-12-05",
            "detail": "first-party tar 0.1.0 name collision; SC2 renamed it to raven-tar (3fcb1ea9)",
        },
        {
            "id": "RUSTSEC-2026-0068",
            "kind": "false-positive",
            "state": "resolved",
            "decision": "remove",
            "owner": "kevin",
            "review": "2026-12-05",
            "detail": "first-party tar 0.1.0 name collision; SC2 renamed it to raven-tar (3fcb1ea9)",
        },
        {
            "id": "RUSTSEC-2020-0071",
            "kind": "false-positive",
            "state": "resolved",
            "decision": "remove",
            "owner": "kevin",
            "review": "2026-12-05",
            "detail": "first-party time 0.1.0 name collision; SC2 renamed it to raven-time (3fcb1ea9), no time crate linked",
        },
    ]
    for advisory in ignored:
        queue.append(
            {
                "id": advisory,
                "kind": "reviewed-exception",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "kevin",
                "review": "2026-12-05",
                "detail": "listed in audit.toml; re-triage reachability and expiry",
            }
        )
    for item in unmaintained(advisories):
        queue.append(
            {
                "id": item["id"],
                "kind": "unmaintained",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "kevin",
                "review": "2026-12-05",
                "detail": f"cargo audit {item['kind']} warning for {item['package']}; keep-pinned until the owner/review date records remove, replace or fork",
            }
        )
    queue.extend(
        [
            {
                "id": "fork:toolchain/vendor/libc",
                "kind": "fork",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "kevin",
                "review": "2027-02-15",
                "detail": "K158 pthread ABI drift model; libc 0.2.186 + rust 1.97.1 std patches; tests/toolchain/pthread-abi/check.sh",
            },
            {
                "id": "fork:userland/uutils",
                "kind": "fork",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "kevin",
                "review": "2027-02-15",
                "detail": "uutils/coreutils cf030b82ef519ab60ccacf763f7f086dd612d9c3 (0.13.0); quarterly sync",
            },
            {
                "id": "fork:userland/brush",
                "kind": "fork",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "kevin",
                "review": "2027-02-15",
                "detail": "reubeno/brush 8adb6bc06815c34ccf6503902f6f292e04b1cbba (0.4.0); quarterly sync",
            },
            {
                "id": "frozen:crates/raven-rmm",
                "kind": "frozen",
                "state": "open",
                "decision": "keep-pinned",
                "owner": "mark",
                "review": "2027-02-15",
                "detail": "redox rmm 015af335c7bed8bfaa8c2ae8419ceba46fe0a227; frozen (S117/DR-63); replace when the page layer is touched",
            },
        ]
    )
    for rank, item in enumerate(queue, start=1):
        item["rank"] = rank
    return queue


def stable_view(report):
    return {
        "counts": report["counts"],
        "duplicates": report["duplicates"],
        "advisories": {
            "found": report["advisories"]["found"],
            "list": [
                item.get("advisory", {}).get("id")
                for item in report["advisories"]["list"]
            ],
            "ignored": report["advisories"]["ignored"],
        },
        "vendor": {"crates": report["vendor"].get("crates")},
        "queue": [
            {"id": item["id"], "decision": item["decision"]}
            for item in report["queue"]
        ],
    }


def live_report():
    meta = metadata()
    counts = shipped_graph(meta)
    audit_report = audit()
    ignored = parse_ignores()
    report = {
        "schema": 1,
        "generated_utc": datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        ),
        "git_head": run(["git", "rev-parse", "HEAD"])[1].strip(),
        "source": {
            "lockfile": "Cargo.lock",
            "metadata": "cargo metadata --format-version 1",
            "tree": "cargo tree --edges normal,build and --edges dev",
            "audit": "cargo audit --json",
            "process": "ravenbsd-prjmgmt/process/SUPPLY-CHAIN.md section 5",
        },
        "counts": {
            key: value for key, value in counts.items() if key != "duplicates"
        },
        "duplicates": counts["duplicates"],
        "advisories": {
            "found": audit_report["found"],
            "count": audit_report["count"],
            "list": audit_report["list"],
            "ignored": ignored,
            "database": audit_report["database"],
            "lockfile_dependency_count": audit_report["lockfile_dependency_count"],
            "warnings": audit_report["warnings"],
        },
        "tools": {
            "cargo-audit": {
                "status": "ok",
                "version": tool_version("cargo-audit"),
            },
            "cargo-deny": deny_advisories(),
            "osv-scanner": osv(),
            "cargo-udeps": udeps(),
        },
        "tree": tree_edges(),
        "vendor": parse_vendor(),
        "queue": decision_queue(ignored, audit_report),
    }
    return report


report = live_report()

if mode == "check":
    if report["advisories"]["found"]:
        fail(
            f"{report['advisories']['count']} unignored advisory(ies) found; "
            "triage into audit.toml/deny.toml with owner, reachability and expiry"
        )
    if not metrics_path.is_file():
        fail(f"{metrics_path} is missing; run scripts/deps-audit.sh --write")
    committed = json.loads(metrics_path.read_text(encoding="utf-8"))
    current = stable_view(report)
    previous = stable_view(committed)
    if current != previous:
        print("deps-audit: committed metrics/deps.json is stale", file=sys.stderr)
        for key in current:
            if current[key] != previous.get(key):
                print(f"  changed: {key}", file=sys.stderr)
                print(
                    f"    committed: {json.dumps(previous.get(key), sort_keys=True)}",
                    file=sys.stderr,
                )
                print(
                    f"    live:      {json.dumps(current[key], sort_keys=True)}",
                    file=sys.stderr,
                )
        raise SystemExit(1)
    print(
        "deps-audit: OK "
        f"({report['counts']['shipped']} shipped, {report['counts']['direct']} direct, "
        f"{len(report['duplicates'])} duplicate name(s), "
        f"{len(report['advisories']['ignored'])} reviewed exception(s), "
        f"{len(report['queue'])} queue item(s))"
    )
    raise SystemExit(0)

metrics_path.parent.mkdir(parents=True, exist_ok=True)
metrics_path.write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(
    "deps-audit: wrote metrics/deps.json "
    f"({report['counts']['shipped']} shipped, {report['counts']['direct']} direct, "
    f"{len(report['duplicates'])} duplicate name(s), "
    f"{len(report['queue'])} queue item(s), "
    f"advisories={'found' if report['advisories']['found'] else 'none'})"
)
PY
