#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Exact wire assertions for the K290 TCP window scenarios.

The K253 harness performs the live exchange and writes a pcap.  This verifier
reads that pcap without touching the wire and asserts the exact TCP sequence
and byte layout for window scale, the zero-window probe, the reopen and the
persist path.  It never trusts the harness "pass" line alone.
"""

from __future__ import annotations

import argparse
import hashlib
import socket
import struct
import sys
from pathlib import Path

PORT = 23090
PAYLOAD = b"K253-zero-window-44-byte-payload-0123456789!"
assert len(PAYLOAD) == 44

TCP_FIN = 0x01
TCP_SYN = 0x02
TCP_RST = 0x04
TCP_ACK = 0x10


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_pcap(path: Path) -> list[bytes]:
    data = path.read_bytes()
    if len(data) < 24:
        return []
    magic = struct.unpack("<I", data[:4])[0]
    if magic not in (0xA1B2C3D4, 0xD4C3B2A1):
        raise ValueError(f"{path}: not a little-endian pcap")
    frames: list[bytes] = []
    offset = 24
    while offset + 16 <= len(data):
        _, _, captured, _ = struct.unpack("<IIII", data[offset:offset + 16])
        offset += 16
        if offset + captured > len(data):
            break
        frames.append(data[offset:offset + captured])
        offset += captured
    return frames


def ipv4_bytes(text: str) -> bytes:
    return socket.inet_aton(text)


def tcp_frame(frame: bytes) -> dict | None:
    if len(frame) < 14 or struct.unpack("!H", frame[12:14])[0] != 0x0800:
        return None
    if len(frame) < 34 or (frame[14] & 0x0F) * 4 < 20:
        return None
    ihl = (frame[14] & 0x0F) * 4
    if frame[23] != 6 or len(frame) < 14 + ihl + 20:
        return None
    total = struct.unpack("!H", frame[16:18])[0]
    start = 14 + ihl
    sport, dport, seq, ack, offset, flags, window, _, _ = struct.unpack(
        "!HHIIBBHHH", frame[start:start + 20]
    )
    data_offset = (offset >> 4) * 4
    if data_offset < 20 or start + data_offset > 14 + total:
        return None
    options = frame[start + 20:start + data_offset]
    payload = frame[start + data_offset:14 + total]
    return {
        "src_ip": socket.inet_ntoa(frame[26:30]),
        "dst_ip": socket.inet_ntoa(frame[30:34]),
        "sport": sport,
        "dport": dport,
        "seq": seq,
        "ack": ack,
        "flags": flags,
        "window": window,
        "options": options,
        "payload": payload,
    }


def parse_options(options: bytes) -> dict:
    parsed: dict = {}
    index = 0
    while index < len(options):
        kind = options[index]
        if kind == 0:
            break
        if kind == 1:
            index += 1
            continue
        if index + 1 >= len(options):
            break
        length = options[index + 1]
        if length < 2 or index + length > len(options):
            break
        value = options[index + 2:index + length]
        if kind == 2 and len(value) == 2:
            parsed["mss"] = struct.unpack("!H", value)[0]
        elif kind == 3 and len(value) == 1:
            parsed["window_scale"] = value[0]
        elif kind == 4 and not value:
            parsed["sack"] = True
        index += length
    return parsed


def is_client(frame: dict) -> bool:
    return frame["dport"] == PORT and frame["sport"] != PORT


def is_server(frame: dict) -> bool:
    return frame["sport"] == PORT and frame["dport"] != PORT


def handshake(frames: list[dict], index: int = 0) -> tuple[dict, dict, int]:
    """Return the client SYN, server SYN-ACK and its index."""
    seen = -1
    for position, frame in enumerate(frames):
        if is_client(frame) and frame["flags"] & TCP_SYN and not frame["flags"] & TCP_ACK:
            seen += 1
            if seen != index:
                continue
            for follow, candidate in enumerate(frames[position + 1:], position + 1):
                if (
                    is_server(candidate)
                    and candidate["flags"] & TCP_SYN
                    and candidate["flags"] & TCP_ACK
                    and candidate["ack"] == (frame["seq"] + 1) & 0xFFFFFFFF
                ):
                    return frame, candidate, follow
            raise AssertionError(f"handshake {index}: no matching SYN-ACK")
    raise AssertionError(f"handshake {index}: no client SYN")


def check_window_scale(frames: list[dict]) -> str:
    syn, synack, _ = handshake(frames, 0)
    client = parse_options(syn["options"])
    server = parse_options(synack["options"])
    assert client.get("mss") == 1460, f"client MSS {client.get('mss')}"
    assert client.get("window_scale") == 7, f"client WS {client.get('window_scale')}"
    assert client.get("sack"), "client did not offer SACK"
    assert server.get("mss") == 1460, f"server MSS {server.get('mss')}"
    assert "window_scale" in server, "server omitted WS after client offered it"
    assert server["window_scale"] <= 14, f"server WS {server['window_scale']}"
    assert server.get("sack"), "server did not accept SACK"

    plain_syn, plain_synack, _ = handshake(frames, 1)
    plain_client = parse_options(plain_syn["options"])
    plain_server = parse_options(plain_synack["options"])
    assert "window_scale" not in plain_client, "plain client offered WS"
    assert "window_scale" not in plain_server, "server offered WS to a plain client"
    return (
        f"client mss={client['mss']} ws={client['window_scale']} sack={client.get('sack')}; "
        f"server mss={server['mss']} ws={server['window_scale']} sack={server.get('sack')}; "
        f"plain ws={plain_server.get('window_scale', 'absent')}"
    )


def zero_window(frames: list[dict], require_data_probe: bool) -> tuple[str, str]:
    syn, synack, synack_index = handshake(frames, 0)
    server_isn = synack["seq"]
    accept = (server_isn + 1) & 0xFFFFFFFF
    zero = None
    for frame in frames[synack_index + 1:]:
        if (
            is_client(frame)
            and frame["flags"] & TCP_ACK
            and frame["ack"] == accept
            and frame["window"] == 0
        ):
            zero = frame
            break
    assert zero is not None, "client never advertised a zero window"

    position = frames.index(zero)
    probe = None
    probe_kind = ""
    for frame in frames[position + 1:]:
        if not is_server(frame):
            continue
        if frame["seq"] == server_isn and not frame["payload"]:
            probe, probe_kind = frame, "0B-walk"
            break
        if frame["seq"] == accept and len(frame["payload"]) == 1:
            probe, probe_kind = frame, "1B-probe"
            break
    assert probe is not None, "no zero-window probe before the reopen"
    if require_data_probe:
        assert probe_kind == "1B-probe", f"persist probe was {probe_kind}, not a data probe"

    probe_position = frames.index(probe)
    reopen = None
    for frame in frames[probe_position + 1:]:
        if is_client(frame) and frame["flags"] & TCP_ACK and frame["window"] != 0:
            reopen = frame
            break
    assert reopen is not None, "no window reopen after the probe"

    reopen_position = frames.index(reopen)
    probes = [
        frame for frame in frames[position + 1:reopen_position]
        if is_server(frame)
        and (
            (frame["seq"] == server_isn and not frame["payload"])
            or (frame["seq"] == accept and len(frame["payload"]) == 1)
        )
    ]
    assert len(probes) == 1, f"{len(probes)} probes before the reopen; replayed probe"
    data = [
        frame for frame in frames[reopen_position + 1:]
        if is_server(frame) and frame["payload"]
    ]
    assert data, "no data after the reopen"
    assert len(data) == 1, f"{len(data)} data segments after reopen, expected one"
    segment = data[0]
    expected_seq = accept + (1 if probe_kind == "1B-probe" else 0)
    assert segment["seq"] == expected_seq, (
        f"data seq {segment['seq']:#x}, expected {expected_seq:#x}"
    )
    assert probe["payload"] + segment["payload"] == PAYLOAD, (
        "probe plus data bytes differ from the fixture payload"
    )
    assert segment["ack"] == (syn["seq"] + 1) & 0xFFFFFFFF, "data ack is not the client ISN+1"
    assert segment["seq"] not in [frame["seq"] for frame in data[1:]], "replayed data"
    detail = (
        f"probe={probe_kind} seq+{probe['seq'] - server_isn}; "
        f"reopen-win={reopen['window']}; data={len(segment['payload'])}B "
        f"seq+{segment['seq'] - server_isn}; replayed=no"
    )
    return probe_kind, detail


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pcap", type=Path, required=True)
    parser.add_argument(
        "--scenario",
        choices=("window-scale", "zero-window", "reopen", "persist"),
        required=True,
    )
    parser.add_argument("--json", action="store_true")
    parser.add_argument(
        "--strict-probe",
        action="store_true",
        help="require an RFC-style one-byte data probe, not a zero-length walk",
    )
    args = parser.parse_args()
    frames = [parsed for frame in read_pcap(args.pcap) if (parsed := tcp_frame(frame))]
    if not frames:
        print(f"k290-window: {args.pcap}: no TCP frames", file=sys.stderr)
        return 1
    try:
        if args.scenario == "window-scale":
            detail = check_window_scale(frames)
            kind = "window-scale"
        else:
            kind, detail = zero_window(frames, require_data_probe=args.strict_probe)
    except AssertionError as error:
        print(f"k290-window: {args.scenario} FAIL: {error}", file=sys.stderr)
        return 1
    digest = sha256(args.pcap)
    if args.json:
        print(
            '{"scenario": "%s", "result": "pass", "pcap_sha256": "%s", "detail": "%s"}'
            % (args.scenario, digest, detail.replace('"', "'"))
        )
    else:
        print(
            f"k290-window: {args.scenario} pass pcap_sha256={digest} "
            f"probe={kind} {detail}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
