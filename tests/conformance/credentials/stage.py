#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S179: inject the credential probe into a release initfs image.

The release profile has no hook for a test-only binary, and ``tests/fixtures/**`` is outside this
ticket's Owns. This script parses the canonical initfs archive (CONTRACTS C7), adds
``/bin/cred-probe`` with mode 0755, and writes a deterministic replacement. The format is the one
implemented by ``crates/raven-initfs`` (``RAVENFS1`` version 2, 64-byte header, 48-byte entries,
page-aligned data, IEEE CRC-32).

Usage:
  python3 tests/conformance/credentials/stage.py --input build/x86_64/initfs.img \
      --probe build/conformance/credentials/cred-probe \
      --output build/conformance/credentials/initfs.img
  python3 tests/conformance/credentials/stage.py --input ... --probe ... --output ... \
      --add /etc/services.d/cred-umask.toml 0644 tests/conformance/credentials/cred-umask.service.toml \
      --replace /etc/svcd.conf 0644 tests/conformance/credentials/cred.svcd.conf
  python3 tests/conformance/credentials/stage.py --selftest
  python3 tests/conformance/credentials/stage.py --roundtrip build/x86_64/initfs.img

Exit: 0 on success, 1 on a format or self-test failure, 2 for a usage problem.
"""

from __future__ import annotations

import argparse
import struct
import sys
import zlib
from pathlib import Path

MAGIC = b"RAVENFS1"
VERSION = 2
HEADER_SIZE = 64
ENTRY_SIZE = 48
PAGE_SIZE = 4096
HEADER = struct.Struct("<8sIIIIQQQQ8s")
ENTRY = struct.Struct("<IIQQIIQ8s")
DEFAULT_EPOCH = 1_767_225_600


class InitfsError(Exception):
    """A malformed or unsupported initfs image."""


def _align(value: int, align: int = PAGE_SIZE) -> int:
    return (value + align - 1) & ~(align - 1)


def parse(image: bytes) -> tuple[int, list[tuple[str, int, int, bytes]]]:
    """Parse ``image`` and return ``(epoch, [(path, mode, mtime, data), ...])``."""
    if len(image) < HEADER_SIZE:
        raise InitfsError("image is shorter than the header")
    magic, version, header_size, count, entry_size, names_off, names_len, total, epoch, _reserved = (
        HEADER.unpack_from(image, 0)
    )
    if magic != MAGIC:
        raise InitfsError(f"bad magic {magic!r}")
    if version not in (1, VERSION):
        raise InitfsError(f"unsupported version {version}")
    if header_size != HEADER_SIZE or entry_size != ENTRY_SIZE:
        raise InitfsError(f"unexpected header/entry size {header_size}/{entry_size}")
    if total > len(image):
        raise InitfsError(f"total {total} exceeds image {len(image)}")
    if names_off + names_len > total:
        raise InitfsError("names blob exceeds the image")
    names = image[names_off : names_off + names_len]
    entries: list[tuple[str, int, int, bytes]] = []
    seen: set[str] = set()
    for index in range(count):
        base = HEADER_SIZE + index * ENTRY_SIZE
        name_off, name_len, data_off, data_len, mode, crc, mtime, _entry_reserved = (
            ENTRY.unpack_from(image, base)
        )
        if name_off + name_len > len(names):
            raise InitfsError(f"entry {index} name exceeds the names blob")
        path = names[name_off : name_off + name_len].decode("utf-8")
        if path in seen:
            raise InitfsError(f"duplicate path {path}")
        seen.add(path)
        if data_off + data_len > total:
            raise InitfsError(f"{path}: data exceeds the image")
        data = image[data_off : data_off + data_len]
        if zlib.crc32(data) & 0xFFFFFFFF != crc:
            raise InitfsError(f"{path}: CRC mismatch")
        entries.append((path, mode, mtime, data))
    if version == 1:
        epoch = DEFAULT_EPOCH
    return epoch, entries


def build(epoch: int, files: list[tuple[str, int, int, bytes]]) -> bytes:
    """Build a deterministic ``RAVENFS1`` version 2 image from ``files``."""
    ordered = sorted(files, key=lambda item: item[0].encode())
    names_len = sum(len(path.encode()) for path, _mode, _mtime, _data in ordered)
    names_off = HEADER_SIZE + len(ordered) * ENTRY_SIZE
    data_start = _align(names_off + names_len)
    data_bytes = sum(_align(len(data)) for _path, _mode, _mtime, data in ordered)
    total = data_start + data_bytes

    image = bytearray(total)
    HEADER.pack_into(
        image, 0, MAGIC, VERSION, HEADER_SIZE, len(ordered), ENTRY_SIZE,
        names_off, names_len, total, epoch, b"\0" * 8,
    )
    name_cursor = names_off
    data_cursor = data_start
    for index, (path, mode, mtime, data) in enumerate(ordered):
        encoded = path.encode()
        name_off = name_cursor - names_off
        ENTRY.pack_into(
            image, HEADER_SIZE + index * ENTRY_SIZE, name_off, len(encoded), data_cursor,
            len(data), mode, zlib.crc32(data) & 0xFFFFFFFF, mtime, b"\0" * 8,
        )
        image[name_cursor : name_cursor + len(encoded)] = encoded
        image[data_cursor : data_cursor + len(data)] = data
        name_cursor += len(encoded)
        data_cursor += _align(len(data))
    return bytes(image)


def inject(image: bytes, path: str, data: bytes, mode: int = 0o755) -> bytes:
    """Return ``image`` with one new file added; a duplicate path is an error."""
    if not path.startswith("/") or path == "/" or "//" in path or path.endswith("/"):
        raise InitfsError(f"invalid absolute path {path!r}")
    if any(component in (".", "..") for component in path.split("/")):
        raise InitfsError(f"invalid path component in {path!r}")
    epoch, entries = parse(image)
    if any(entry[0] == path for entry in entries):
        raise InitfsError(f"initfs already contains {path}")
    entries.append((path, mode, 0, data))
    return build(epoch, entries)


def replace(image: bytes, path: str, data: bytes, mode: int = 0o644) -> bytes:
    """Return ``image`` with an existing file's data and mode replaced."""
    if not path.startswith("/") or path == "/" or "//" in path or path.endswith("/"):
        raise InitfsError(f"invalid absolute path {path!r}")
    epoch, entries = parse(image)
    if not any(entry[0] == path for entry in entries):
        raise InitfsError(f"initfs does not contain {path}")
    updated = [
        (entry[0], mode, entry[2], data) if entry[0] == path else entry for entry in entries
    ]
    return build(epoch, updated)


def selftest() -> int:
    """Exercise the parser/builder/injector without a real image."""
    failures: list[str] = []

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    check("crc-known-vector", zlib.crc32(b"123456789") & 0xFFFFFFFF, 0xCBF43926)

    files = [
        ("/init", 0o755, 0, b"init-image" * 300),
        ("/bin/tool", 0o755, 0, b"tool"),
        ("/etc/keep", 0o644, 0, b""),
    ]
    image = build(1234, files)
    epoch, parsed = parse(image)
    check("epoch", epoch, 1234)
    check("paths", [entry[0] for entry in parsed], ["/bin/tool", "/etc/keep", "/init"])
    check("data", {entry[0]: entry[3] for entry in parsed},
          {"/init": b"init-image" * 300, "/bin/tool": b"tool", "/etc/keep": b""})

    injected = inject(image, "/bin/cred-probe", b"probe" * 2000, 0o755)
    _, entries = parse(injected)
    selected = next(entry for entry in entries if entry[0] == "/bin/cred-probe")
    check("probe-mode", selected[1], 0o755)
    check("probe-data", selected[3], b"probe" * 2000)
    check("probe-count", len(entries), len(files) + 1)
    try:
        inject(injected, "/bin/cred-probe", b"again")
    except InitfsError as error:
        check("duplicate", str(error), "initfs already contains /bin/cred-probe")
    else:
        failures.append("duplicate: injected twice")

    replaced = replace(injected, "/etc/keep", b"replaced", 0o600)
    _, entries = parse(replaced)
    selected = next(entry for entry in entries if entry[0] == "/etc/keep")
    check("replace-mode", selected[1], 0o600)
    check("replace-data", selected[3], b"replaced")
    check("replace-count", len(entries), len(files) + 1)
    try:
        replace(replaced, "/etc/missing", b"", 0o644)
    except InitfsError as error:
        check("replace-missing", str(error), "initfs does not contain /etc/missing")
    else:
        failures.append("replace: missing path accepted")

    if failures:
        for failure in failures:
            print(f"stage: selftest FAIL: {failure}", file=sys.stderr)
        return 1
    print("stage: selftest PASS")
    return 0


def roundtrip(path: Path) -> int:
    """Rebuild an existing image and prove the bytes are identical."""
    try:
        image = path.read_bytes()
        epoch, entries = parse(image)
        rebuilt = build(epoch, entries)
    except (OSError, InitfsError) as error:
        print(f"stage: roundtrip FAIL: {error}", file=sys.stderr)
        return 1
    if rebuilt != image:
        offset = next(
            (index for index, (left, right) in enumerate(zip(rebuilt, image)) if left != right),
            min(len(rebuilt), len(image)),
        )
        print(
            f"stage: roundtrip FAIL: {path} differs at byte {offset} "
            f"(rebuilt {len(rebuilt)} bytes, original {len(image)} bytes)",
            file=sys.stderr,
        )
        return 1
    print(f"stage: roundtrip PASS: {path} ({len(entries)} entries, epoch {epoch})")
    return 0


def _staged_file(spec: list[str]) -> tuple[str, int, bytes]:
    """Parse ``PATH MODE SOURCE`` from one ``--add``/``--replace`` triple."""
    path, mode, source = spec
    try:
        parsed_mode = int(mode, 8)
    except ValueError as error:
        raise InitfsError(f"{path}: mode {mode!r} is not octal") from error
    try:
        data = Path(source).read_bytes()
    except OSError as error:
        raise InitfsError(f"{path}: {error}") from error
    return path, parsed_mode, data


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--add", nargs=3, action="append", default=[], metavar=("PATH", "MODE", "SOURCE"),
        help="add a new initfs file (MODE is octal, e.g. 0644)",
    )
    parser.add_argument(
        "--replace", nargs=3, action="append", default=[], metavar=("PATH", "MODE", "SOURCE"),
        help="replace an existing initfs file (MODE is octal, e.g. 0644)",
    )
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--roundtrip", type=Path)
    options = parser.parse_args(argv)
    if options.selftest:
        return selftest()
    if options.roundtrip is not None:
        return roundtrip(options.roundtrip)
    if options.input is None or options.output is None:
        print("stage: --input and --output are required", file=sys.stderr)
        return 2
    if options.probe is None and not options.add and not options.replace:
        print("stage: nothing to stage (give --probe, --add or --replace)", file=sys.stderr)
        return 2
    try:
        image = options.input.read_bytes()
        if options.probe is not None:
            probe = options.probe.read_bytes()
            if not probe:
                raise InitfsError(f"{options.probe} is empty")
            image = inject(image, "/bin/cred-probe", probe, 0o755)
        for spec in options.add:
            path, mode, data = _staged_file(spec)
            image = inject(image, path, data, mode)
        for spec in options.replace:
            path, mode, data = _staged_file(spec)
            image = replace(image, path, data, mode)
    except (OSError, InitfsError) as error:
        print(f"stage: {error}", file=sys.stderr)
        return 1
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_bytes(image)
    print(
        f"stage: wrote {options.output} ({len(image)} bytes, "
        f"probe {'yes' if options.probe is not None else 'no'}, "
        f"added {len(options.add)}, replaced {len(options.replace)})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
