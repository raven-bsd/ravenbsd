#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K253 packet-level TCP conformance harness (tap target + Linux baseline).

The tested connection never uses the host kernel TCP stack for the peer: the
peer is a raw AF_PACKET client that builds Ethernet/IPv4/TCP frames itself.
The Linux baseline runs the same raw client against a host-kernel server in a
disposable network namespace; the Raven target runs it against a guest server
over the K227 tap (`10.0.2.15`).  Every emitted and received frame is written to
one pcap file, and the target run keeps the guest serial log.

Run through `scripts/net-conformance.sh`, which provides the tap/namespace
plumbing.  The harness deliberately refuses to fake a pass: each scenario makes
a wire-level assertion and returns a non-zero status when it is not met.
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import random
import re
import select
import signal
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path

PAYLOAD = b"K253-zero-window-44-byte-payload-0123456789!"
assert len(PAYLOAD) == 44

ETH_P_ALL = 0x0003
ETH_IPV4 = 0x0800
ETH_ARP = 0x0806
TCP_FIN = 0x01
TCP_SYN = 0x02
TCP_RST = 0x04
TCP_PSH = 0x08
TCP_ACK = 0x10
TCP_URG = 0x20
TCP_ECE = 0x40
TCP_CWR = 0x80
IP_ECN_CE = 0x03

RAVEN_GUEST = "10.0.2.15"
RAVEN_HOST = "10.0.2.2"
RAVEN_PORT = 23090
RAVEN_UDP_PORT = 23091
RAVEN_GUEST_MAC = "52:54:00:12:34:56"
DEFAULT_TAP = "nctap0"

LINUX_HOST = "10.99.0.1"
LINUX_GUEST = "10.99.0.2"
LINUX_PORT = 23090
LINUX_IF_A = "ncc0"
LINUX_IF_B = "ncc1"
LINUX_IF_A_MAC = "02:00:00:00:99:00"
LINUX_IF_B_MAC = "02:00:00:00:99:01"


def log(message: str) -> None:
    print(f"net-conformance: {message}", flush=True)


def checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\x00"
    total = 0
    for index in range(0, len(data), 2):
        total += (data[index] << 8) | data[index + 1]
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def ip_bytes(address: str) -> bytes:
    return socket.inet_aton(address)


def mac_bytes(mac: str) -> bytes:
    return bytes(int(part, 16) for part in mac.split(":"))


def mac_text(raw: bytes) -> str:
    return ":".join(f"{byte:02x}" for byte in raw)


def interface_mac(interface: str) -> str:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        info = fcntl.ioctl(
            sock.fileno(), 0x8927, struct.pack("256s", interface.encode()[:15])
        )
    finally:
        sock.close()
    return mac_text(info[18:24])


def tcp_options(mss: int = 1460, window_scale: int | None = 7, sack: bool = True) -> bytes:
    options = bytearray()
    options += struct.pack("!BBH", 2, 4, mss)
    if window_scale is not None:
        options += struct.pack("!BBB", 3, 3, window_scale)
    if sack:
        options += b"\x04\x02"
    options += b"\x01\x01"
    while len(options) % 4:
        options += b"\x00"
    return bytes(options)


def parse_tcp_options(options: bytes) -> dict:
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


