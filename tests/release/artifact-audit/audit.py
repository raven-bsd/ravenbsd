#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""RavenBSD 0.1.0 release artifact and licence audit.

The audit resolves the requested release profile into an artifact manifest
without building an image. Every staged program and add-on must exist as a
source artifact, carry an SPDX licence, and have a provenance notice. The
manifest, hashes and failures are written below
`tickets/evidence/artifacts/S85j/` in the management tree.

The scaffold C/C++ implementation scan reports only implementation files in
the scaffold build roots; public headers and test programs are not hits.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

PROJECT_LICENSE = "BSD-3-Clause"
ALLOWED_LICENSES = frozenset(
    {
        "MIT",
        "Apache-2.0",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "0BSD",
        "Zlib",
        "Unicode-3.0",
        "CC0-1.0",
    }
)
IMPLEMENTATION_EXTENSIONS = (".c", ".cc", ".cpp", ".cxx", ".c++")
DEFAULT_PROFILES = {"x86_64": "release", "aarch64": "pi4-release"}
SCAFFOLD_ROOTS = ("kernel", "boot", "crates", "libc", "userland", "xtask")
VENDORED_PREFIXES = ("userland/brush/", "userland/uutils/")
LICENSE_FILE_NAMES = (
    "LICENSE",
    "LICENCE",
    "LICENSE.md",
    "LICENCE.md",
    "LICENSE.txt",
    "LICENCE.txt",
    "COPYING",
    "COPYRIGHT",
)
HEADER_RE = re.compile(r"^\[\[?([A-Za-z0-9_.-]+)\]\]?$")
KEY_RE = re.compile(r"^([A-Za-z0-9_-]+)\s*=\s*(.*)$")


class AuditError(ValueError):
    """A manifest, profile or provenance input cannot be read."""


# --------------------------------------------------------------------------
# Strict reader for the TOML shapes used by profiles/*.toml.
# --------------------------------------------------------------------------


def _strip_comment(line: str) -> str:
    out = []
    quoted = False
    escaped = False
    for char in line:
        if quoted:
            out.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
            out.append(char)
        elif char == "#":
            break
        else:
            out.append(char)
    return "".join(out).strip()


def _balanced(text: str) -> bool:
    depth = 0
    quoted = False
    escaped = False
    for char in text:
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            continue
        if char == '"':
            quoted = True
        elif char in "[{":
            depth += 1
        elif char in "]}":
            depth -= 1
            if depth < 0:
                return False
    return depth == 0 and not quoted


def _split_items(text: str) -> list[str]:
    items = []
    depth = 0
    quoted = False
    escaped = False
    start = 0
    for index, char in enumerate(text):
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            continue
        if char == '"':
            quoted = True
        elif char in "[{":
            depth += 1
        elif char in "]}":
            depth -= 1
        elif char == "," and depth == 0:
            items.append(text[start:index].strip())
            start = index + 1
    tail = text[start:].strip()
    if tail:
        items.append(tail)
    return [item for item in items if item]


def _parse_string(text: str) -> str:
    if len(text) < 2 or text[0] != '"' or text[-1] != '"':
        raise AuditError(f"not a quoted string: {text!r}")
    body = text[1:-1]
    out = []
    index = 0
    while index < len(body):
        char = body[index]
        if char != "\\":
            out.append(char)
            index += 1
            continue
        index += 1
        if index >= len(body):
            raise AuditError(f"unterminated escape: {text!r}")
        escape = body[index]
        out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(escape, "\\" + escape))
        index += 1
    return "".join(out)


def _parse_value(text: str):
    text = text.strip()
    if not text:
        raise AuditError("empty value")
    if text.startswith('"'):
        return _parse_string(text)
    if text in ("true", "false"):
        return text == "true"
    if text.startswith("["):
        if not text.endswith("]"):
            raise AuditError(f"unterminated array: {text!r}")
        return [_parse_value(item) for item in _split_items(text[1:-1])]
    if text.startswith("{"):
        if not text.endswith("}"):
            raise AuditError(f"unterminated inline table: {text!r}")
        table = {}
        for item in _split_items(text[1:-1]):
            match = KEY_RE.match(item)
            if not match:
                raise AuditError(f"invalid inline table entry: {item!r}")
            table[match.group(1)] = _parse_value(match.group(2))
        return table
    if text.startswith("0o"):
        return int(text[2:], 8)
    if re.fullmatch(r"[0-9]+", text):
        return int(text)
    raise AuditError(f"unsupported TOML value: {text!r}")


