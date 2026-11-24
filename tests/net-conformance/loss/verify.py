#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Offline assertions for the K323 loss/reordering pcaps.

The live runner asserts timing; this verifier re-reads the pcap and asserts the
durable facts: segment counts, repeated sequence numbers and byte-identical
retransmissions.  A missing retransmit or a changed payload fails.
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import sys
from pathlib import Path

PORT = 23090
PAYLOAD = b"K253-zero-window-44-byte-payload-0123456789!"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_frames(path: Path) -> list[bytes]:
    data = path.read_bytes()
    if len(data) < 24:
        return []
    frames = []
    offset = 24
    while offset + 16 <= len(data):
        captured = struct.unpack("<IIII", data[offset:offset + 16])[2]
        offset += 16
        if offset + captured > len(data):
            break
        frames.append(data[offset:offset + captured])
        offset += captured
    return frames


def tcp(frame: bytes) -> dict | None:
    if len(frame) < 54 or struct.unpack("!H", frame[12:14])[0] != 0x0800:
        return None
    if frame[23] != 6:
        return None
    ihl = (frame[14] & 0x0F) * 4
    start = 14 + ihl
    sport, dport, seq, _, offset, _, _, _, _ = struct.unpack(
        "!HHIIBBHHH", frame[start:start + 20]
    )
    data_offset = (offset >> 4) * 4
    total = struct.unpack("!H", frame[16:18])[0]
    payload = frame[start + data_offset:14 + total]
    return {"sport": sport, "dport": dport, "seq": seq, "payload": payload}


def check(scenario: str, path: Path) -> str:
    frames = [item for frame in read_frames(path) if (item := tcp(frame))]
    data = [frame for frame in frames if frame["sport"] == PORT and frame["payload"]]
    assert data, "no server data frames"
    assert all(frame["payload"] == PAYLOAD for frame in data), "payload changed"
    first = data[0]
    assert first["seq"] != 0, "data sequence is zero"
    retransmits = [frame for frame in data[1:] if frame["seq"] == first["seq"]]
    if scenario in ("single-loss", "rto"):
        assert len(data) == 2, f"{len(data)} data frames, expected 2"
        assert len(retransmits) == 1, "retransmit missing or duplicated"
    elif scenario == "multiple-loss":
        assert len(retransmits) >= 2, f"{len(retransmits)} retransmits, expected at least 2"
    elif scenario in ("dup-ack", "fast-retransmit"):
        assert len(retransmits) >= 1, "duplicate ACKs did not produce a retransmit"
    elif scenario == "reordering":
        assert len(data) == 1, f"{len(data)} data frames after a stale ACK, expected 1"
    else:
        raise AssertionError(f"unknown scenario {scenario}")
    return f"data={len(data)} retransmits={len(retransmits)} seq=0x{first['seq']:08x}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--pcap", type=Path, required=True)
    args = parser.parse_args()
    try:
        detail = check(args.scenario, args.pcap)
    except AssertionError as error:
        print(f"k323-loss: {args.scenario} FAIL: {error}", file=sys.stderr)
        return 1
    print(
        f"k323-loss: {args.scenario} pass pcap_sha256={sha256(args.pcap)} {detail}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