def build_frame(
    src_mac: str,
    dst_mac: str,
    src_ip: str,
    dst_ip: str,
    src_port: int,
    dst_port: int,
    seq: int,
    ack: int,
    flags: int,
    window: int,
    options: bytes = b"",
    payload: bytes = b"",
    ident: int = 0,
    tos: int = 0,
) -> bytes:
    tcp_length = 20 + len(options) + len(payload)
    offset = ((20 + len(options)) // 4) << 4
    tcp = struct.pack(
        "!HHIIBBHHH",
        src_port,
        dst_port,
        seq & 0xFFFFFFFF,
        ack & 0xFFFFFFFF,
        offset,
        flags,
        window & 0xFFFF,
        0,
        0,
    ) + options + payload
    pseudo = ip_bytes(src_ip) + ip_bytes(dst_ip) + struct.pack("!BBH", 0, 6, tcp_length)
    tcp = tcp[:16] + struct.pack("!H", checksum(pseudo + tcp)) + tcp[18:]
    total = 20 + tcp_length
    header = struct.pack(
        "!BBHHHBBH4s4s",
        0x45,
        tos & 0xFF,
        total,
        ident & 0xFFFF,
        0x4000,
        64,
        6,
        0,
        ip_bytes(src_ip),
        ip_bytes(dst_ip),
    )
    header = header[:10] + struct.pack("!H", checksum(header)) + header[12:]
    return mac_bytes(dst_mac) + mac_bytes(src_mac) + struct.pack("!H", ETH_IPV4) + header + tcp


def ipv4_packet(
    src_ip: str,
    dst_ip: str,
    protocol: int,
    payload: bytes,
    ident: int = 0,
    flags: int = 0,
    frag_offset: int = 0,
    options: bytes = b"",
    tos: int = 0,
    ihl_override: int | None = None,
    total_override: int | None = None,
) -> bytes:
    """Build one IPv4 packet without the Ethernet header."""
    if len(options) % 4:
        options += b"\x00" * (4 - len(options) % 4)
    header_len = 20 + len(options)
    ihl = 5 + len(options) // 4 if ihl_override is None else ihl_override
    total = header_len + len(payload) if total_override is None else total_override
    flags_fragment = ((flags & 0x7) << 13) | ((frag_offset // 8) & 0x1FFF)
    header = struct.pack(
        "!BBHHHBBH4s4s",
        0x40 | (ihl & 0x0F),
        tos & 0xFF,
        total & 0xFFFF,
        ident & 0xFFFF,
        flags_fragment,
        64,
        protocol,
        0,
        ip_bytes(src_ip),
        ip_bytes(dst_ip),
    ) + options
    header = header[:10] + struct.pack("!H", checksum(header)) + header[12:]
    return header + payload


def build_ipv4_frame(
    src_mac: str,
    dst_mac: str,
    src_ip: str,
    dst_ip: str,
    protocol: int,
    payload: bytes,
    ident: int = 0,
    flags: int = 0,
    frag_offset: int = 0,
    options: bytes = b"",
    tos: int = 0,
    ihl_override: int | None = None,
    total_override: int | None = None,
) -> bytes:
    packet = ipv4_packet(
        src_ip, dst_ip, protocol, payload, ident, flags, frag_offset, options, tos,
        ihl_override, total_override,
    )
    return (
        mac_bytes(dst_mac) + mac_bytes(src_mac)
        + struct.pack("!H", ETH_IPV4) + packet
    )


def build_udp(
    src_ip: str,
    dst_ip: str,
    src_port: int,
    dst_port: int,
    payload: bytes,
    bad_checksum: bool = False,
    length_override: int | None = None,
) -> bytes:
    length = 8 + len(payload) if length_override is None else length_override
    segment = struct.pack("!HHHH", src_port, dst_port, length & 0xFFFF, 0) + payload
    pseudo = ip_bytes(src_ip) + ip_bytes(dst_ip) + struct.pack("!BBH", 0, 17, len(segment))
    value = checksum(pseudo + segment)
    if value == 0:
        value = 0xFFFF
    if bad_checksum:
        value ^= 0xFFFF
    return segment[:6] + struct.pack("!H", value) + segment[8:]


def build_icmp(
    message_type: int,
    code: int,
    rest: int,
    payload: bytes,
    bad_checksum: bool = False,
    truncate: int | None = None,
) -> bytes:
    message = struct.pack("!BBHI", message_type, code, 0, rest) + payload
    value = checksum(message)
    if bad_checksum:
        value ^= 0xFFFF
    message = message[:2] + struct.pack("!H", value) + message[4:]
    return message if truncate is None else message[:truncate]


def udp_fragments(datagram: bytes, chunk: int) -> list[tuple[int, bool, bytes]]:
    """Split an IP payload into 8-byte-aligned fragments."""
    if chunk <= 0 or chunk % 8:
        raise ValueError("fragment chunk must be a positive multiple of 8")
    fragments = []
    offset = 0
    while offset < len(datagram):
        end = min(offset + chunk, len(datagram))
        more = end < len(datagram)
        fragments.append((offset, more, datagram[offset:end]))
        offset = end
    return fragments


def parse_frame(frame: bytes) -> dict | None:
    if len(frame) < 14:
        return None
    ethertype = struct.unpack("!H", frame[12:14])[0]
    if ethertype == ETH_ARP:
        if len(frame) < 42:
            return None
        return {
            "type": "arp",
            "op": struct.unpack("!H", frame[20:22])[0],
            "sender_mac": frame[22:28],
            "sender_ip": socket.inet_ntoa(frame[28:32]),
            "target_mac": frame[32:38],
            "target_ip": socket.inet_ntoa(frame[38:42]),
        }
    if ethertype != ETH_IPV4 or len(frame) < 34:
        return None
    version_ihl = frame[14]
    ihl = (version_ihl & 0x0F) * 4
    if ihl < 20 or len(frame) < 14 + ihl:
        return None
    total = struct.unpack("!H", frame[16:18])[0]
    protocol = frame[23]
    src_ip = socket.inet_ntoa(frame[26:30])
    dst_ip = socket.inet_ntoa(frame[30:34])
    flags_fragment = struct.unpack("!H", frame[20:22])[0]
    base = {
        "type": "ipv4",
        "protocol": protocol,
        "src_ip": src_ip,
        "dst_ip": dst_ip,
        "ihl": ihl,
        "total_len": total,
        "id": struct.unpack("!H", frame[18:20])[0],
        "flags": (flags_fragment >> 13) & 0x7,
        "frag_offset": (flags_fragment & 0x1FFF) * 8,
        "options": frame[34:14 + ihl],
        "payload": frame[14 + ihl:14 + total],
    }
    if protocol == 17:
        udp_start = 14 + ihl
        if len(frame) < udp_start + 8:
            return base
        sport, dport, length, checksum = struct.unpack("!HHHH", frame[udp_start:udp_start + 8])
        return {
            **base,
            "type": "udp",
            "sport": sport,
            "dport": dport,
            "length": length,
            "checksum": checksum,
            "payload": frame[udp_start + 8:14 + total],
        }
    if protocol == 1:
        icmp_start = 14 + ihl
        if len(frame) < icmp_start + 8:
            return base
        message_type, code, checksum, rest = struct.unpack(
            "!BBHI", frame[icmp_start:icmp_start + 8]
        )
        return {
            **base,
            "type": "icmp",
            "message_type": message_type,
            "code": code,
            "checksum": checksum,
            "rest": rest,
            "payload": frame[icmp_start + 8:14 + total],
        }
    if protocol != 6:
        return base
    tcp_start = 14 + ihl
    if len(frame) < tcp_start + 20:
        return base
    sport, dport, seq, ack, offset, flags, window, _, _ = struct.unpack(
        "!HHIIBBHHH", frame[tcp_start:tcp_start + 20]
    )
    data_offset = (offset >> 4) * 4
    if data_offset < 20:
        return None
    options = frame[tcp_start + 20:tcp_start + data_offset]
    payload = frame[tcp_start + data_offset:14 + total]
    return {
        "type": "tcp",
        "src_ip": src_ip,
        "dst_ip": dst_ip,
        "tos": frame[15],
        "sport": sport,
        "dport": dport,
        "seq": seq,
        "ack": ack,
        "flags": flags,
        "window": window,
        "options": options,
        "payload": payload,
    }


class PcapWriter:
    def __init__(self, path: Path):
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)
        self.stream = open(path, "wb")
        self.stream.write(struct.pack("<IHHiIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, 1))

    def write(self, frame: bytes) -> None:
        now = time.time()
        self.stream.write(
            struct.pack("<IIII", int(now), int((now % 1) * 1_000_000), len(frame), len(frame))
        )
        self.stream.write(frame)
        self.stream.flush()

    def close(self) -> None:
        self.stream.close()


class RawPeer:
    def __init__(self, interface: str, host_ip: str, guest_ip: str, guest_mac: str,
                 pcap_path: Path | None):
        self.interface = interface
        self.host_ip = host_ip
        self.guest_ip = guest_ip
        self.guest_mac = guest_mac
        self.mac = interface_mac(interface)
        self.pcap = PcapWriter(pcap_path) if pcap_path else None
        self.sock = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(ETH_P_ALL))
        self.sock.bind((interface, 0))
        self.sock.setblocking(False)
        self.our_port = 0
        self.peer_port = 0

    def close(self) -> None:
        self.sock.close()
        if self.pcap:
            self.pcap.close()

    def send(self, src_port: int, dst_port: int, seq: int, ack: int, flags: int,
             window: int, options: bytes = b"", payload: bytes = b"",
             tos: int = 0) -> None:
        frame = build_frame(
            self.mac,
            self.guest_mac,
            self.host_ip,
            self.guest_ip,
            src_port,
            dst_port,
            seq,
            ack,
            flags,
            window,
            options,
            payload,
            ident=random.randrange(0xFFFF),
            tos=tos,
        )
        self.sock.send(frame)
        if self.pcap:
            self.pcap.write(frame)

    def send_raw(self, frame: bytes) -> None:
        self.sock.send(frame)
        if self.pcap:
            self.pcap.write(frame)

    def _arp(self, parsed: dict) -> None:
        if parsed["op"] != 1 or parsed["target_ip"] != self.host_ip:
            return
        sender_mac = parsed["sender_mac"]
        reply = (
            sender_mac + mac_bytes(self.mac) +
            struct.pack("!H", ETH_ARP) +
            struct.pack("!HHBBH", 1, ETH_IPV4, 6, 4, 2) +
            mac_bytes(self.mac) + ip_bytes(self.host_ip) +
            sender_mac + ip_bytes(parsed["sender_ip"])
        )
        self.send_raw(reply)

    def recv(self, timeout: float) -> dict | None:
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            ready, _, _ = select.select([self.sock], [], [], remaining)
            if not ready:
                return None
            frame = self.sock.recv(65535)
            if self.pcap:
                self.pcap.write(frame)
            parsed = parse_frame(frame)
            if not parsed:
                continue
            if parsed["type"] == "arp":
                self._arp(parsed)
                continue
            if parsed["type"] != "tcp" or parsed["src_ip"] != self.guest_ip:
                continue
            if parsed["dport"] != self.our_port or parsed["sport"] != self.peer_port:
                continue
            return parsed

    def wait(self, timeout: float, predicate) -> dict | None:
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            parsed = self.recv(remaining)
            if parsed is not None and predicate(parsed):
                return parsed

    def recv_l4(self, timeout: float, predicate=None) -> dict | None:
        """Read any guest IP frame (UDP, ICMP or TCP) for the L4 fuzz rows."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            ready, _, _ = select.select([self.sock], [], [], remaining)
            if not ready:
                return None
            frame = self.sock.recv(65535)
            if self.pcap:
                self.pcap.write(frame)
            parsed = parse_frame(frame)
            if not parsed:
                continue
            if parsed.get("type") == "arp":
                self._arp(parsed)
                continue
            if parsed.get("src_ip") != self.guest_ip:
                continue
            if predicate is not None and not predicate(parsed):
                continue
            return parsed
        return None


def handshake(peer: RawPeer, local_port: int, remote_port: int, final_window: int,
              window_scale: int | None = 7, ecn: bool = False, mss: int = 1460,
              isn: int | None = None) -> dict:
    peer.our_port = local_port
    peer.peer_port = remote_port
    if isn is None:
        isn = random.randrange(1, 0x7FFFFFFF)
    isn &= 0xFFFFFFFF
    syn_flags = TCP_SYN | ((TCP_ECE | TCP_CWR) if ecn else 0)
    peer.send(local_port, remote_port, isn, 0, syn_flags, 65535,
              tcp_options(mss, window_scale), tos=0x02 if ecn else 0)
    synack = peer.wait(
        5.0, lambda p: p["flags"] & TCP_SYN and p["flags"] & TCP_ACK and p["ack"] == isn + 1
    )
    if synack is None:
        raise RuntimeError("no SYN-ACK")
    peer.send(local_port, remote_port, isn + 1, synack["seq"] + 1, TCP_ACK, final_window)
    return {
        "isn": isn,
        "server_isn": synack["seq"],
        "options": parse_tcp_options(synack["options"]),
        "synack": synack,
        "window_scale": window_scale,
        "ecn": ecn,
    }


def ack(peer: RawPeer, state: dict, local_port: int, remote_port: int, next_seq: int,
        window: int) -> None:
    peer.send(local_port, remote_port, state["isn"] + 1, next_seq, TCP_ACK, window)


def collect_stream(peer: RawPeer, state: dict, local_port: int, remote_port: int,
                   timeout: float, window: int,
                   start_seq: int | None = None,
                   stop_after: int | None = None) -> tuple[bytes, list[dict], int, bool]:
    """Read the server byte stream in order, ACKing each in-order segment.

    Returns the reassembled bytes, the frames, the next expected sequence and a
    flag that a payload byte below ``start_seq`` was retransmitted.  The flag is
    only meaningful when the caller accepted a zero-window probe byte: a sender
    must resume at that byte, not replay it.
    """
    received = bytearray()
    frames: list[dict] = []
    next_seq = state["server_isn"] + 1 if start_seq is None else start_seq
    replayed = False
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        parsed = peer.recv(min(0.25, max(0.0, deadline - time.monotonic())))
        if parsed is None:
            continue
        flags = parsed["flags"]
        if flags & (TCP_RST | TCP_FIN):
            frames.append(parsed)
            break
        if parsed["payload"]:
            frames.append(parsed)
            if parsed["seq"] < next_seq:
                replayed = True
            if parsed["seq"] <= next_seq < parsed["seq"] + len(parsed["payload"]):
                offset = next_seq - parsed["seq"]
                received += parsed["payload"][offset:]
                next_seq += len(parsed["payload"]) - offset
                ack(peer, state, local_port, remote_port, next_seq, window)
                if stop_after is not None and len(received) >= stop_after:
                    break
    return bytes(received), frames, next_seq, replayed


def reset(peer: RawPeer, state: dict, local_port: int, remote_port: int,
          next_seq: int | None = None) -> None:
    if next_seq is None:
        next_seq = state["isn"] + 1
    peer.send(local_port, remote_port, next_seq, state["server_isn"] + 1,
              TCP_RST | TCP_ACK, 0)


def scenario_handshake(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 65535, window_scale=7)
    options = state["options"]
    ws_ok = "window_scale" in options
    reset(peer, state, local_port, remote_port)

    # A peer that did not offer window scaling must not be offered one.
    plain_port = local_port + 1
    plain = handshake(peer, plain_port, remote_port, 65535, window_scale=None)
    plain_options = plain["options"]
    plain_ok = "window_scale" not in plain_options and plain["synack"]["window"] <= 65535
    reset(peer, plain, plain_port, remote_port)

    ok = options.get("mss", 0) > 0 and options.get("sack", False) and ws_ok and plain_ok
    detail = (
        f"SYN-ACK mss={options.get('mss')} ws={options.get('window_scale')} "
        f"sack={options.get('sack', False)}; plain-peer ws="
        f"{plain_options.get('window_scale', 'absent')} win={plain['synack']['window']}"
    )
    return ok, detail


def probe_once(peer: RawPeer, state: dict, local_port: int, remote_port: int,
               timeout: float) -> dict | None:
    """Wait for the server's zero-window probe without accepting new data.

    Linux emits a zero-length window probe at ``snd_una - 1``; a stack that
    sends the RFC-style one-byte data probe is accepted as well.  The probe's
    byte is not accepted while the advertised window is zero.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        parsed = peer.recv(min(0.25, max(0.0, deadline - time.monotonic())))
        if parsed is None:
            continue
        if parsed["flags"] & TCP_RST:
            return None
        is_one_byte = len(parsed["payload"]) == 1 and parsed["seq"] == state["server_isn"] + 1
        is_walk = not parsed["payload"] and parsed["seq"] == state["server_isn"]
        if is_one_byte or is_walk:
            ack(peer, state, local_port, remote_port, state["server_isn"] + 1, 0)
            return parsed
    return None


def scenario_zero_window(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 0)
    probe = probe_once(peer, state, local_port, remote_port, 4.0)
    if probe is None:
        return False, "no persist/window probe while the peer window was zero"
    one_byte = len(probe["payload"]) == 1
    accept_next = state["server_isn"] + (2 if one_byte else 1)
    # Accept the probe byte (if any) and reopen the window; the rest must flush.
    ack(peer, state, local_port, remote_port, accept_next, 65535)
    data, frames, _, replayed = collect_stream(
        peer, state, local_port, remote_port, 4.0, 65535, start_seq=accept_next
    )
    total = probe["payload"] + data
    detail = (
        f"probe={'1B' if one_byte else '0B-walk'} seq={probe['seq'] - state['server_isn']}; "
        f"after-reopen={len(data)}B; total={len(total)}/44; frames={len(frames)}; "
        f"replayed-probe={'yes' if replayed else 'no'}"
    )
    reset(peer, state, local_port, remote_port)
    return total == PAYLOAD and not replayed, detail


def scenario_window_update(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 0)
    # Let the zero-window ACK settle, then send a stale zero followed by the reopen.
    time.sleep(0.4)
    ack(peer, state, local_port, remote_port, state["server_isn"] + 1, 0)
    ack(peer, state, local_port, remote_port, state["server_isn"] + 1, 32768)
    data, frames, _, _ = collect_stream(peer, state, local_port, remote_port, 4.0, 32768)
    reset(peer, state, local_port, remote_port)
    return data == PAYLOAD, f"delivered={len(data)}/44 frames={len(frames)}"


def scenario_retransmit(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 65535)
    first = peer.wait(4.0, lambda p: bool(p["payload"]))
    if first is None:
        return False, "no data segment to retransmit"
    # Do not ACK: force an RTO retransmission of the same sequence and bytes.
    again = peer.wait(4.0, lambda p: bool(p["payload"]) and p["seq"] == first["seq"])
    ok = (
        again is not None
        and again["payload"] == first["payload"]
        and len(first["payload"]) == len(PAYLOAD)
    )
    ack(peer, state, local_port, remote_port, first["seq"] + len(first["payload"]), 65535)
    reset(peer, state, local_port, remote_port)
    detail = (
        f"first={len(first['payload'])}B seq+{first['seq'] - state['server_isn'] - 1}; "
        f"retransmit={'yes' if again else 'no'}"
    )
    return ok, detail


def scenario_fin_rst(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 65535)
    data, frames, next_seq, _ = collect_stream(
        peer, state, local_port, remote_port, 3.0, 65535
    )
    peer.send(local_port, remote_port, state["isn"] + 1, next_seq, TCP_FIN | TCP_ACK, 65535)
    fin = peer.wait(
        4.0, lambda p: bool(p["flags"] & (TCP_FIN | TCP_RST))
    )
    reset(peer, state, local_port, remote_port)
    ok = data == PAYLOAD and fin is not None and bool(fin["flags"] & TCP_FIN)
    detail = (
        f"delivered={len(data)}/44; "
        f"peer-close={'FIN' if fin and fin['flags'] & TCP_FIN else 'RST' if fin else 'none'}"
    )
    return ok, detail


def scenario_ecn(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    state = handshake(peer, local_port, remote_port, 65535, ecn=True)
    negotiated = bool(state["synack"]["flags"] & TCP_ECE)
    if not negotiated:
        reset(peer, state, local_port, remote_port)
        return False, "SYN-ACK did not echo ECE; ECN was not negotiated"
    peer.send(local_port, remote_port, state["isn"] + 1, state["server_isn"] + 1,
              TCP_ACK | TCP_PSH, 65535, payload=b"ecn", tos=IP_ECN_CE)
    echoed = peer.wait(3.0, lambda p: bool(p["flags"] & TCP_ECE) and not p["payload"])
    reset(peer, state, local_port, remote_port)
    if echoed is None:
        return False, "CE-marked data did not produce an ECE ACK"
    return True, "ECN negotiated; CE data echoed as ECE"


# K548 network fuzz.  The case table is generated from a fixed seed, and the FNV-1a hash of the
# canonical table is printed and asserted, so a gate run cannot silently shrink or reorder it.
FUZZ_SEED = 0x4B35_3438_4655_5A5A
FUZZ_CASES = 224
FUZZ_TABLE_HASH = 0x3006_6A6F_544F_F0BC
TCP_MINMSS = 536
FNV64_OFFSET = 0xCBF2_9CE4_8422_2325
FNV64_PRIME = 0x0000_0100_0000_01B3

FUZZ_RESERVED = (
    (TCP_SYN, 0, 0, 65535, b"", "challenge"),
    (TCP_SYN | TCP_FIN, 0, 0, 65535, b"", "challenge"),
    (TCP_SYN | TCP_PSH | TCP_URG, 0, 0, 8192, b"", "challenge"),
    (TCP_SYN | TCP_ECE | TCP_CWR, 0, 0, 65535, b"", "challenge"),
    (TCP_SYN | TCP_ACK, 0, 0, 65535, b"", "challenge"),
    (TCP_RST, 0, 0, 0, b"", "close"),
    (TCP_FIN | TCP_RST, 0, 0, 0, b"", "close"),
    (TCP_RST, 100, 0, 0, b"", "challenge"),
    (TCP_RST, 1_000_000, 0, 0, b"", "ignore"),
    (TCP_ACK, 0, 1_000_000, 65535, b"", "challenge"),
    (TCP_PSH | TCP_ACK, 0, 0, 65535, b"FZZ1", "data-ack"),
    (TCP_ECE | TCP_CWR | TCP_ACK, 0, 0, 65535, b"FZZ2", "data-ack"),
)


def fnv64_update(value: int, data: bytes) -> int:
    for byte in data:
        value ^= byte
        value = (value * FNV64_PRIME) & 0xFFFFFFFFFFFFFFFF
    return value


def fuzz_lcg(seed: int, count: int) -> list[int]:
    value = seed & 0xFFFFFFFFFFFFFFFF
    numbers = []
    for _ in range(count):
        value = (value * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        numbers.append(value)
    return numbers


def fuzz_table(seed: int) -> list[dict]:
    numbers = fuzz_lcg(seed, FUZZ_CASES)
    cases: list[dict] = []

    def add(group: str, index: int, **fields) -> None:
        port = 42000 + len(cases)
        case = {
            "group": group,
            "index": index,
            "port": port,
            "isn": (0x10000000 + (numbers[len(cases)] & 0x0FFFFFFF)),
        }
        case.update(fields)
        cases.append(case)

    for index in range(48):
        variant = index % len(FUZZ_RESERVED)
        flags, seq_offset, ack_offset, window, payload, expect = FUZZ_RESERVED[variant]
        add("reserved", index, variant=variant, flags=flags, seq_offset=seq_offset,
            ack_offset=ack_offset, window=window, payload=payload.hex(), expect=expect)
    for index in range(48):
        add("out-of-window", index, seq_offset=1_000_000 + (numbers[len(cases)] % 1_000_000),
            window=65535, payload=b"past-window".hex())
    for index in range(32):
        windows = (536, 1024, 1460, 4096, 16384, 32768, 65535, 8192)
        add("zero-window", index, reopen=windows[index % len(windows)])
    for index in range(40):
        mss_values = (1, 64, 128, 256, 535, 536, 537, 1000, 1460, 2000)
        add("tiny-mss", index, mss=mss_values[index % len(mss_values)])
    for index in range(40):
        add("duplicate-fin", index, window=(4096, 16384, 65535, 8192)[index % 4])
    for index in range(16):
        add("rst-after-close", index, rst_offset=(1, 2, 100, 1_000_000)[index % 4])

    if len(cases) != FUZZ_CASES:
        raise RuntimeError(f"fuzz table has {len(cases)} cases, expected {FUZZ_CASES}")
    return cases


def fuzz_table_hash(cases: list[dict]) -> int:
    value = FNV64_OFFSET
    for case in cases:
        encoded = json.dumps(case, sort_keys=True, separators=(",", ":")).encode()
        value = fnv64_update(value, encoded)
    return value


def fuzz_quiet(peer: RawPeer, timeout: float, allow=None) -> dict | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = peer.recv(min(0.1, max(0.0, deadline - time.monotonic())))
        if frame is None:
            continue
        if allow is not None and allow(frame):
            continue
        return frame
    return None


def fuzz_payload(peer: RawPeer, timeout: float) -> dict | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = peer.recv(min(0.1, max(0.0, deadline - time.monotonic())))
        if frame is not None and frame["payload"]:
            return frame
    return None


def scenario_fuzz(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    cases = fuzz_table(FUZZ_SEED)
    table_hash = fuzz_table_hash(cases)
    mutations = set(filter(None, os.environ.get("NETCONF_FUZZ_MUTATION", "").split(",")))
    mutated = "all" in mutations or "tcp" in mutations
    deviations = 0

    def check(case: dict, name: str, passed: bool, detail: str) -> None:
        nonlocal deviations
        if passed:
            return
        deviations += 1
        log(f"deviation case={case['group']}[{case['index']}] check={name} {detail}")

    log(f"fuzz: start seed=0x{FUZZ_SEED:016x} cases={len(cases)} "
        f"table-hash=0x{table_hash:016x}")
    group_started = time.monotonic()
    group_cases = 0
    previous_group = cases[0]["group"]

    for case in cases:
        port = case["port"]
        group = case["group"]
        isn = case["isn"]
        if group != previous_group:
            log(f"fuzz: group={previous_group} cases={group_cases} "
                f"elapsed={time.monotonic() - group_started:.2f}s")
            group_started = time.monotonic()
            group_cases = 0
            previous_group = group
        group_cases += 1
        case_started = time.monotonic()
        try:
            if group == "reserved":
                state = handshake(peer, port, remote_port, 65535, window_scale=7, isn=isn)
                data, _frames, next_seq, _ = collect_stream(
                    peer, state, port, remote_port, 1.0, 65535,
                    stop_after=len(PAYLOAD))
                check(case, "setup-data", data == PAYLOAD, f"delivered={len(data)}/44")
                payload = bytes.fromhex(case["payload"])
                seq = (isn + 1 + case["seq_offset"]) & 0xFFFFFFFF
                ack = (next_seq + case["ack_offset"]) & 0xFFFFFFFF
                peer.send(port, remote_port, seq, ack, case["flags"], case["window"],
                          payload=payload)
                if case["expect"] == "challenge":
                    reply = peer.wait(
                        1.0, lambda p: bool(p["flags"] & TCP_ACK)
                        and not p["flags"] & (TCP_SYN | TCP_RST))
                    expected_ack = isn + (2 if mutated else 1)
                    check(case, "challenge-ack",
                          reply is not None and reply["ack"] == expected_ack,
                          f"reply={None if reply is None else hex(reply['flags'])} "
                          f"ack={None if reply is None else reply['ack']}")
                    reset(peer, state, port, remote_port)
                elif case["expect"] == "data-ack":
                    reply = peer.wait(
                        1.0, lambda p: bool(p["flags"] & TCP_ACK)
                        and p["ack"] == (isn + 1 + len(payload)) & 0xFFFFFFFF)
                    check(case, "data-ack", reply is not None,
                          f"reply={None if reply is None else hex(reply['flags'])}")
                    reset(peer, state, port, remote_port)
                elif case["expect"] == "ignore":
                    reply = fuzz_quiet(peer, 0.2)
                    check(case, "rst-out-of-window-ignored", reply is None,
                          f"unexpected={None if reply is None else hex(reply['flags'])}")
                    reset(peer, state, port, remote_port)
                else:
                    reply = fuzz_quiet(peer, 0.2)
                    check(case, "reset-not-answered", reply is None,
                          f"unexpected={None if reply is None else hex(reply['flags'])}")
            elif group == "out-of-window":
                state = handshake(peer, port, remote_port, 65535, window_scale=7, isn=isn)
                data, _frames, _next_seq, _ = collect_stream(
                    peer, state, port, remote_port, 1.0, 65535,
                    stop_after=len(PAYLOAD))
                check(case, "setup-data", data == PAYLOAD, f"delivered={len(data)}/44")
                seq = (isn + 1 + case["seq_offset"]) & 0xFFFFFFFF
                peer.send(port, remote_port, seq, state["server_isn"] + 1,
                          TCP_ACK, case["window"], payload=b"past-window")
                reply = peer.wait(
                    1.0, lambda p: bool(p["flags"] & TCP_ACK)
                    and not p["flags"] & (TCP_SYN | TCP_RST) and not p["payload"])
                check(case, "past-window-ack",
                      reply is not None and reply["ack"] == isn + 1,
                      f"reply={None if reply is None else hex(reply['flags'])} "
                      f"ack={None if reply is None else reply['ack']}")
                reset(peer, state, port, remote_port)
            elif group == "zero-window":
                state = handshake(peer, port, remote_port, 0, window_scale=7, isn=isn)
                early = fuzz_payload(peer, 0.15)
                check(case, "zero-window-stall", early is None,
                      f"unexpected-payload={0 if early is None else len(early['payload'])}")
                peer.send(port, remote_port, isn + 1, state["server_isn"] + 1,
                          TCP_ACK, case["reopen"])
                data, _frames, _next_seq, _ = collect_stream(
                    peer, state, port, remote_port, 1.5, case["reopen"],
                    stop_after=len(PAYLOAD))
                check(case, "zero-window-reopen", data == PAYLOAD,
                      f"delivered={len(data)}/44 window={case['reopen']}")
                reset(peer, state, port, remote_port)
            elif group == "tiny-mss":
                state = handshake(peer, port, remote_port, 65535, window_scale=7,
                                  mss=case["mss"], isn=isn)
                data, frames, _next_seq, _ = collect_stream(
                    peer, state, port, remote_port, 1.5, 65535,
                    stop_after=len(PAYLOAD))
                bound = 1460 if case["mss"] >= TCP_MINMSS else TCP_MINMSS
                largest = max((len(frame["payload"]) for frame in frames if frame["payload"]),
                              default=0)
                check(case, "tiny-mss-data",
                      data == PAYLOAD and largest <= bound,
                      f"delivered={len(data)}/44 largest={largest} bound={bound} "
                      f"peer-mss={case['mss']}")
                reset(peer, state, port, remote_port)
            elif group == "duplicate-fin":
                state = handshake(peer, port, remote_port, 65535, window_scale=7, isn=isn)
                fin_seq = isn + 1
                for _ in range(2):
                    peer.send(port, remote_port, fin_seq, state["server_isn"] + 1,
                              TCP_FIN | TCP_ACK, case["window"])
                delivered = bytearray()
                acks = []
                saw_reset = False
                deadline = time.monotonic() + 0.7
                while time.monotonic() < deadline:
                    frame = peer.recv(min(0.1, max(0.0, deadline - time.monotonic())))
                    if frame is None:
                        continue
                    if frame["flags"] & TCP_RST:
                        saw_reset = True
                        break
                    if frame["payload"]:
                        delivered += frame["payload"]
                        peer.send(port, remote_port, isn + 2,
                                  (frame["seq"] + len(frame["payload"])) & 0xFFFFFFFF,
                                  TCP_ACK, case["window"])
                    if frame["flags"] & TCP_FIN:
                        peer.send(port, remote_port, isn + 2, frame["seq"] + 1,
                                  TCP_ACK, case["window"])
                    if frame["flags"] & TCP_ACK:
                        acks.append(frame["ack"])
                    if len(delivered) >= len(PAYLOAD) and acks.count(isn + 2) >= 2:
                        break
                check(case, "duplicate-fin-ack",
                      bytes(delivered) == PAYLOAD and acks.count(isn + 2) >= 2
                      and not saw_reset,
                      f"delivered={len(delivered)}/44 acks={acks.count(isn + 2)} "
                      f"rst={saw_reset}")
                reset(peer, state, port, remote_port)
            elif group == "rst-after-close":
                state = handshake(peer, port, remote_port, 65535, window_scale=7, isn=isn)
                data, _frames, next_seq, _ = collect_stream(
                    peer, state, port, remote_port, 1.0, 65535,
                    stop_after=len(PAYLOAD))
                check(case, "setup-data", data == PAYLOAD, f"delivered={len(data)}/44")
                peer.send(port, remote_port, isn + 1, next_seq, TCP_FIN | TCP_ACK, 65535)
                fin_ack = peer.wait(
                    0.8, lambda p: bool(p["flags"] & TCP_ACK) and p["ack"] == isn + 2
                    and not p["flags"] & TCP_RST)
                check(case, "fin-acked", fin_ack is not None, "no ACK for the peer FIN")
                server_fin = peer.wait(
                    1.2, lambda p: bool(p["flags"] & TCP_FIN) and not p["flags"] & TCP_RST)
                check(case, "server-closed", server_fin is not None, "no server FIN")
                if server_fin is not None:
                    peer.send(port, remote_port, isn + 2, server_fin["seq"] + 1,
                              TCP_ACK, 65535)
                    time.sleep(0.02)
                    rst_seq = (isn + 2 + case["rst_offset"]) & 0xFFFFFFFF
                    peer.send(port, remote_port, rst_seq, server_fin["seq"] + 1, TCP_RST, 0)
                    reply = fuzz_quiet(
                        peer, 0.2,
                        allow=lambda p: bool(p["flags"] & TCP_FIN)
                        and p["seq"] == server_fin["seq"])
                    check(case, "rst-after-close-not-answered", reply is None,
                          f"unexpected={None if reply is None else hex(reply['flags'])}")
                canary_port = port + 10000
                canary = handshake(peer, canary_port, remote_port, 65535, window_scale=7,
                                   isn=isn ^ 0x5A5A5A5A)
                canary_data, _frames, _next_seq, _ = collect_stream(
                    peer, canary, canary_port, remote_port, 1.0, 65535,
                    stop_after=len(PAYLOAD))
                check(case, "listener-canary", canary_data == PAYLOAD,
                      f"delivered={len(canary_data)}/44")
                reset(peer, canary, canary_port, remote_port)
        except RuntimeError as error:
            check(case, "harness", False, str(error))
            try:
                reset(peer, {"isn": isn, "server_isn": 0}, port, remote_port)
            except Exception:
                pass
        elapsed = time.monotonic() - case_started
        if elapsed >= 0.5:
            log(f"fuzz: slow case={group}[{case['index']}] elapsed={elapsed:.2f}s")

    log(f"fuzz: group={previous_group} cases={group_cases} "
        f"elapsed={time.monotonic() - group_started:.2f}s")
    log(f"fuzz: summary cases={len(cases)} deviations={deviations} "
        f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    detail = (f"cases={len(cases)} deviations={deviations} "
              f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    return deviations == 0, detail


# K548 rev 41: UDP, ICMP, IPv4 options and fragmentation.  Two bounded rows keep each campaign
# under 60 s: `fuzz-l4` is the fast groups, `fuzz-frag-timeout` spends one 31 s wait on the
# in-tree 30 s reassembly expiry.
FUZZ_L4_CASES = 104
FUZZ_L4_TABLE_HASH = 0xCAFD_5886_1CA3_40F4
FUZZ_FRAG_TIMEOUT_CASES = 2
FUZZ_FRAG_TIMEOUT_TABLE_HASH = 0xF268_2BC1_E696_D88D

IP_OPTION_VARIANTS = (
    ("record-route", bytes([7, 7, 4, 0, 0, 0, 0, 1]), None, None),
    ("timestamp", bytes([68, 8, 5, 0, 0, 0, 0, 0]), None, None),
    ("nop-eol", bytes([1, 0, 0, 0]), None, None),
    ("unknown", bytes([0x42, 4, 0, 0]), None, None),
    ("ihl4", b"", 4, None),
    ("ihl6-short", bytes([1, 1, 1, 1]), 6, 20),
    ("bad-ip-checksum", bytes([1, 0, 0, 0]), None, None),
)


def fuzz_bytes(value: int, length: int) -> bytes:
    state = value & 0xFFFFFFFFFFFFFFFF
    data = bytearray()
    for _ in range(length):
        state = (state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        data.append((state >> 32) & 0xFF)
    return bytes(data)


def fuzz_l4_table(seed: int) -> list[dict]:
    numbers = fuzz_lcg(seed, FUZZ_L4_CASES)
    cases: list[dict] = []

    def add(group: str, index: int, **fields) -> None:
        case = {"group": group, "index": index, "port": 44000 + len(cases)}
        case.update(fields)
        cases.append(case)

    udp_lengths = (0, 1, 8, 16, 32, 64, 128, 256, 512, 1024)
    for index in range(32):
        if index < 10:
            kind = "echo"
            length = udp_lengths[index]
        elif index < 20:
            kind = "bad-checksum"
            length = udp_lengths[index - 10]
        elif index < 28:
            kind = "bad-length"
            length = udp_lengths[index - 20]
        else:
            kind = "wrong-port"
            length = 16
        add(
            "udp",
            index,
            kind=kind,
            src_port=44000 + index,
            dst_port=RAVEN_UDP_PORT + 2 if kind != "wrong-port" else RAVEN_UDP_PORT + 3,
            payload_len=length,
            payload=fuzz_bytes(numbers[len(cases)], length).hex(),
        )

    icmp_lengths = (0, 8, 16, 24, 32, 48, 64, 96, 128, 256, 512, 1024)
    for index in range(24):
        if index < 12:
            kind = "echo"
            length = icmp_lengths[index]
            code = 0
        elif index < 16:
            kind = ("truncated", "bad-checksum", "unknown", "redirect")[index - 12]
            length = 16
            code = 0
        elif index < 20:
            kind = "unreachable"
            length = 16
            code = (0, 1, 3, 4)[index - 16]
        elif index < 22:
            kind = "broadcast-echo"
            length = 16
            code = 0
        else:
            kind = "wrong-destination"
            length = 16
            code = 0
        add(
            "icmp",
            index,
            kind=kind,
            ident=(0x4B00 + index) & 0xFFFF,
            seq=index,
            code=code,
            payload_len=length,
            payload=fuzz_bytes(numbers[len(cases)], length).hex(),
            dst_ip="10.0.2.255" if kind == "broadcast-echo" else
                   "10.0.2.99" if kind == "wrong-destination" else RAVEN_GUEST,
        )

    for index in range(24):
        name, options, ihl_override, total_override = IP_OPTION_VARIANTS[
            index % len(IP_OPTION_VARIANTS)
        ]
        add(
            "ip-options",
            index,
            kind=name,
            ident=0x4C00 + index,
            seq=index,
            options=options.hex(),
            ihl_override=ihl_override,
            total_override=total_override,
            payload_len=16,
            payload=fuzz_bytes(numbers[len(cases)], 16).hex(),
        )

    for index in range(24):
        if index < 12:
            kind = "valid"
            length = (24, 32, 64, 128, 256, 512)[index % 6]
            chunk = (8, 16, 32)[index % 3]
        elif index < 16:
            kind = "out-of-order"
            length = 64
            chunk = 16
        elif index < 20:
            kind = ("overlap", "tiny", "missing-last", "duplicate")[index - 16]
            length = 64
            chunk = 16
        else:
            kind = "bad-icmp-checksum"
            length = 64
            chunk = 16
        add(
            "frag",
            index,
            kind=kind,
            ident=0x5000 + index,
            icmp_ident=0x4D00 + index,
            seq=index,
            payload_len=length,
            chunk=chunk,
            payload=fuzz_bytes(numbers[len(cases)], length).hex(),
        )

    if len(cases) != FUZZ_L4_CASES:
        raise RuntimeError(f"L4 fuzz table has {len(cases)} cases")
    return cases


def fuzz_frag_timeout_table(seed: int) -> list[dict]:
    numbers = fuzz_lcg(seed, FUZZ_FRAG_TIMEOUT_CASES)
    cases = []
    for index, kind in enumerate(("timeout", "canary")):
        cases.append({
            "group": "frag-timeout",
            "index": index,
            "kind": kind,
            "ident": 0x6000 + index,
            "icmp_ident": 0x4E00 + index,
            "seq": index,
            "payload_len": 64,
            "chunk": 8,
            "payload": fuzz_bytes(numbers[index], 64).hex(),
        })
    return cases


def fuzz_quiet_l4(peer: RawPeer, timeout: float, predicate) -> dict | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = peer.recv_l4(min(0.1, max(0.0, deadline - time.monotonic())), predicate)
        if frame is not None:
            return frame
    return None


def send_ipv4_l4(peer: RawPeer, protocol: int, payload: bytes,
                 dst_ip: str | None = None, **fields) -> None:
    destination = peer.guest_ip if dst_ip is None else dst_ip
    peer.send_raw(build_ipv4_frame(
        peer.mac, peer.guest_mac, peer.host_ip, destination, protocol, payload, **fields
    ))


def send_udp_unbound_case(peer: RawPeer, case: dict) -> None:
    payload = bytes.fromhex(case["payload"])
    kind = case["kind"]
    datagram = build_udp(
        peer.host_ip,
        peer.guest_ip,
        case["src_port"],
        case["dst_port"],
        payload,
        bad_checksum=kind == "bad-checksum",
        length_override=8 + len(payload) + 1 if kind == "bad-length" else None,
    )
    send_ipv4_l4(peer, 17, datagram, ident=0x4000 + case["index"])


def send_icmp_case(peer: RawPeer, case: dict) -> None:
    payload = bytes.fromhex(case["payload"])
    kind = case["kind"]
    rest = (case["ident"] << 16) | case["seq"]
    if kind == "unreachable":
        inner = ipv4_packet(
            peer.host_ip,
            peer.guest_ip,
            17,
            build_udp(peer.host_ip, peer.guest_ip, 1234, 53, b""),
            ident=0x7000 + case["index"],
        )
        message = build_icmp(3, case["code"], 0, inner)
    elif kind == "truncated":
        message = build_icmp(8, 0, rest, payload, truncate=4)
    elif kind == "bad-checksum":
        message = build_icmp(8, 0, rest, payload, bad_checksum=True)
    elif kind == "unknown":
        message = build_icmp(42, 0, rest, payload)
    elif kind == "redirect":
        message = build_icmp(5, 1, rest, payload)
    else:
        message = build_icmp(8, 0, rest, payload)
    send_ipv4_l4(peer, 1, message, ident=0x4100 + case["index"], dst_ip=case["dst_ip"])


def send_option_case(peer: RawPeer, case: dict) -> None:
    payload = bytes.fromhex(case["payload"])
    message = build_icmp(8, 0, (case["ident"] << 16) | case["seq"], payload)
    frame = build_ipv4_frame(
        peer.mac,
        peer.guest_mac,
        peer.host_ip,
        peer.guest_ip,
        1,
        message,
        ident=0x4200 + case["index"],
        options=bytes.fromhex(case["options"]),
        ihl_override=case["ihl_override"],
        total_override=case["total_override"],
    )
    if case["kind"] == "bad-ip-checksum":
        frame = bytearray(frame)
        frame[14 + 10] ^= 0xFF
        frame = bytes(frame)
    peer.send_raw(frame)


def send_fragments(peer: RawPeer, case: dict, fragments: list[tuple[int, bool, bytes]],
                   protocol: int = 1) -> None:
    for offset, more, data in fragments:
        send_ipv4_l4(
            peer,
            protocol,
            data,
            ident=case["ident"],
            flags=1 if more else 0,
            frag_offset=offset,
        )


def fuzz_icmp_warmup(peer: RawPeer) -> bool:
    rest = 0x4A00_0000
    send_ipv4_l4(
        peer, 1, build_icmp(8, 0, rest, b"K548-WARMUP"), ident=0x4400
    )
    reply = peer.recv_l4(
        1.5,
        lambda p: p["type"] == "icmp" and p["message_type"] == 0
        and p["rest"] == rest,
    )
    return reply is not None


def scenario_fuzz_l4(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    cases = fuzz_l4_table(FUZZ_SEED)
    table_hash = fuzz_table_hash(cases)
    mutations = set(filter(None, os.environ.get("NETCONF_FUZZ_MUTATION", "").split(",")))
    deviations = 0
    group_deviations: dict[str, int] = {}

    def check(case: dict, name: str, passed: bool, detail: str) -> None:
        nonlocal deviations
        if passed:
            return
        deviations += 1
        group_deviations[case["group"]] = group_deviations.get(case["group"], 0) + 1
        log(f"deviation case={case['group']}[{case['index']}] check={name} {detail}")

    def mutated(group: str) -> bool:
        return "all" in mutations or group in mutations

    log(f"fuzz-l4: start seed=0x{FUZZ_SEED:016x} cases={len(cases)} "
        f"table-hash=0x{table_hash:016x}")
    if not fuzz_icmp_warmup(peer):
        log("fuzz-l4: setup failed: no ICMP echo warm-up")
        log(f"fuzz-l4: summary cases={len(cases)} deviations=1 "
            f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
        return False, "no ICMP echo warm-up"

    for case in cases:
        group = case["group"]
        kind = case["kind"]
        if group == "udp":
            send_udp_unbound_case(peer, case)
            if kind in ("echo", "wrong-port"):
                reply = fuzz_quiet_l4(
                    peer, 0.8,
                    lambda p: p["type"] == "icmp" and p["message_type"] == 3
                    and p["code"] == (4 if mutated("udp") else 3),
                )
                quoted = b"" if reply is None else reply["payload"]
                quoted_ok = (
                    len(quoted) >= 28
                    and quoted[9] == 17
                    and quoted[20:24] == struct.pack("!HH", case["src_port"], case["dst_port"])
                )
                check(case, "udp-port-unreachable",
                      reply is not None and quoted_ok,
                      f"icmp={None if reply is None else reply['message_type']} "
                      f"quote={len(quoted)}B")
            else:
                reply = fuzz_quiet_l4(peer, 0.2, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "udp-malformed-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
        elif group == "icmp":
            send_icmp_case(peer, case)
            if kind in ("echo", "broadcast-echo", "wrong-destination"):
                if kind == "echo":
                    reply = peer.recv_l4(
                        1.0,
                        lambda p: p["type"] == "icmp" and p["message_type"] == 0
                        and p["rest"] == ((case["ident"] << 16) | case["seq"]),
                    )
                    expected = bytes.fromhex(case["payload"])
                    if mutated("icmp"):
                        expected = expected[:-1] if expected else b"x"
                    check(case, "icmp-echo",
                          reply is not None and reply["payload"] == expected,
                          f"reply={None if reply is None else len(reply['payload'])}B "
                          f"expected={len(expected)}B")
                else:
                    reply = fuzz_quiet_l4(peer, 0.2, lambda p: p["type"] == "icmp")
                    check(case, f"icmp-{kind}-drop", reply is None,
                          f"unexpected={None if reply is None else reply['message_type']}")
            else:
                reply = fuzz_quiet_l4(peer, 0.2, lambda p: p["type"] == "icmp")
                check(case, "icmp-malformed-drop", reply is None,
                      f"unexpected={None if reply is None else reply['message_type']}")
        elif group == "ip-options":
            send_option_case(peer, case)
            if kind in ("record-route", "timestamp", "nop-eol", "unknown"):
                rest = (case["ident"] << 16) | case["seq"]
                reply = peer.recv_l4(
                    1.0,
                    lambda p: p["type"] == "icmp" and p["message_type"] == 0
                    and p["rest"] == rest,
                )
                expected = bytes.fromhex(case["payload"])
                if mutated("ip-options"):
                    expected = expected[:-1] if expected else b"x"
                check(case, "ip-option-ignored",
                      reply is not None and reply["payload"] == expected,
                      f"reply={None if reply is None else len(reply['payload'])}B "
                      f"expected={len(expected)}B")
            else:
                reply = fuzz_quiet_l4(peer, 0.2, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "ip-option-malformed-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
        elif group == "frag":
            payload = bytes.fromhex(case["payload"])
            rest = (case["icmp_ident"] << 16) | case["seq"]
            message = build_icmp(
                8, 0, rest, payload, bad_checksum=kind == "bad-icmp-checksum"
            )
            fragments = udp_fragments(message, case["chunk"])
            if kind in ("valid", "out-of-order"):
                ordered = list(reversed(fragments)) if kind == "out-of-order" else fragments
                send_fragments(peer, case, ordered)
                reply = peer.recv_l4(
                    1.5,
                    lambda p: p["type"] == "icmp" and p["message_type"] == 0
                    and p["rest"] == rest,
                )
                expected = payload[:-1] if mutated("frag") else payload
                check(case, "frag-reassembled",
                      reply is not None and reply["payload"] == expected,
                      f"reply={None if reply is None else len(reply['payload'])}B "
                      f"expected={len(expected)}B")
            elif kind == "missing-last":
                send_fragments(peer, case, fragments[:-1])
                reply = fuzz_quiet_l4(peer, 0.3, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "frag-missing-last-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
            elif kind == "overlap":
                send_ipv4_l4(peer, 1, message[0:16], ident=case["ident"], flags=1)
                send_ipv4_l4(peer, 1, message[8:24], ident=case["ident"], flags=1,
                             frag_offset=8)
                reply = fuzz_quiet_l4(peer, 0.3, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "frag-overlap-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
            elif kind == "tiny":
                send_ipv4_l4(peer, 1, message[0:4], ident=case["ident"], flags=1)
                send_ipv4_l4(peer, 1, message[8:], ident=case["ident"], frag_offset=8)
                reply = fuzz_quiet_l4(peer, 0.3, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "frag-tiny-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
            elif kind == "duplicate":
                send_ipv4_l4(peer, 1, fragments[0][2], ident=case["ident"],
                             flags=1 if fragments[0][1] else 0,
                             frag_offset=fragments[0][0])
                send_ipv4_l4(peer, 1, fragments[0][2], ident=case["ident"],
                             flags=1 if fragments[0][1] else 0,
                             frag_offset=fragments[0][0])
                reply = fuzz_quiet_l4(peer, 0.3, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "frag-duplicate-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")
            else:
                send_fragments(peer, case, fragments)
                reply = fuzz_quiet_l4(peer, 0.3, lambda p: p["type"] in ("udp", "icmp"))
                check(case, "frag-bad-icmp-drop", reply is None,
                      f"unexpected={None if reply is None else reply['type']}")

    for group in ("udp", "icmp", "ip-options", "frag"):
        log(f"fuzz-l4: group={group} deviations={group_deviations.get(group, 0)}")
    log(f"fuzz-l4: summary cases={len(cases)} deviations={deviations} "
        f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    detail = (f"cases={len(cases)} deviations={deviations} "
              f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    return deviations == 0, detail


def scenario_fuzz_frag_timeout(peer: RawPeer, local_port: int,
                               remote_port: int) -> tuple[bool, str]:
    cases = fuzz_frag_timeout_table(FUZZ_SEED)
    table_hash = fuzz_table_hash(cases)
    mutations = set(filter(None, os.environ.get("NETCONF_FUZZ_MUTATION", "").split(",")))
    mutated = "all" in mutations or "frag-timeout" in mutations
    deviations = 0

    def check(case: dict, name: str, passed: bool, detail: str) -> None:
        nonlocal deviations
        if passed:
            return
        deviations += 1
        log(f"deviation case=frag-timeout[{case['index']}] check={name} {detail}")

    log(f"fuzz-frag-timeout: start seed=0x{FUZZ_SEED:016x} cases={len(cases)} "
        f"table-hash=0x{table_hash:016x}")
    if not fuzz_icmp_warmup(peer):
        log("fuzz-frag-timeout: setup failed: no ICMP echo warm-up")
        log(f"fuzz-frag-timeout: summary cases={len(cases)} deviations=1 "
            f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
        return False, "no ICMP echo warm-up"
    timeout_case, canary_case = cases
    timeout_payload = bytes.fromhex(timeout_case["payload"])
    timeout_rest = (timeout_case["icmp_ident"] << 16) | timeout_case["seq"]
    timeout_message = build_icmp(8, 0, timeout_rest, timeout_payload)
    timeout_fragments = udp_fragments(timeout_message, timeout_case["chunk"])
    send_fragments(peer, timeout_case, timeout_fragments[:-1])
    time.sleep(31.0)
    send_fragments(peer, timeout_case, timeout_fragments[-1:])
    reply = fuzz_quiet_l4(peer, 0.6, lambda p: p["type"] == "icmp")
    check(timeout_case, "frag-timeout-drop", reply is None if not mutated else reply is not None,
          f"reply={None if reply is None else reply['type']}")

    canary_payload = bytes.fromhex(canary_case["payload"])
    canary_rest = (canary_case["icmp_ident"] << 16) | canary_case["seq"]
    canary_message = build_icmp(8, 0, canary_rest, canary_payload)
    send_fragments(peer, canary_case, udp_fragments(canary_message, canary_case["chunk"]))
    reply = peer.recv_l4(
        1.5,
        lambda p: p["type"] == "icmp" and p["message_type"] == 0
        and p["rest"] == canary_rest,
    )
    expected = canary_payload[:-1] if mutated else canary_payload
    check(canary_case, "frag-canary",
          reply is not None and reply["payload"] == expected,
          f"reply={None if reply is None else len(reply['payload'])}B "
          f"expected={len(expected)}B")

    log(f"fuzz-frag-timeout: summary cases={len(cases)} deviations={deviations} "
        f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    detail = (f"cases={len(cases)} deviations={deviations} "
              f"seed=0x{FUZZ_SEED:016x} table-hash=0x{table_hash:016x}")
    return deviations == 0, detail


def percentile(values: list[int], fraction: float) -> int:
    if not values:
        return 0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(fraction * (len(ordered) - 1)))))
    return ordered[index]


def scenario_churn(peer: RawPeer, local_port: int, remote_port: int) -> tuple[bool, str]:
    total = 1000
    mutation = os.environ.get("NETCONF_CHURN_MUTATION") == "1"
    connects: list[int] = []
    accepted = 0
    reads = 0
    deviations = 0
    timeouts = 0
    resets = 0
    started = time.monotonic()

    for index in range(total):
        port = 47000 + index
        began = time.monotonic()
        try:
            state = handshake(
                peer, port, remote_port, 65535, window_scale=7,
                isn=0x20000000 + index,
            )
            connects.append(int((time.monotonic() - began) * 1000))
            accepted += 1
            data, _frames, _next_seq, _ = collect_stream(
                peer, state, port, remote_port, 1.0, 65535,
                stop_after=len(PAYLOAD),
            )
            expected = PAYLOAD[:-1] if mutation else PAYLOAD
            if data == expected:
                reads += 1
            else:
                deviations += 1
                log(f"churn: deviation index={index} read={len(data)}/{len(expected)}")
            reset(peer, state, port, remote_port)
            resets += 1
        except RuntimeError as error:
            timeouts += 1
            log(f"churn: timeout index={index} {error}")

    elapsed = time.monotonic() - started
    ready_ms = getattr(peer, "ready_ms", 0)
    summary = (
        f"connects={total} accepted={accepted} reads={reads} deviations={deviations} "
        f"timeouts={timeouts} resets={resets} ready-ms={ready_ms} "
        f"connect-ms min={min(connects) if connects else 0} "
        f"p50={percentile(connects, 0.50)} p90={percentile(connects, 0.90)} "
        f"max={max(connects) if connects else 0} elapsed={elapsed:.2f}s"
    )
    log(f"churn: summary {summary}")
    return deviations == 0 and accepted == total and reads + deviations == total, summary


SCENARIOS = {
    "handshake": scenario_handshake,
    "ecn": scenario_ecn,
    "zero-window": scenario_zero_window,
    "window-update": scenario_window_update,
    "retransmit": scenario_retransmit,
    "fin-rst": scenario_fin_rst,
    "fuzz": scenario_fuzz,
    "fuzz-l4": scenario_fuzz_l4,
    "fuzz-frag-timeout": scenario_fuzz_frag_timeout,
    "churn": scenario_churn,
}
NARROWED = {
    "dup-ack": "a single client cannot make the server's receiver see duplicate ACKs; needs a server-to-client data phase",
    "out-of-order": "the fixture has one in-order client->server stream; reordering the server->client stream needs a second raw client",
    "partial-ack": "partial ACK needs a multi-segment server write and selective loss; the 44-byte fixture is one segment",
    "backlog": "the guest listener accepts one connection at a time; a multi-SYN backlog needs a second listener harness",
    "busy": "device/queue busy injection needs a driver shim outside this test-only ticket",
    "pmtu-blackhole": "the fixture writes only 44 bytes, below every tested MTU, so a path-MTU decrease is not observable; needs a router namespace with a reduced MTU and suppressed ICMP frag-needed",
    "plpmtud": "PLPMTUD probes are only emitted for writes above the probe floor; the 44-byte fixture cannot trigger one; needs a large-write fixture",
}


def run(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, capture_output=True, text=True)


def linux_netns_setup() -> tuple[subprocess.Popen, str, str]:
    """Create a disposable netns for the Linux server and a veth pair to it."""
    holder = subprocess.Popen(["unshare", "-n", "sleep", "300"])
    try:
        run(["ip", "link", "del", LINUX_IF_A], check=False)
        run(["ip", "link", "add", LINUX_IF_A, "type", "veth", "peer", "name", LINUX_IF_B])
        run(["ip", "link", "set", LINUX_IF_A, "address", LINUX_IF_A_MAC])
        run(["ip", "link", "set", LINUX_IF_B, "address", LINUX_IF_B_MAC])
        run(["ip", "link", "set", LINUX_IF_B, "netns", str(holder.pid)])
        run(["ip", "addr", "add", f"{LINUX_HOST}/24", "dev", LINUX_IF_A])
        run(["ip", "link", "set", LINUX_IF_A, "up"])
        nsenter = ["nsenter", "-t", str(holder.pid), "-n"]
        run(nsenter + ["ip", "addr", "add", f"{LINUX_GUEST}/24", "dev", LINUX_IF_B])
        run(nsenter + ["ip", "link", "set", LINUX_IF_B, "up"])
        run(nsenter + ["ip", "link", "set", "lo", "up"])
        run(nsenter + ["sysctl", "-qw", "net.ipv4.tcp_ecn=1"], check=False)
        run(nsenter + ["ip", "neigh", "replace", LINUX_HOST, "lladdr", LINUX_IF_A_MAC,
                       "dev", LINUX_IF_B])
        run(["ip", "neigh", "replace", LINUX_GUEST, "lladdr", LINUX_IF_B_MAC,
             "dev", LINUX_IF_A])
    except Exception:
        holder.terminate()
        holder.wait()
        raise
    return holder, LINUX_IF_A_MAC, LINUX_IF_B_MAC


def run_linux(scenario: str, pcap: Path) -> tuple[bool, str]:
    holder, _client_mac, guest_mac = linux_netns_setup()
    server = None
    dropped_rst = False
    try:
        # The host kernel has no socket for the raw handshake and would answer the
        # unexpected SYN-ACK/data with RST.  The tested connection is the raw one.
        drop = ["iptables", "-I", "OUTPUT", "-p", "tcp", "--dst", LINUX_GUEST,
                "--tcp-flags", "RST", "RST", "-j", "DROP"]
        if run(drop, check=False).returncode == 0:
            dropped_rst = True
        server = subprocess.Popen(
            ["nsenter", "-t", str(holder.pid), "-n", sys.executable,
             str(Path(__file__).with_name("linux_server.py")),
             "--host", LINUX_GUEST, "--port", str(LINUX_PORT)],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        )
        assert server.stdout is not None
        ready = server.stdout.readline().strip()
        if ready != "ready":
            raise RuntimeError(f"linux server did not bind: {ready}")
        peer = RawPeer(LINUX_IF_A, LINUX_HOST, LINUX_GUEST, guest_mac, pcap)
        try:
            return SCENARIOS[scenario](peer, 40000 + random.randrange(1000), LINUX_PORT)
        finally:
            peer.close()
    finally:
        if server is not None:
            server.terminate()
            try:
                server.wait(timeout=2)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        if dropped_rst:
            run(["iptables", "-D", "OUTPUT", "-p", "tcp", "--dst", LINUX_GUEST,
                 "--tcp-flags", "RST", "RST", "-j", "DROP"], check=False)
        run(["ip", "link", "del", LINUX_IF_A], check=False)
        holder.terminate()
        holder.wait()


def qemu_boot(image: Path, serial: Path, tap: str) -> subprocess.Popen:
    serial.parent.mkdir(parents=True, exist_ok=True)
    Path("build/qtest").mkdir(parents=True, exist_ok=True)
    serial.write_bytes(b"")
    vars_file = Path("build/qtest") / f"net-conformance-{serial.stem}.vars.fd"
    subprocess.run(
        ["cp", "/usr/share/OVMF/OVMF_VARS_4M.fd", str(vars_file)], check=True
    )
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd",
        "-drive", f"if=pflash,format=raw,file={vars_file}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"tap,id=n0,ifname={tap},script=no,downscript=no",
        "-device", f"virtio-net-pci,netdev=n0,disable-legacy=on,mac={RAVEN_GUEST_MAC}",
    ]
    process = subprocess.Popen(
        argv, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True
    )
    # A cap can kill this Python process before its finally block; the caller uses the pid file
    # to reap only this QEMU rather than leaving an image lock behind.
    serial.with_suffix(".qemu.pid").write_text(str(process.pid))
    return process


def wait_serial(serial: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if serial.is_file() and needle in serial.read_bytes():
            return True
        time.sleep(0.2)
    return False


def run_raven(scenario: str, image: Path, pcap: Path, serial: Path,
              tap: str) -> tuple[bool, str]:
    started = time.monotonic()
    qemu = qemu_boot(image, serial, tap)
    try:
        if not wait_serial(serial, b"netconf: listening", 150):
            stderr = b""
            if qemu.stderr is not None:
                try:
                    stderr = qemu.stderr.read(4096)
                except Exception:
                    pass
            tail = serial.read_bytes()[-2000:] if serial.is_file() else b""
            return False, (
                "guest did not reach netconf: listening; "
                f"serial-tail={tail[-400:]!r} qemu={stderr[-300:]!r}"
            )
        ready_ms = int((time.monotonic() - started) * 1000)
        # The guest loads static 10.0.2.15 and needs its ARP entry to settle.
        time.sleep(1.0)
        peer = RawPeer(tap, RAVEN_HOST, RAVEN_GUEST, RAVEN_GUEST_MAC, pcap)
        peer.ready_ms = ready_ms
        try:
            ok, detail = SCENARIOS[scenario](peer, 40000 + random.randrange(1000), RAVEN_PORT)
        finally:
            peer.close()
        if scenario.startswith("fuzz") or scenario == "churn":
            time.sleep(0.2)
            serial_text = serial.read_text(errors="replace") if serial.is_file() else ""
            fault = re.search(r"(?i)\bpanic\b|EXCEPTION|BUG:|Oops|double fault", serial_text)
            if fault is not None:
                ok = False
                detail += f"; guest-fault={fault.group(0)}"
            if qemu.poll() is not None:
                ok = False
                detail += f"; qemu-exit={qemu.returncode}"
        return ok, detail
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()
        try:
            serial.with_suffix(".qemu.pid").unlink()
        except FileNotFoundError:
            pass


def selftest() -> int:
    frame = build_frame(
        "02:00:00:00:00:01", "02:00:00:00:00:02", "10.0.0.1", "10.0.0.2",
        1234, 80, 1000, 0, TCP_SYN, 65535, tcp_options(1460, 7),
    )
    parsed = parse_frame(frame)
    assert parsed and parsed["type"] == "tcp" and parsed["seq"] == 1000
    options = parse_tcp_options(parsed["options"])
    assert options["mss"] == 1460 and options["window_scale"] == 7 and options["sack"]
    header = struct.pack(
        "!BBHHHBBH4s4s", 0x45, 0, 40, 1, 0x4000, 64, 6, 0,
        ip_bytes("10.0.0.1"), ip_bytes("10.0.0.2"),
    )
    fixed = header[:10] + struct.pack("!H", checksum(header)) + header[12:]
    assert checksum(fixed) == 0
    assert len(PAYLOAD) == 44
    pcap = Path("/tmp/net-conformance-selftest.pcap")
    writer = PcapWriter(pcap)
    writer.write(frame)
    writer.close()
    assert pcap.read_bytes()[:4] == b"\xd4\xc3\xb2\xa1"
    assert {"handshake", "zero-window", "window-update", "retransmit", "fin-rst",
            "fuzz", "fuzz-l4", "fuzz-frag-timeout", "churn"} <= set(SCENARIOS)
    cases = fuzz_table(FUZZ_SEED)
    assert len(cases) == FUZZ_CASES
    assert len({case["port"] for case in cases}) == FUZZ_CASES
    assert fuzz_table_hash(cases) == FUZZ_TABLE_HASH
    log(f"fuzz table seed=0x{FUZZ_SEED:016x} cases={FUZZ_CASES} "
        f"table-hash=0x{FUZZ_TABLE_HASH:016x}")
    l4_cases = fuzz_l4_table(FUZZ_SEED)
    assert len(l4_cases) == FUZZ_L4_CASES
    assert fuzz_table_hash(l4_cases) == FUZZ_L4_TABLE_HASH
    log(f"fuzz-l4 table seed=0x{FUZZ_SEED:016x} cases={FUZZ_L4_CASES} "
        f"table-hash=0x{FUZZ_L4_TABLE_HASH:016x}")
    timeout_cases = fuzz_frag_timeout_table(FUZZ_SEED)
    assert len(timeout_cases) == FUZZ_FRAG_TIMEOUT_CASES
    assert fuzz_table_hash(timeout_cases) == FUZZ_FRAG_TIMEOUT_TABLE_HASH
    log(f"fuzz-frag-timeout table seed=0x{FUZZ_SEED:016x} "
        f"cases={FUZZ_FRAG_TIMEOUT_CASES} "
        f"table-hash=0x{FUZZ_FRAG_TIMEOUT_TABLE_HASH:016x}")
    log("selftest PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--scenario", default="handshake")
    parser.add_argument("--baseline", choices=["linux"])
    parser.add_argument("--target", choices=["raven"])
    parser.add_argument("--image", default="build/qtest/net-conformance.img")
    parser.add_argument("--tap", default=os.environ.get("NETCONF_TAP", DEFAULT_TAP))
    parser.add_argument("--pcap", default="tests/net-conformance/results/latest.pcap")
    parser.add_argument("--serial", default="tests/net-conformance/results/latest-serial.log")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    if args.scenario in NARROWED:
        log(f"scenario {args.scenario} narrowed: {NARROWED[args.scenario]}")
        return 77
    if args.scenario not in SCENARIOS:
        print(f"net-conformance: unknown scenario {args.scenario}", file=sys.stderr)
        return 2
    pcap = Path(args.pcap)
    pcap.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    if args.baseline == "linux":
        ok, detail = run_linux(args.scenario, pcap)
    elif args.target == "raven":
        ok, detail = run_raven(
            args.scenario, Path(args.image), pcap, Path(args.serial), args.tap
        )
    else:
        print("net-conformance: choose --baseline linux or --target raven", file=sys.stderr)
        return 2
    elapsed = time.monotonic() - started
    log(f"{args.scenario} {'pass' if ok else 'fail'} ({elapsed:.2f}s) {detail}")
    log(f"pcap={pcap}")
    if args.target == "raven":
        log(f"serial={args.serial}")
    return 0 if ok else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        import traceback

        traceback.print_exc()
        print(f"net-conformance: ERROR {error}", file=sys.stderr)
        sys.exit(1)
