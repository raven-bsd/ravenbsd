#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Minimal raw-TCP peer and pcap writer for the K263 Linux baselines.

The Linux kernel is the stack under test. One end of a veth pair (``k263h``) holds
10.263.0.1 and a normal Python listener; this peer sends and receives Ethernet
frames on the other end (``k263p``) with source 10.263.0.2. No host TCP socket is
used for the peer, so SYN options, ACK selection and window/sequence numbers are
under scenario control.
"""

import os
import socket
import struct
import subprocess
import time

ETH_P_ALL = 0x0003
ETH_P_IP = 0x0800
IPPROTO_TCP = 6

FIN = 0x01
SYN = 0x02
RST = 0x04
PSH = 0x08
ACK = 0x10
ECE = 0x40
CWR = 0x80

HOST_IF = "k263h"
PEER_IF = "k263p"
HOST_IP = "10.26.3.1"
PEER_IP = "10.26.3.2"
HOST_MAC_FALLBACK = bytes.fromhex("020000000001")
PEER_MAC_FALLBACK = bytes.fromhex("020000000002")


def run(argv, check=True):
    result = subprocess.run(argv, text=True, capture_output=True)
    if check and result.returncode != 0:
        raise RuntimeError(f"{argv}: {result.stderr.strip()}")
    return result


def read_mac(iface):
    with open(f"/sys/class/net/{iface}/address", encoding="ascii") as handle:
        return bytes.fromhex(handle.read().strip().replace(":", ""))


def setup_veth():
    run(["ip", "link", "del", HOST_IF], check=False)
    run(["ip", "link", "add", HOST_IF, "type", "veth", "peer", "name", PEER_IF])
    run(["ip", "addr", "add", f"{HOST_IP}/24", "dev", HOST_IF])
    run(["ip", "link", "set", HOST_IF, "up"])
    run(["ip", "link", "set", PEER_IF, "up"])
    run(["ip", "neigh", "replace", PEER_IP, "lladdr",
         ":".join(f"{b:02x}" for b in read_mac(PEER_IF)), "dev", HOST_IF, "nud", "permanent"])
    run(["sysctl", "-q", "-w", f"net.ipv4.conf.{HOST_IF}.rp_filter=0"])
    run(["sysctl", "-q", "-w", f"net.ipv6.conf.{HOST_IF}.disable_ipv6=1"], check=False)
    run(["sysctl", "-q", "-w", f"net.ipv6.conf.{PEER_IF}.disable_ipv6=1"], check=False)
    run(["ip", "link", "set", PEER_IF, "arp", "off"], check=False)
    return read_mac(HOST_IF), read_mac(PEER_IF)


def teardown_veth():
    run(["ip", "link", "del", HOST_IF], check=False)


def csum(data):
    if len(data) & 1:
        data += b"\0"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def ipv4_header(src, dst, total, ident, tos=0, flags_frag=0x4000, ttl=64, proto=IPPROTO_TCP):
    header = struct.pack(
        "!BBHHHBBH4s4s",
        0x45, tos, total, ident, flags_frag, ttl, proto, 0,
        socket.inet_aton(src), socket.inet_aton(dst),
    )
    checksum = csum(header)
    return header[:10] + struct.pack("!H", checksum) + header[12:]


def tcp_segment(src_ip, dst_ip, sport, dport, seq, ack, flags, window, options=b"", payload=b""):
    while len(options) % 4:
        options += b"\x01"
    offset = (20 + len(options)) // 4
    tcp = struct.pack(
        "!HHIIBBHHH",
        sport, dport, seq & 0xFFFFFFFF, ack & 0xFFFFFFFF,
        offset << 4, flags, window & 0xFFFF, 0, 0,
    ) + options + payload
    pseudo = struct.pack(
        "!4s4sBBH", socket.inet_aton(src_ip), socket.inet_aton(dst_ip), 0, IPPROTO_TCP, len(tcp)
    )
    checksum = csum(pseudo + tcp)
    return tcp[:16] + struct.pack("!H", checksum) + tcp[18:]


def option(kind, value=b""):
    return bytes([kind, len(value) + 2]) + value


MSS = option(2, struct.pack("!H", 1460))
SACK_PERMITTED = option(4)


def window_scale(shift):
    return option(3, bytes([shift]))


class Packet:
    def __init__(self, raw):
        self.raw = raw
        self.src_mac = raw[6:12]
        self.dst_mac = raw[0:6]
        if len(raw) < 34 or struct.unpack("!H", raw[12:14])[0] != ETH_P_IP:
            raise ValueError("not IPv4")
        ip = raw[14:]
        ihl = (ip[0] & 0x0F) * 4
        total = struct.unpack("!H", ip[2:4])[0]
        self.ip = ip[:total]
        self.tos = ip[1]
        self.src_ip = socket.inet_ntoa(ip[12:16])
        self.dst_ip = socket.inet_ntoa(ip[16:20])
        self.frag = struct.unpack("!H", ip[6:8])[0] & 0x1FFF
        tcp = self.ip[ihl:]
        self.sport, self.dport, self.seq, self.ack = struct.unpack("!HHII", tcp[:12])
        self.offset = (tcp[12] >> 4) * 4
        self.flags = tcp[13]
        self.window = struct.unpack("!H", tcp[14:16])[0]
        self.options = tcp[20:self.offset]
        self.payload = tcp[self.offset:]
        self.mss = None
        self.wscale = None
        self.sack = False
        index = 0
        while index < len(self.options):
            kind = self.options[index]
            if kind == 0:
                break
            if kind == 1:
                index += 1
                continue
            length = self.options[index + 1]
            if length < 2 or index + length > len(self.options):
                break
            value = self.options[index + 2:index + length]
            if kind == 2 and len(value) == 2:
                self.mss = struct.unpack("!H", value)[0]
            elif kind == 3 and len(value) == 1:
                self.wscale = value[0]
            elif kind == 4:
                self.sack = True
            index += length

    def __str__(self):
        names = []
        for bit, name in ((SYN, "S"), (ACK, "A"), (FIN, "F"), (RST, "R"), (PSH, "P"),
                          (ECE, "E"), (CWR, "C")):
            if self.flags & bit:
                names.append(name)
        return (f"{self.src_ip}:{self.sport}->{self.dst_ip}:{self.dport} "
                f"[{''.join(names)}] seq={self.seq} ack={self.ack} win={self.window} "
                f"len={len(self.payload)} mss={self.mss} ws={self.wscale} sack={self.sack}")


class Pcap:
    def __init__(self, path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        self.handle = open(path, "wb")
        self.handle.write(struct.pack("<IHHIIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, 1))

    def write(self, frame, incoming):
        seconds = time.time()
        self.handle.write(struct.pack("<IIII", int(seconds), int((seconds % 1) * 1_000_000),
                                      len(frame), len(frame)))
        self.handle.write(frame)
        self.handle.flush()

    def close(self):
        self.handle.close()


class RawPeer:
    def __init__(self, host_mac, peer_mac, pcap_path):
        self.host_mac = host_mac
        self.peer_mac = peer_mac
        self.sock = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(ETH_P_ALL))
        self.sock.bind((PEER_IF, 0))
        self.sock.settimeout(0.25)
        self.pcap = Pcap(pcap_path)
        self.ident = 0x4000

    def close(self):
        self.pcap.close()
        self.sock.close()

    def send(self, src_ip, dst_ip, sport, dport, seq, ack, flags, window,
             options=b"", payload=b"", tos=0):
        tcp = tcp_segment(src_ip, dst_ip, sport, dport, seq, ack, flags, window, options, payload)
        self.ident = (self.ident + 1) & 0xFFFF
        ip = ipv4_header(src_ip, dst_ip, 20 + len(tcp), self.ident, tos=tos)
        frame = self.host_mac + self.peer_mac + struct.pack("!H", ETH_P_IP) + ip + tcp
        self.sock.send(frame)
        self.pcap.write(frame, False)

    def recv(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                frame, address = self.sock.recvfrom(65535)
            except socket.timeout:
                continue
            if address[2] == socket.PACKET_OUTGOING:
                continue
            if frame[6:12] != self.host_mac:
                continue
            try:
                packet = Packet(frame)
            except ValueError:
                continue
            if packet.src_ip != HOST_IP or packet.dst_ip != PEER_IP:
                continue
            self.pcap.write(frame, True)
            if predicate(packet):
                return packet
        return None

    def recv_all(self, predicate, duration):
        deadline = time.monotonic() + duration
        packets = []
        while time.monotonic() < deadline:
            packet = self.recv(predicate, min(0.2, max(0.0, deadline - time.monotonic())))
            if packet is not None:
                packets.append(packet)
                deadline = time.monotonic() + duration
        return packets


def handshake(peer, port, with_wscale=True, ecn=False, sack=True, src_port=40000):
    isn = int(time.time() * 1000) & 0xFFFFFFFF
    options = MSS
    if sack:
        options += SACK_PERMITTED
    if with_wscale:
        options += window_scale(7)
    flags = SYN
    if ecn:
        flags |= ECE | CWR
    peer.send(PEER_IP, HOST_IP, src_port, port, isn, 0, flags, 65535, options)
    synack = peer.recv(lambda packet: packet.sport == port and packet.flags & SYN
                       and packet.flags & ACK, 3.0)
    if synack is None:
        return None, None, None
    peer.send(PEER_IP, HOST_IP, src_port, port, isn + 1, synack.seq + 1, ACK, 65535)
    return isn, synack, src_port


def run_endpoint(args):
    return subprocess.Popen(
        ["ip", "netns", "exec", "none"] if False else
        [os.environ.get("PYTHON", "python3"),
         os.path.join(os.path.dirname(__file__), "kernel_endpoint.py")] + args,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
