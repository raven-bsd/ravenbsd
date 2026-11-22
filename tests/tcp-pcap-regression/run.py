#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K342 TCP pcap regression suite.

Replays the committed K281 tcpdump fixtures and the K323 loss/reordering pcaps
through a small independent pcap decoder and through the host ``tcpdump``.
It asserts packet decode fields, filters and the durable TCP outcome:
handshake/FIN order, payload bytes, retransmission sequence and counts.
Malformed, truncated, unknown-linktype and missing inputs must fail.
"""

from __future__ import annotations

import hashlib
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
K281 = ROOT / "tests" / "fixtures" / "tcpdump"
K323 = HERE / "fixtures" / "k323"
K323_CACHE = Path(tempfile.gettempdir()) / "k342-fixtures"
PAYLOAD = b"K253-zero-window-44-byte-payload-0123456789!"
PORT = 23090


def fixture(name: str) -> Path:
    """Materialize a committed .pcap.hex fixture into a temporary pcap."""
    source = K323 / f"{name}.pcap.hex"
    if not source.is_file():
        raise PcapError(f"missing fixture: {source}")
    data = bytes.fromhex(source.read_text())
    K323_CACHE.mkdir(parents=True, exist_ok=True)
    target = K323_CACHE / f"{name}.pcap"
    if not target.is_file() or target.read_bytes() != data:
        target.write_bytes(data)
    return target


class PcapError(Exception):
    """The pcap or Ethernet/IP packet is malformed."""


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_pcap(path: Path) -> tuple[int, list[bytes], bytes]:
    """Return (linktype, frames, raw header) or raise on a malformed file."""
    if not path.is_file():
        raise PcapError(f"missing pcap: {path}")
    data = path.read_bytes()
    if len(data) < 24:
        raise PcapError("pcap global header is truncated")
    magic = data[:4]
    if magic == b"\xd4\xc3\xb2\xa1":
        endian = "<"
    elif magic == b"\xa1\xb2\xc3\xd4":
        endian = ">"
    else:
        raise PcapError(f"unknown pcap magic {magic.hex()}")
    _, _, _, _, snaplen, linktype = struct.unpack(endian + "HHiIII", data[4:24])
    if linktype != 1:
        raise PcapError(f"unsupported link type {linktype}")
    frames: list[bytes] = []
    offset = 24
    while offset + 16 <= len(data):
        _, _, captured, _ = struct.unpack(endian + "IIII", data[offset:offset + 16])
        offset += 16
        if captured > snaplen:
            raise PcapError(f"captured length {captured} exceeds snaplen {snaplen}")
        if offset + captured > len(data):
            raise PcapError(
                f"truncated record: wanted {captured} bytes, have {len(data) - offset}"
            )
        frames.append(data[offset:offset + captured])
        offset += captured
    if offset != len(data):
        raise PcapError("trailing bytes after the last pcap record")
    return linktype, frames, data[:24]


def checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\x00"
    total = 0
    for index in range(0, len(data), 2):
        total += (data[index] << 8) | data[index + 1]
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def parse_frame(frame: bytes) -> dict | None:
    if len(frame) < 14:
        raise PcapError("Ethernet frame is shorter than 14 bytes")
    ethertype = struct.unpack("!H", frame[12:14])[0]
    if ethertype == 0x0806:
        return {"type": "arp", "length": len(frame)}
    if ethertype == 0x86DD:
        if len(frame) < 14 + 40 + 20:
            raise PcapError("IPv6 TCP frame is truncated")
        header = frame[14:54]
        next_header = header[6]
        if next_header != 6:
            return {"type": "ipv6", "protocol": next_header}
        src = header[8:24]
        dst = header[24:40]
        tcp_start = 54
        return parse_tcp(frame, tcp_start, f"ip6:{src.hex()}", f"ip6:{dst.hex()}")
    if ethertype != 0x0800:
        return None
    if len(frame) < 14 + 20:
        raise PcapError("IPv4 frame is truncated")
    ihl = (frame[14] & 0x0F) * 4
    if ihl < 20 or len(frame) < 14 + ihl:
        raise PcapError("IPv4 header length is invalid")
    total = struct.unpack("!H", frame[16:18])[0]
    if 14 + total > len(frame):
        raise PcapError("IPv4 total length exceeds the frame")
    ip_checksum_ok = checksum(frame[14:14 + ihl]) == 0
    protocol = frame[23]
    src = frame[26:30]
    dst = frame[30:34]
    if protocol != 6:
        return {
            "type": "ipv4",
            "protocol": protocol,
            "length": total,
            "checksum_ok": ip_checksum_ok,
        }
    if len(frame) < 14 + ihl + 20:
        raise PcapError("TCP header is truncated")
    parsed = parse_tcp(frame, 14 + ihl, str(src), str(dst), total)
    parsed["checksum_ok"] = ip_checksum_ok
    return parsed


def parse_tcp(frame: bytes, start: int, src: str, dst: str, total: int | None = None) -> dict:
    if len(frame) < start + 20:
        raise PcapError("TCP header is truncated")
    sport, dport, seq, ack, offset, flags, window, _, _ = struct.unpack(
        "!HHIIBBHHH", frame[start:start + 20]
    )
    data_offset = (offset >> 4) * 4
    if data_offset < 20:
        raise PcapError("TCP data offset is invalid")
    end = len(frame) if total is None else 14 + total
    if start + data_offset > end or end > len(frame):
        raise PcapError("TCP payload is truncated")
    return {
        "type": "tcp",
        "src": src,
        "dst": dst,
        "sport": sport,
        "dport": dport,
        "seq": seq,
        "ack": ack,
        "flags": flags,
        "window": window,
        "payload": frame[start + data_offset:end],
    }


def tcpdump(path: Path, filter_text: str | None = None, verbose: bool = False) -> subprocess.CompletedProcess:
    command = ["tcpdump", "-nn"]
    if verbose:
        command.append("-vv")
    command += ["-r", str(path)]
    if filter_text:
        command.append(filter_text)
    return subprocess.run(command, capture_output=True, text=True, timeout=20)


def tcpdump_count(path: Path, filter_text: str) -> int:
    completed = tcpdump(path, filter_text)
    if completed.returncode != 0:
        raise PcapError(f"tcpdump filter failed: {completed.stderr.strip()}")
    return len([line for line in completed.stdout.splitlines() if line.strip()])


def decoded_tcp(path: Path) -> list[dict]:
    _, frames, _ = read_pcap(path)
    packets = []
    for frame in frames:
        parsed = parse_frame(frame)
        if parsed and parsed["type"] == "tcp":
            packets.append(parsed)
    return packets


def check_handshake() -> str:
    path = K281 / "ipv4-tcp-handshake.pcap"
    packets = decoded_tcp(path)
    assert len(packets) == 8, f"{len(packets)} TCP packets, expected 8"
    flags = [packet["flags"] for packet in packets]
    assert flags == [0x02, 0x12, 0x10, 0x18, 0x10, 0x11, 0x11, 0x10], flags
    assert packets[0]["dport"] == 80 and packets[0]["sport"] == 40000
    assert packets[3]["payload"] == b"GET / HTTP/1.0\r\n\r\n"
    assert tcpdump_count(path, "tcp") == 8
    assert tcpdump_count(path, "tcp port 80") == 8
    assert tcpdump_count(path, "tcp[tcpflags] & tcp-syn != 0") == 2
    assert tcpdump_count(path, "tcp port 9999") == 0
    text = tcpdump(path).stdout
    assert "Flags [S]" in text and "Flags [S.]" in text and "Flags [F.]" in text
    return "8 TCP frames, SYN/SYN-ACK/ACK/GET/FIN order, 4 filters"


def check_ipv6() -> str:
    path = K281 / "ipv6-tcp-http.pcap"
    packets = decoded_tcp(path)
    assert len(packets) == 5, f"{len(packets)} IPv6 TCP packets, expected 5"
    assert packets[0]["flags"] == 0x02 and packets[-1]["flags"] == 0x11
    assert packets[3]["payload"] == b"GET / HTTP/1.0\r\n\r\n"
    assert tcpdump_count(path, "ip6 and tcp") == 5
    return "5 IPv6 TCP frames, HTTP payload and FIN"


def check_k323(scenario: str) -> str:
    path = fixture(scenario)
    packets = decoded_tcp(path)
    data = [
        packet
        for packet in packets
        if packet["sport"] == PORT and packet["payload"]
    ]
    assert data, f"{scenario}: no server data frames"
    assert all(packet["payload"] == PAYLOAD for packet in data), f"{scenario}: payload changed"
    first = data[0]
    assert first["seq"] != 0, f"{scenario}: data sequence is zero"
    retransmits = [packet for packet in data[1:] if packet["seq"] == first["seq"]]
    expected = {
        "single-loss": (2, 1),
        "multiple-loss": (3, None),
        "dup-ack": (2, None),
        "fast-retransmit": (2, None),
        "reordering": (1, 0),
        "rto": (2, 1),
    }
    expected_data, expected_retransmits = expected[scenario]
    assert len(data) == expected_data, f"{scenario}: {len(data)} data frames, expected {expected_data}"
    if expected_retransmits is None:
        assert len(retransmits) >= 1, f"{scenario}: no retransmit"
    else:
        assert len(retransmits) == expected_retransmits, (
            f"{scenario}: {len(retransmits)} retransmits, expected {expected_retransmits}"
        )
    text = tcpdump(path).stdout
    assert "10.0.2.15.23090" in text and "length 44" in text, f"{scenario}: tcpdump fields"
    assert tcpdump_count(path, f"tcp port {PORT}") > 0
    assert tcpdump_count(path, "tcp port 9999") == 0
    return f"data={len(data)} retransmits={len(retransmits)} seq=0x{first['seq']:08x}"


def check_manifest() -> str:
    manifest = K323 / "SHA256SUMS"
    rows = [line.split() for line in manifest.read_text().splitlines() if line.strip()]
    assert len(rows) == 6, f"{len(rows)} K323 pcaps in the manifest, expected 6"
    for digest, name in rows:
        source = K323 / f"{name}.hex"
        assert source.is_file(), f"missing fixture {name}.hex"
        assert sha256_bytes(bytes.fromhex(source.read_text())) == digest, (
            f"hash mismatch for {name}"
        )
    return "6 K323 pcap SHA-256 values match"


def check_malformed() -> str:
    cases = [
        K281 / "malformed-truncated.pcap",
        K281 / "malformed-bad-caplen.pcap",
        K281 / "negative" / "unknown-linktype.pcap",
    ]
    for path in cases:
        try:
            read_pcap(path)
        except PcapError:
            continue
        raise AssertionError(f"malformed pcap decoded: {path}")
    for path in cases[:2]:
        completed = tcpdump(path)
        assert completed.returncode != 0, f"tcpdump accepted malformed {path}"
    bad = K281 / "negative" / "bad-checksum.pcap"
    _, frames, _ = read_pcap(bad)
    parsed = parse_frame(frames[0])
    assert parsed and parsed["type"] == "ipv4" and parsed["checksum_ok"] is False
    verbose = tcpdump(bad, verbose=True)
    assert "bad cksum" in verbose.stdout, "tcpdump did not mark the bad checksum"
    return "truncated/bad-caplen/unknown-linktype rejected; bad checksum visible"


def check_missing_and_mutation() -> str:
    try:
        read_pcap(K323 / "does-not-exist.pcap")
    except PcapError:
        pass
    else:
        raise AssertionError("missing pcap was accepted")
    source = fixture("single-loss")
    with tempfile.TemporaryDirectory() as directory:
        broken = Path(directory) / "single-loss.pcap"
        shutil.copy2(source, broken)
        data = bytearray(broken.read_bytes())
        offset = data.find(PAYLOAD)
        assert offset >= 0, "fixture payload not found in the pcap"
        data[offset] ^= 0xFF
        broken.write_bytes(data)
        try:
            check_k323_file(broken)
        except (AssertionError, PcapError):
            return "missing pcap rejected; mutated payload rejected"
        raise AssertionError("mutated pcap passed the regression")
    return "unreachable"


def check_k323_file(path: Path) -> None:
    """Run the same K323 data assertions against an arbitrary file."""
    packets = decoded_tcp(path)
    data = [packet for packet in packets if packet["sport"] == PORT and packet["payload"]]
    assert all(packet["payload"] == PAYLOAD for packet in data)


CASES = [
    ("handshake", check_handshake),
    ("ipv6", check_ipv6),
    ("single-loss", lambda: check_k323("single-loss")),
    ("multiple-loss", lambda: check_k323("multiple-loss")),
    ("dup-ack", lambda: check_k323("dup-ack")),
    ("fast-retransmit", lambda: check_k323("fast-retransmit")),
    ("reordering", lambda: check_k323("reordering")),
    ("rto", lambda: check_k323("rto")),
    ("manifest", check_manifest),
    ("malformed", check_malformed),
]


def main() -> int:
    if shutil.which("tcpdump") is None:
        print("tcp-pcap: tcpdump is required", file=sys.stderr)
        return 77
    failures = 0
    for name, check in CASES:
        try:
            detail = check()
        except (AssertionError, PcapError, OSError, subprocess.SubprocessError) as error:
            print(f"tcp-pcap: {name} FAIL: {error}", file=sys.stderr)
            failures += 1
        else:
            print(f"tcp-pcap: {name} pass: {detail}")
    try:
        detail = check_missing_and_mutation()
        print(f"tcp-pcap: negative pass: {detail}")
    except (AssertionError, PcapError, OSError) as error:
        print(f"tcp-pcap: negative FAIL: {error}", file=sys.stderr)
        failures += 1
    print(f"tcp-pcap: {len(CASES) + 1 - failures}/{len(CASES) + 1} pass")
    return 1 if failures else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:  # noqa: BLE001 - the suite must fail closed
        print(f"tcp-pcap: ERROR {error}", file=sys.stderr)
        sys.exit(1)
