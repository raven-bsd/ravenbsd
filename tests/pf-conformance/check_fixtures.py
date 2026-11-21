#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K277 fixture integrity and independent tcpdump decode checks.

The checker is deliberately independent of raven-pf: it validates the fixture
schemas, the pfctl manifest, the pcap structure/checksums, and compares the
committed pcaps with the host tcpdump decoder when it is available.
"""

from __future__ import annotations

import argparse
import hashlib
import ipaddress
import shlex
import shutil
import struct
import subprocess
import sys
from pathlib import Path

HEADER_SUFFIXES = {".py", ".sh", ".md", ".rs", ".toml", ".yml", ".ld", ".tsv", ".conf"}
SPDX = "SPDX-License-Identifier: BSD-3-Clause"
COPYRIGHT = "Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors"

errors: list[str] = []


def error(message: str) -> None:
    errors.append(message)


def read_rows(path: Path) -> list[list[str]]:
    rows = []
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = [field.strip() for field in stripped.split("|")]
        if len(fields) < 2:
            error(f"{path}:{index}: expected pipe-separated fields")
            continue
        rows.append(fields)
    return rows


def check_headers(root: Path) -> None:
    for base in (
        root / "tests/pf-conformance",
        root / "tests/fixtures/pfctl",
        root / "tests/fixtures/tcpdump",
    ):
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.suffix not in HEADER_SUFFIXES:
                continue
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()[:5]
            joined = "\n".join(lines)
            if SPDX not in joined:
                error(f"{path}: missing SPDX header in the first 5 lines")
            if COPYRIGHT not in joined:
                error(f"{path}: missing Copyright header in the first 5 lines")


def check_pf_conformance(root: Path) -> None:
    base = root / "tests/pf-conformance"
    manifest = base / "manifest.tsv"
    if not manifest.is_file():
        error(f"{manifest}: missing")
        return
    semantics = base / "semantics.tsv"
    semantic_ids = set()
    if not semantics.is_file():
        error(f"{semantics}: missing")
    else:
        for fields in read_rows(semantics):
            if len(fields) != 3:
                error(f"{semantics}: expected 3 fields, got {fields}")
                continue
            semantic_ids.add(fields[0])
    seen = set()
    for fields in read_rows(manifest):
        if len(fields) != 6:
            error(f"{manifest}: expected 6 fields, got {fields}")
            continue
        ident, kind, rules, packets, support, expect = fields
        if ident in seen:
            error(f"{manifest}: duplicate id {ident}")
        seen.add(ident)
        rules_path = base / rules
        if not rules_path.is_file():
            error(f"{manifest}: {ident}: missing rules file {rules_path}")
            continue
        if kind == "scenario":
            if packets == "-":
                error(f"{manifest}: {ident}: scenario has no packet file")
                continue
            packet_path = base / packets
            if not packet_path.is_file():
                error(f"{manifest}: {ident}: missing packet file {packet_path}")
                continue
            check_packets(packet_path, support, ident)
        elif kind == "parse-error":
            if support != "raven-pf-parse-error":
                error(f"{manifest}: {ident}: parse-error needs raven-pf-parse-error support")
            line, _, substring = expect.partition(",")
            if not line.isdigit() or not substring:
                error(f"{manifest}: {ident}: expect must be <line>,<substring>")
        else:
            error(f"{manifest}: {ident}: unknown kind {kind}")
        if support not in {
            "raven-pf",
            "raven-pf-parse-error",
            "raven-pf-known-divergence",
            "donor-only",
        }:
            error(f"{manifest}: {ident}: unknown support {support}")
        if ident not in semantic_ids:
            error(f"{semantics}: no row for manifest id {ident}")
    for ident in semantic_ids - seen:
        error(f"{semantics}: row for unknown manifest id {ident}")


def check_translation(path: Path, line_number: int, text: str, support: str) -> None:
    if support not in {"raven-pf", "raven-pf-known-divergence"}:
        error(f"{path}:{line_number}: translation field on a non-raven-pf row")
        return
    try:
        source, destination = text.split(">", 1)
        source_ip, source_port = source.rsplit(":", 1)
        destination_ip, destination_port = destination.rsplit(":", 1)
        ipaddress.ip_address(source_ip)
        ipaddress.ip_address(destination_ip)
    except ValueError as exc:
        error(f"{path}:{line_number}: bad translation {text}: {exc}")
        return
    for value, label in ((source_port, "source"), (destination_port, "destination")):
        if not value.isdigit() or not 0 <= int(value) <= 65535:
            error(f"{path}:{line_number}: bad translation {label} port {value}")


def check_packets(path: Path, support: str, scenario: str) -> None:
    rows = 0
    counters = 0
    text = path.read_text(encoding="utf-8")
    if "# reference:" not in text:
        error(f"{path}: missing `# reference:` donor trace")
    if support == "donor-only" and "# support: donor-only" not in text:
        error(f"{path}: donor-only scenario must say `# support: donor-only`")
    for line_number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("# expect-counters "):
            counters += 1
            continue
        if not stripped or stripped.startswith("#"):
            continue
        rows += 1
        fields = stripped.split()
        if len(fields) not in {12, 13, 14}:
            error(f"{path}:{line_number}: expected 12..14 fields, found {len(fields)}")
            continue
        now, direction, interface, proto, source, sport, destination, dport, length, flags, expected, disposition = fields[:12]
        fragment_field = fields[12] if len(fields) >= 13 else None
        translation = fields[13] if len(fields) == 14 and fields[13] != "-" else None
        if not now.isdigit():
            error(f"{path}:{line_number}: bad now {now}")
        if direction not in {"in", "out"}:
            error(f"{path}:{line_number}: bad direction {direction}")
        if not interface:
            error(f"{path}:{line_number}: empty interface")
        if proto not in {"tcp", "udp", "icmp", "icmp6"}:
            error(f"{path}:{line_number}: bad protocol {proto}")
        try:
            source_ip = ipaddress.ip_address(source)
        except ValueError as exc:
            error(f"{path}:{line_number}: bad source {source}: {exc}")
            source_ip = None
        try:
            destination_ip = ipaddress.ip_address(destination)
        except ValueError as exc:
            error(f"{path}:{line_number}: bad destination {destination}: {exc}")
            destination_ip = None
        for value, label in ((sport, "source port"), (dport, "destination port")):
            if not value.isdigit() or not 0 <= int(value) <= 65535:
                error(f"{path}:{line_number}: bad {label} {value}")
        if not length.isdigit() or int(length) == 0:
            error(f"{path}:{line_number}: bad length {length}")
        if flags != "-" and not set(flags) <= set("SAFR"):
            error(f"{path}:{line_number}: bad flags {flags}")
        if expected not in {"pass", "block", "queued"}:
            error(f"{path}:{line_number}: bad expected verdict {expected}")
        if disposition not in {"-", "created", "existing"}:
            error(f"{path}:{line_number}: bad disposition {disposition}")
        if fragment_field not in {None, "-", "frag"}:
            offset, _, more = fragment_field.partition("/")
            if not offset.isdigit() or int(offset) % 8 or more not in {"0", "1"}:
                error(f"{path}:{line_number}: bad fragment {fragment_field}")
        if expected == "queued" and fragment_field in {None, "-", "frag"}:
            error(f"{path}:{line_number}: `queued` needs an offset/MF fragment field")
        if translation is not None:
            check_translation(path, line_number, translation, support)
        executed = support in {"raven-pf", "raven-pf-known-divergence"}
        if executed:
            if expected == "queued":
                error(f"{path}:{line_number}: raven-pf scenario has a queued row")
        elif source_ip is not None and source_ip.version not in {4, 6}:
            error(f"{path}:{line_number}: bad source family")
        _ = destination_ip
    if rows == 0:
        error(f"{path}: no packet rows")
    if support in {"raven-pf", "raven-pf-known-divergence"}:
        if counters == 0:
            error(f"{path}: no expect-counters line")
        if rows == 0:
            error(f"{scenario}: no supported rows")


def check_pfctl(root: Path) -> None:
    base = root / "tests/fixtures/pfctl"
    manifest = base / "manifest.tsv"
    if not manifest.is_file():
        error(f"{manifest}: missing")
        return
    listed = {fields[0] for fields in read_rows(manifest) if fields}
    for path in sorted(base.rglob("*.conf")):
        relative = path.relative_to(base).as_posix()
        if relative not in listed:
            error(f"{manifest}: {relative}: fixture has no manifest row")
    for fields in read_rows(manifest):
        if len(fields) != 5:
            error(f"{manifest}: expected 5 fields, got {fields}")
            continue
        name, status, line, construct, note = fields
        path = base / name
        if not path.is_file():
            error(f"{manifest}: missing {path}")
            continue
        if status not in {"ok", "error"}:
            error(f"{manifest}: {name}: bad status {status}")
            continue
        if status == "ok":
            if line != "-":
                error(f"{manifest}: {name}: accepted file needs line '-'")
            if not construct:
                error(f"{manifest}: {name}: missing construct")
            continue
        if not line.isdigit():
            error(f"{manifest}: {name}: rejected file needs a line number")
            continue
        count = len(path.read_text(encoding="utf-8").splitlines())
        if int(line) > count:
            error(f"{manifest}: {name}: line {line} beyond {count} lines")
        if not note:
            error(f"{manifest}: {name}: missing note")


def checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\x00"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def pseudo4(src: bytes, dst: bytes, proto: int, length: int) -> bytes:
    return src + dst + struct.pack("!BBH", 0, proto, length)


def pseudo6(src: bytes, dst: bytes, proto: int, length: int) -> bytes:
    return src + dst + struct.pack("!I", length) + b"\x00\x00\x00" + struct.pack("!B", proto)


def check_transport(proto: int, payload: bytes, src: bytes, dst: bytes, family: int) -> None:
    if proto == 6:
        if len(payload) < 20:
            raise ValueError("truncated TCP header")
        offset = (payload[12] >> 4) * 4
        if offset < 20 or offset > len(payload):
            raise ValueError("bad TCP data offset")
        segment = payload
        pseudo = pseudo6(src, dst, 6, len(segment)) if family == 6 else pseudo4(src, dst, 6, len(segment))
        if checksum(pseudo + segment) != 0:
            raise ValueError("bad TCP checksum")
    elif proto == 17:
        if len(payload) < 8:
            raise ValueError("truncated UDP header")
        length = struct.unpack("!H", payload[4:6])[0]
        if length < 8 or length > len(payload):
            raise ValueError("bad UDP length")
        datagram = payload[:length]
        stored = struct.unpack("!H", datagram[6:8])[0]
        if family == 4 and stored == 0:
            return
        pseudo = pseudo6(src, dst, 17, len(datagram)) if family == 6 else pseudo4(src, dst, 17, len(datagram))
        if checksum(pseudo + datagram) != 0:
            raise ValueError("bad UDP checksum")
    elif proto == 1 and family == 4:
        if len(payload) < 8 or checksum(payload) != 0:
            raise ValueError("bad ICMP checksum")
    elif proto == 58 and family == 6:
        if len(payload) < 8:
            raise ValueError("truncated ICMPv6 header")
        if checksum(pseudo6(src, dst, 58, len(payload)) + payload) != 0:
            raise ValueError("bad ICMPv6 checksum")


def check_ipv4(payload: bytes) -> None:
    if len(payload) < 20:
        raise ValueError("truncated IPv4 header")
    if payload[0] >> 4 != 4:
        raise ValueError("not IPv4")
    ihl = (payload[0] & 0x0F) * 4
    if ihl < 20 or ihl > len(payload):
        raise ValueError("bad IPv4 IHL")
    total = struct.unpack("!H", payload[2:4])[0]
    if total < ihl or total > len(payload):
        raise ValueError("bad IPv4 total length")
    if checksum(payload[:ihl]) != 0:
        raise ValueError("bad IPv4 header checksum")
    flags_frag = struct.unpack("!H", payload[6:8])[0]
    offset = flags_frag & 0x1FFF
    more = bool(flags_frag & 0x2000)
    proto = payload[9]
    body = payload[ihl:total]
    if offset == 0 and not more:
        check_transport(proto, body, payload[12:16], payload[16:20], 4)


def check_ipv6(payload: bytes) -> None:
    if len(payload) < 40:
        raise ValueError("truncated IPv6 header")
    if payload[0] >> 4 != 6:
        raise ValueError("not IPv6")
    length = struct.unpack("!H", payload[4:6])[0]
    if 40 + length > len(payload):
        raise ValueError("bad IPv6 payload length")
    next_header = payload[6]
    body = payload[40:40 + length]
    src = payload[8:24]
    dst = payload[24:40]
    check_transport(next_header, body, src, dst, 6)


def check_pcap(path: Path) -> int:
    data = path.read_bytes()
    if len(data) < 24:
        raise ValueError("truncated pcap header")
    magic, major, minor, _, _, snaplen, linktype = struct.unpack("<IHHIIII", data[:24])
    if magic != 0xA1B2C3D4 or (major, minor) != (2, 4) or linktype != 1:
        raise ValueError("unexpected pcap header")
    offset = 24
    packets = 0
    while offset < len(data):
        if offset + 16 > len(data):
            raise ValueError("truncated record header")
        _, _, caplen, origlen = struct.unpack("<IIII", data[offset:offset + 16])
        if caplen > snaplen or origlen < caplen:
            raise ValueError("bad record lengths")
        offset += 16
        frame = data[offset:offset + caplen]
        if len(frame) != caplen:
            raise ValueError("truncated frame")
        offset += caplen
        if len(frame) < 14:
            raise ValueError("short Ethernet frame")
        ethertype = struct.unpack("!H", frame[12:14])[0]
        if ethertype == 0x0800:
            check_ipv4(frame[14:])
        elif ethertype == 0x86DD:
            check_ipv6(frame[14:])
        elif ethertype == 0x0806:
            if len(frame) < 14 + 28:
                raise ValueError("short ARP frame")
        else:
            raise ValueError(f"unexpected ethertype 0x{ethertype:04x}")
        packets += 1
    return packets


def check_tcpdump(root: Path) -> None:
    base = root / "tests/fixtures/tcpdump"
    manifest = base / "manifest.tsv"
    if not manifest.is_file():
        error(f"{manifest}: missing")
        return
    malformed_path = base / "malformed.tsv"
    malformed: dict[str, str] = {}
    if not malformed_path.is_file():
        error(f"{malformed_path}: missing")
    else:
        for fields in read_rows(malformed_path):
            if len(fields) != 2:
                error(f"{malformed_path}: expected 2 fields, got {fields}")
                continue
            malformed[fields[0]] = fields[1]
    entries: dict[str, str] = {}
    for fields in read_rows(manifest):
        if len(fields) != 6:
            error(f"{manifest}: expected 6 fields, got {fields}")
            continue
        name, count, linktype, digest, decoder, note = fields
        path = base / name
        if not path.is_file():
            error(f"{manifest}: missing {path}")
            continue
        entries[name] = decoder
        if linktype != "ethernet":
            error(f"{manifest}: {name}: unsupported linktype {linktype}")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != digest:
            error(f"{manifest}: {name}: sha256 {actual} != {digest}")
        try:
            packets = check_pcap(path)
        except (ValueError, struct.error) as exc:
            error(f"{path}: {exc}")
            continue
        if packets != int(count):
            error(f"{path}: {packets} packets, manifest says {count}")
    listed = set(entries) | set(malformed)
    for path in sorted(base.glob("*.pcap")):
        if path.name not in listed:
            error(f"{base}: {path.name}: pcap has no manifest row")
    for name, expected in malformed.items():
        path = base / name
        if not path.is_file():
            error(f"{malformed_path}: missing {path}")
            continue
        try:
            check_pcap(path)
        except (ValueError, struct.error) as exc:
            if expected not in str(exc):
                error(f"{path}: expected `{expected}`, got `{exc}`")
        else:
            error(f"{path}: malformed pcap was accepted")
    if not shutil.which("tcpdump"):
        print("SKIP tcpdump decode: tcpdump is not installed")
        return
    for name in malformed:
        result = subprocess.run(
            ["tcpdump", "-nn", "-r", str(base / name)],
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        )
        if result.returncode == 0:
            error(f"{base / name}: tcpdump accepted a malformed pcap")
        elif not result.stderr.strip():
            error(f"{base / name}: tcpdump failed without a diagnostic")
    decode_path = base / "decode.tsv"
    if decode_path.is_file():
        decoded: dict[str, list[str]] = {}
        for fields in read_rows(decode_path):
            if len(fields) != 3:
                error(f"{decode_path}: expected 3 fields, got {fields}")
                continue
            name, line, substring = fields
            if name not in entries:
                error(f"{decode_path}: unknown pcap {name}")
                continue
            if name not in decoded:
                result = subprocess.run(
                    shlex.split(entries[name]) + ["-r", str(base / name)],
                    capture_output=True,
                    text=True,
                    timeout=20,
                    check=False,
                )
                if result.returncode != 0:
                    error(f"{base / name}: tcpdump failed: {result.stderr.strip()}")
                    decoded[name] = []
                    continue
                decoded[name] = result.stdout.splitlines()
            lines = decoded[name]
            if line == "*":
                if substring not in "\n".join(lines):
                    error(f"{decode_path}: {name}: missing `{substring}`")
            elif line.isdigit():
                index = int(line) - 1
                if index >= len(lines):
                    error(f"{decode_path}: {name}: line {line} beyond {len(lines)}")
                elif substring not in lines[index]:
                    error(f"{decode_path}: {name}:{line}: missing `{substring}` in `{lines[index]}`")
            else:
                error(f"{decode_path}: bad line selector {line}")
    filters_path = base / "filters.tsv"
    if filters_path.is_file():
        for fields in read_rows(filters_path):
            if len(fields) != 3:
                error(f"{filters_path}: expected 3 fields, got {fields}")
                continue
            name, expression, expected = fields
            if name not in entries:
                error(f"{filters_path}: unknown pcap {name}")
                continue
            result = subprocess.run(
                ["tcpdump", "-nn", "-r", str(base / name), expression],
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
            if result.returncode != 0:
                error(f"{filters_path}: {name} `{expression}`: {result.stderr.strip()}")
                continue
            count = len([line for line in result.stdout.splitlines() if line.strip()])
            if count != int(expected):
                error(f"{filters_path}: {name} `{expression}`: {count} != {expected}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    default_root = Path(__file__).resolve().parents[2]
    parser.add_argument("--root", type=Path, default=default_root)
    args = parser.parse_args()
    root = args.root.resolve()
    check_headers(root)
    check_pf_conformance(root)
    check_pfctl(root)
    check_tcpdump(root)
    if errors:
        for message in errors:
            print(f"FAIL {message}")
        print(f"fixture-check: {len(errors)} failure(s)")
        return 1
    print("fixture-check: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
