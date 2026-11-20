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
import os
import random
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

RAVEN_GUEST = "10.0.2.15"
RAVEN_HOST = "10.0.2.2"
RAVEN_PORT = 23090
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
        0,
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
    if ihl < 20 or len(frame) < 14 + ihl + 20:
        return None
    total = struct.unpack("!H", frame[16:18])[0]
    protocol = frame[23]
    src_ip = socket.inet_ntoa(frame[26:30])
    dst_ip = socket.inet_ntoa(frame[30:34])
    if protocol != 6:
        return {"type": "ipv4", "protocol": protocol}
    tcp_start = 14 + ihl
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
             window: int, options: bytes = b"", payload: bytes = b"") -> None:
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


def handshake(peer: RawPeer, local_port: int, remote_port: int, final_window: int,
              window_scale: int | None = 7) -> dict:
    peer.our_port = local_port
    peer.peer_port = remote_port
    isn = random.randrange(1, 0x7FFFFFFF)
    peer.send(local_port, remote_port, isn, 0, TCP_SYN, 65535,
              tcp_options(1460, window_scale))
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
    }


def ack(peer: RawPeer, state: dict, local_port: int, remote_port: int, next_seq: int,
        window: int) -> None:
    peer.send(local_port, remote_port, state["isn"] + 1, next_seq, TCP_ACK, window)


def collect_stream(peer: RawPeer, state: dict, local_port: int, remote_port: int,
                   timeout: float, window: int,
                   start_seq: int | None = None) -> tuple[bytes, list[dict], int, bool]:
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


SCENARIOS = {
    "handshake": scenario_handshake,
    "zero-window": scenario_zero_window,
    "window-update": scenario_window_update,
    "retransmit": scenario_retransmit,
    "fin-rst": scenario_fin_rst,
}
NARROWED = {
    "dup-ack": "a single client cannot make the server's receiver see duplicate ACKs; needs a server-to-client data phase",
    "out-of-order": "the fixture has one in-order client->server stream; reordering the server->client stream needs a second raw client",
    "partial-ack": "partial ACK needs a multi-segment server write and selective loss; the 44-byte fixture is one segment",
    "backlog": "the guest listener accepts one connection at a time; a multi-SYN backlog needs a second listener harness",
    "busy": "device/queue busy injection needs a driver shim outside this test-only ticket",
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
    return subprocess.Popen(
        argv, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True
    )


def wait_serial(serial: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if serial.is_file() and needle in serial.read_bytes():
            return True
        time.sleep(0.2)
    return False


def run_raven(scenario: str, image: Path, pcap: Path, serial: Path,
              tap: str) -> tuple[bool, str]:
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
        # The guest loads static 10.0.2.15 and needs its ARP entry to settle.
        time.sleep(1.0)
        peer = RawPeer(tap, RAVEN_HOST, RAVEN_GUEST, RAVEN_GUEST_MAC, pcap)
        try:
            return SCENARIOS[scenario](peer, 40000 + random.randrange(1000), RAVEN_PORT)
        finally:
            peer.close()
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()


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
    assert {"handshake", "zero-window", "window-update", "retransmit", "fin-rst"} <= set(SCENARIOS)
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