def _descend(doc: dict, parts: list[str]) -> dict:
    node = doc
    for part in parts:
        child = node.setdefault(part, {})
        if not isinstance(child, dict):
            raise AuditError(f"table {'.'.join(parts)} conflicts with a value")
        node = child
    return node


def parse_toml_subset(path: Path) -> dict:
    """Read one profile TOML file, rejecting syntax outside the profile shapes."""
    doc: dict = {}
    current = doc
    pending_key = None
    pending_value = ""
    lines = path.read_text(encoding="utf-8").splitlines()
    for number, raw in enumerate(lines, 1):
        line = _strip_comment(raw)
        if pending_key is not None:
            pending_value += " " + line
            if _balanced(pending_value):
                current[pending_key] = _parse_value(pending_value)
                pending_key = None
            continue
        if not line:
            continue
        header = HEADER_RE.match(line)
        if header:
            parts = header.group(1).split(".")
            array_table = line.startswith("[[")
            if array_table:
                parent = _descend(doc, parts[:-1])
                table = parent.setdefault(parts[-1], [])
                if not isinstance(table, list):
                    raise AuditError(f"{path}:{number}: array table conflicts with a value")
                current = {}
                table.append(current)
            else:
                current = _descend(doc, parts)
            continue
        key = KEY_RE.match(line)
        if not key:
            raise AuditError(f"{path}:{number}: unsupported profile syntax: {line!r}")
        pending_key = key.group(1)
        pending_value = key.group(2).strip()
        if _balanced(pending_value):
            if pending_key in current:
                raise AuditError(f"{path}:{number}: duplicate key {pending_key}")
            current[pending_key] = _parse_value(pending_value)
            pending_key = None
    if pending_key is not None:
        raise AuditError(f"{path}: unterminated value for {pending_key}")
    return doc


def _profile_initfs(document: dict, source: str) -> tuple[list[dict], list[dict], bool, bool]:
    initfs = document.get("initfs", {})
    if not isinstance(initfs, dict):
        raise AuditError(f"{source}: initfs is not a table")
    programs = initfs.get("programs", [])
    files = initfs.get("files", [])
    if not isinstance(programs, list) or not isinstance(files, list):
        raise AuditError(f"{source}: initfs programs/files are not arrays")
    for program in programs:
        if not isinstance(program, dict) or not isinstance(program.get("package"), str):
            raise AuditError(f"{source}: invalid initfs program entry {program!r}")
        if not isinstance(program.get("path"), str):
            raise AuditError(f"{source}: initfs program without a path")
    for entry in files:
        if not isinstance(entry, dict) or not isinstance(entry.get("src"), str):
            raise AuditError(f"{source}: invalid initfs file entry {entry!r}")
        if not isinstance(entry.get("path"), str):
            raise AuditError(f"{source}: initfs file without a path")
    return programs, files, bool(initfs.get("dynamic", False)), bool(initfs.get("release", False))


def load_profile(
    profiles_dir: Path, name: str, stack: tuple[str, ...] = ()
) -> tuple[list[dict], list[dict], bool, bool]:
    """Resolve a profile, its parent and its sorted fragments like `cargo xtask`."""
    path = profiles_dir / f"{name}.toml"
    document = parse_toml_subset(path)
    if document.get("name") != name:
        raise AuditError(f"{path}: profile name does not match {name}")
    programs: list[dict] = []
    files: list[dict] = []
    dynamic = False
    release = False
    parent = document.get("extends")
    if parent is not None:
        if not isinstance(parent, str) or stack:
            raise AuditError(f"{path}: profile extends is invalid or more than one level")
        programs, files, dynamic, release = load_profile(profiles_dir, parent, (name,))
        programs = list(programs)
        files = list(files)
    own_programs, own_files, own_dynamic, own_release = _profile_initfs(
        document, str(path)
    )
    programs.extend(own_programs)
    files.extend(own_files)
    dynamic = dynamic or own_dynamic
    release = release or own_release
    fragments = profiles_dir / f"{name}.d"
    if fragments.is_dir():
        for fragment in sorted(fragments.glob("*.toml")):
            f_programs, f_files, f_dynamic, f_release = _profile_initfs(
                parse_toml_subset(fragment), str(fragment)
            )
            programs.extend(f_programs)
            files.extend(f_files)
            dynamic = dynamic or f_dynamic
            release = release or f_release
    seen: dict[str, str] = {}
    for program in programs:
        stage = program["path"]
        if not stage.startswith("/") or len(stage) < 2:
            raise AuditError(f"{path}: initfs path {stage!r} is not absolute")
        if stage in seen:
            raise AuditError(f"duplicate initfs path {stage} in {seen[stage]} and {path}")
        seen[stage] = path.name
    for entry in files:
        stage = entry["path"]
        if not stage.startswith("/") or len(stage) < 2:
            raise AuditError(f"{path}: initfs path {stage!r} is not absolute")
        if stage in seen:
            raise AuditError(f"duplicate initfs path {stage} in {seen[stage]} and {path}")
        seen[stage] = path.name
    return programs, files, dynamic, release


# --------------------------------------------------------------------------
# Provenance and licence data.
# --------------------------------------------------------------------------


def load_provenance(path: Path) -> list[dict]:
    if not path.is_file():
        raise AuditError(f"provenance file is missing: {path}")
    document = parse_toml_subset(path)
    imports = document.get("import", [])
    if not isinstance(imports, list):
        raise AuditError(f"{path}: import records must be an array of tables")
    for entry in imports:
        if not isinstance(entry, dict):
            raise AuditError(f"{path}: invalid import record {entry!r}")
        for key in ("path", "upstream", "license"):
            if not isinstance(entry.get(key), str) or not entry[key]:
                raise AuditError(f"{path}: import record without {key}: {entry!r}")
    return imports


def load_packages(root: Path) -> dict[str, dict]:
    command = [
        "cargo",
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--locked",
        "--offline",
    ]
    result = subprocess.run(
        command, cwd=root, capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise AuditError(
            f"cargo metadata failed with exit {result.returncode}: {result.stderr.strip()}"
        )
    metadata = json.loads(result.stdout)
    packages = {}
    for package in metadata.get("packages", []):
        directory = Path(package["manifest_path"]).parent
        try:
            relative = directory.relative_to(root)
        except ValueError:
            relative = Path(os.path.relpath(directory, root))
        packages[package["name"]] = {
            "dir": relative.as_posix(),
            "license": package.get("license") or "",
        }
    return packages


def find_import(imports: list[dict], source: str) -> dict | None:
    if not source:
        return None
    exact = [entry for entry in imports if entry["path"] == source]
    if exact:
        return exact[0]
    prefix = [entry for entry in imports if entry["path"].startswith(source + "/")]
    if not prefix:
        return None
    return min(prefix, key=lambda entry: len(entry["path"]))


def upstreams_under(imports: list[dict], source: str) -> str:
    if not source:
        return "-"
    names = sorted(
        {
            entry["upstream"]
            for entry in imports
            if entry["path"] == source or entry["path"].startswith(source + "/")
        }
    )
    return ",".join(names) if names else "-"


def notice_for(imports: list[dict], source: str) -> str:
    """Return the provenance upstream or the repository NOTICE for first-party files."""
    upstream = upstreams_under(imports, source)
    return upstream if upstream != "-" else "NOTICE"


def licence_file(directory: Path) -> Path | None:
    for name in LICENSE_FILE_NAMES:
        candidate = directory / name
        if candidate.is_file():
            return candidate
    return None


def generated_owner(root: Path, source: str, arch: str) -> tuple[str, str | None]:
    if not source.startswith("build/"):
        return "file", None
    parts = source[len("build/") :].split("/")
    if parts and parts[0] == arch:
        parts = parts[1:]
    if not parts:
        return "file", None
    head, tail = parts[0], parts[1:]
    if head == "fixtures":
        if tail:
            candidate = f"tests/fixtures/{tail[0]}"
            if (root / candidate).is_dir():
                return "file", candidate
        return "file", None
    for base in ("addons", "userland"):
        candidate = f"{base}/{head}"
        if (root / candidate).is_dir():
            return "addon", candidate
    return "file", None


def build_rows(
    root: Path, arch: str, profile: str, packages: dict[str, dict], imports: list[dict]
) -> list[dict]:
    programs, files, _dynamic, _release = load_profile(root / "profiles", profile)
    rows: list[dict] = []
    for program in programs:
        package = packages.get(program["package"])
        source = package["dir"] if package else ""
        rows.append(
            {
                "kind": "program",
                "stage_path": program["path"],
                "source": source,
                "license": package["license"] if package else "",
                "notice": notice_for(imports, source),
            }
        )
    for entry in files:
        source = entry["src"].replace("{arch}", arch)
        if source.startswith("build/"):
            kind, owner = generated_owner(root, source, arch)
            if kind == "addon" and owner is not None:
                record = find_import(imports, owner)
                rows.append(
                    {
                        "kind": "addon",
                        "stage_path": entry["path"],
                        "source": record["path"] if record else owner,
                        "license": record["license"] if record else "",
                        "notice": record["upstream"] if record else "",
                    }
                )
            else:
                rows.append(
                    {
                        "kind": "file",
                        "stage_path": entry["path"],
                        "source": owner or "",
                        "license": PROJECT_LICENSE,
                        "notice": notice_for(imports, owner) if owner else "",
                    }
                )
        else:
            record = find_import(imports, source)
            rows.append(
                {
                    "kind": "file",
                    "stage_path": entry["path"],
                    "source": source,
                    "license": record["license"] if record else PROJECT_LICENSE,
                    "notice": record["upstream"] if record else notice_for(imports, source),
                }
            )
    rows.sort(key=lambda row: (row["stage_path"], row["kind"]))
    return rows


# --------------------------------------------------------------------------
# Manifest verification.
# --------------------------------------------------------------------------


def verify_rows(
    root: Path,
    rows: list[dict],
    expected: list[dict],
    packages: dict[str, dict],
    imports: list[dict],
) -> list[str]:
    failures: list[str] = []
    by_source = {package["dir"]: package for package in packages.values()}
    expected_by_key = {(row["kind"], row["stage_path"]): row for row in expected}
    actual_by_key = {(row["kind"], row["stage_path"]): row for row in rows}
    for key in sorted(set(expected_by_key) | set(actual_by_key)):
        kind, stage = key
        expected_row = expected_by_key.get(key)
        actual_row = actual_by_key.get(key)
        if actual_row is None and expected_row is not None:
            failures.append(
                f"missing artifact {stage}: {kind} {expected_row['source']}"
            )
            continue
        if expected_row is None and actual_row is not None:
            failures.append(
                f"unexpected artifact {stage}: {kind} {actual_row['source']}"
            )
            continue
        for field in ("source", "license", "notice"):
            if actual_row[field] != expected_row[field]:
                failures.append(
                    f"artifact {stage}: {field} {actual_row[field]!r} does not match "
                    f"the resolved profile {expected_row[field]!r}"
                )
    repository_notice = root / "NOTICE"
    seen: set[str] = set()
    for row in rows:
        stage = row["stage_path"]
        if stage in seen:
            failures.append(f"duplicate artifact row {stage}")
        seen.add(stage)
        license_id = row["license"]
        if not license_id:
            failures.append(f"artifact {stage}: licence is missing")
        elif license_id not in ALLOWED_LICENSES:
            failures.append(f"artifact {stage}: licence {license_id} is not allowed")
        if not row["notice"]:
            failures.append(f"artifact {stage}: notice entry is missing")
        elif row["notice"] == "NOTICE" and not repository_notice.is_file():
            failures.append(f"artifact {stage}: repository NOTICE is missing")
        if row["kind"] == "program":
            package = by_source.get(row["source"])
            if package is None:
                failures.append(f"artifact {stage}: program source {row['source']} is missing")
                continue
            if package["license"] != license_id:
                failures.append(
                    f"artifact {stage}: licence {license_id} does not match "
                    f"Cargo.toml {package['license'] or 'missing'}"
                )
            notice = notice_for(imports, row["source"])
            if notice != row["notice"]:
                failures.append(
                    f"artifact {stage}: notice {row['notice']} does not match "
                    f"provenance {notice}"
                )
            for entry in imports:
                if entry["path"] == row["source"] or entry["path"].startswith(
                    row["source"] + "/"
                ):
                    if "GPL" in entry["license"]:
                        failures.append(
                            f"artifact {stage}: forbidden licence {entry['license']} "
                            f"in {entry['path']}"
                        )
        elif row["kind"] == "addon":
            directory = root / row["source"]
            record = find_import(imports, row["source"])
            if not directory.is_dir():
                failures.append(f"artifact {stage}: add-on source {row['source']} is missing")
            elif record is None or record["path"] != row["source"]:
                failures.append(
                    f"artifact {stage}: notice entry for {row['source']} is missing"
                )
            else:
                if record["license"] != license_id:
                    failures.append(
                        f"artifact {stage}: licence {license_id} does not match "
                        f"provenance {record['license']}"
                    )
                if record["upstream"] != row["notice"]:
                    failures.append(
                        f"artifact {stage}: notice {row['notice']} does not match "
                        f"provenance {record['upstream']}"
                    )
                if licence_file(directory) is None:
                    failures.append(
                        f"artifact {stage}: licence file is missing in {row['source']}"
                    )
        else:
            if not row["source"]:
                failures.append(f"artifact {stage}: source is missing")
            elif not row["source"].startswith("build/") and not (
                root / row["source"]
            ).exists():
                failures.append(f"artifact {stage}: source {row['source']} is missing")
            notice = notice_for(imports, row["source"])
            if notice != row["notice"]:
                failures.append(
                    f"artifact {stage}: notice {row['notice']} does not match "
                    f"provenance {notice}"
                )
    return failures


# --------------------------------------------------------------------------
# Manifest files and hashes.
# --------------------------------------------------------------------------


def write_manifest(path: Path, rows: list[dict], arch: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# SPDX-License-Identifier: BSD-3-Clause",
        "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        f"# RavenBSD release artifact manifest arch={arch}",
        "kind\tstage_path\tsource\tlicense\tnotice",
    ]
    for row in rows:
        values = [
            row["kind"],
            row["stage_path"],
            row["source"],
            row["license"],
            row["notice"],
        ]
        if any("\t" in value or "\n" in value for value in values):
            raise AuditError(f"manifest value for {row['stage_path']} contains a tab or newline")
        lines.append("\t".join(values))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def read_manifest(path: Path) -> list[dict]:
    rows = []
    header = None
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.startswith("#") or not line:
            continue
        fields = line.split("\t")
        if header is None:
            header = fields
            if header != ["kind", "stage_path", "source", "license", "notice"]:
                raise AuditError(f"{path}:{number}: unexpected manifest header {fields!r}")
            continue
        if len(fields) != 5:
            raise AuditError(f"{path}:{number}: manifest row has {len(fields)} fields")
        rows.append(dict(zip(header, fields, strict=True)))
    if header is None:
        raise AuditError(f"{path}: manifest has no header")
    return rows


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_hashes(path: Path, root: Path, manifest: Path, rows: list[dict]) -> None:
    lines = [f"{sha256_file(manifest)}  {manifest.name}"]
    for row in sorted(rows, key=lambda item: item["stage_path"]):
        if row["kind"] != "file" or not row["source"]:
            continue
        candidate = root / row["source"]
        if candidate.is_file():
            lines.append(f"{sha256_file(candidate)}  {row['stage_path'].lstrip('/')}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


# --------------------------------------------------------------------------
# Scaffold C/C++ implementation scan.
# --------------------------------------------------------------------------


def classify_c_path(path: str) -> str:
    if path.startswith("addons/"):
        return "addon"
    if path.startswith(VENDORED_PREFIXES):
        return "vendored"
    parts = path.split("/")
    if parts[0] in ("tests", "tools") or "tests" in parts or "test" in parts:
        return "test"
    if parts[0] in SCAFFOLD_ROOTS:
        return "scaffold"
    return "outside"


def scan_c_implementation(root: Path, paths: list[str]) -> list[str]:
    hits = []
    for path in paths:
        if not path.endswith(IMPLEMENTATION_EXTENSIONS):
            continue
        if classify_c_path(path) == "scaffold":
            hits.append(path)
    return sorted(hits)


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, check=False
    )
    if result.returncode != 0:
        raise AuditError(
            f"git ls-files failed with exit {result.returncode}: "
            f"{result.stderr.decode(errors='replace').strip()}"
        )
    return [entry for entry in result.stdout.decode().split("\0") if entry]


def write_c_scan(path: Path, root: Path, paths: list[str]) -> tuple[list[str], int]:
    hits = scan_c_implementation(root, paths)
    implementation = [
        entry for entry in paths if entry.endswith(IMPLEMENTATION_EXTENSIONS)
    ]
    headers = [entry for entry in paths if entry.endswith(".h")]
    lines = [
        "# SPDX-License-Identifier: BSD-3-Clause",
        "# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        f"# scaffold implementation extensions: {' '.join(IMPLEMENTATION_EXTENSIONS)}",
        f"# implementation files: {len(implementation)}",
        f"# headers ignored (not implementation): {len(headers)}",
    ]
    for entry in sorted(implementation):
        lines.append(f"{classify_c_path(entry)}\t{entry}")
    lines.append(f"scaffold_hits\t{len(hits)}")
    for hit in hits:
        lines.append(f"hit\t{hit}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return hits, len(headers)


# --------------------------------------------------------------------------
# Command line.
# --------------------------------------------------------------------------


def management_root(root: Path) -> Path:
    configured = os.environ.get("RAVEN_MGMT_ROOT")
    if configured:
        return Path(configured).expanduser().resolve()
    # The product clone is commonly reached through a symlinked workspace. Search both
    # the logical PWD and the physical checkout so the durable evidence lands in the
    # real ravenbsd-prjmgmt checkout, not a sibling stub beside a symlink.
    bases = [os.environ.get("PWD", ""), str(Path.cwd()), str(root)]
    seen: set[Path] = set()
    for value in bases:
        if not value:
            continue
        base = Path(value).absolute()
        for ancestor in (base, *base.parents):
            if ancestor in seen:
                continue
            seen.add(ancestor)
            candidate = ancestor / "ravenbsd-prjmgmt"
            if (candidate / "tickets/INDEX.md").is_file():
                return candidate.resolve()
    raise AuditError(
        "management root not found: set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout"
    )


def default_profile(arch: str) -> str:
    try:
        return DEFAULT_PROFILES[arch]
    except KeyError:
        raise AuditError(f"no default release profile for {arch}") from None


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="artifact-audit",
        description="Resolve and verify the RavenBSD release artifact manifest.",
    )
    parser.add_argument("--arch", required=True, choices=("x86_64", "aarch64"))
    parser.add_argument(
        "--profile",
        default=None,
        help="profile name (default: release for x86_64, pi4-release for aarch64)",
    )
    parser.add_argument("--repo", type=Path, default=None)
    parser.add_argument("--manifest", type=Path, default=None)
    parser.add_argument("--evidence-dir", type=Path, default=None)
    parser.add_argument("--tag", default=None)
    return parser.parse_args(argv)


def run(args: argparse.Namespace) -> int:
    root = (args.repo or Path(__file__).resolve().parents[3]).resolve()
    if not (root / "Cargo.toml").is_file():
        raise AuditError(f"{root} is not the product checkout")
    profile = args.profile or default_profile(args.arch)
    evidence = (
        args.evidence_dir.resolve()
        if args.evidence_dir
        else management_root(root) / "tickets/evidence/artifacts/S85j"
    )
    evidence.mkdir(parents=True, exist_ok=True)
    tag = args.tag or args.arch
    packages = load_packages(root)
    imports = load_provenance(root / "PROVENANCE.toml")
    expected = build_rows(root, args.arch, profile, packages, imports)
    if args.manifest:
        manifest = args.manifest.resolve()
        rows = read_manifest(manifest)
    else:
        manifest = evidence / f"manifest-{args.arch}.tsv"
        rows = expected
        write_manifest(manifest, rows, args.arch)
        write_hashes(evidence / f"hashes-{args.arch}.txt", root, manifest, rows)
    failures = verify_rows(root, rows, expected, packages, imports)
    paths = tracked_files(root)
    hits, headers = write_c_scan(evidence / "c-cpp-scan.txt", root, paths)
    for hit in hits:
        failures.append(f"scaffold C/C++ implementation: {hit}")
    failures_path = evidence / f"failures-{tag}.txt"
    failures_path.write_text(
        "".join(failure + "\n" for failure in failures), encoding="utf-8"
    )
    for failure in failures:
        print(f"artifact-audit: FAIL {failure}", file=sys.stderr)
    print(
        f"artifact-audit: arch={args.arch} profile={profile} "
        f"artifacts={len(rows)} headers_ignored={headers} failures={len(failures)}"
    )
    print(f"artifact-audit: manifest={manifest} failures={failures_path}")
    return 1 if failures else 0


def main(argv: list[str] | None = None) -> int:
    try:
        return run(parse_args(argv if argv is not None else sys.argv[1:]))
    except AuditError as error:
        print(f"artifact-audit: error: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"artifact-audit: error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
