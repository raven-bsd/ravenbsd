#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Deterministic pcap fixtures for the K277 tcpdump decode matrix.

The builder uses only the Python standard library and fixed timestamps, MAC
addresses, sequence numbers and payloads, so a regenerated pcap is byte-for-byte
identical to the committed file. Run:

    python3 tests/fixtures/tcpdump/generate.py --out /tmp/pf-pcaps

The expected decode evidence is produced independently by Linux
``tcpdump 4.99.1`` / ``libpcap 1.10.1``; see ``README.md``.
"""

from __future__ import annotations

import argparse
import socket
import struct
from pathlib import Path

ETH_P_IP = 0x0800
ETH_P_IPV6 = 0x86DD
ETH_P_ARP = 0x0806

FIN = 0x01
SYN = 0x02
RST = 0x04
PSH = 0x08
ACK = 0x10

A_MAC = bytes.fromhex("02000000000a")
B_MAC = bytes.fromhex("02000000000b")
BCAST_MAC = b"\xff" * 6

A4 = "10.0.0.5"
B4 = "192.0.2.10"
DNS4 = "192.0.2.53"
A6 = "2001:db8::5"
B6 = "2001:db8::10"
DNS6 = "2001:db8::53"

BASE_TIME = 1_700_000_000


def checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\x00"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def pseudo_v4(src: str, dst: str, proto: int, length: int) -> bytes:
    return (
        socket.inet_aton(src)
        + socket.inet_aton(dst)
        + struct.pack("!BBH", 0, proto, length)
    )


def pseudo_v6(src: str, dst: str, proto: int, length: int) -> bytes:
    return (
        socket.inet_pton(socket.AF_INET6, src)
        + socket.inet_pton(socket.AF_INET6, dst)
        + struct.pack("!I", length)
        + b"\x00\x00\x00"
        + struct.pack("!B", proto)
    )


def ipv4(
    src: str,
    dst: str,
    proto: int,
    payload: bytes,
    ident: int = 0,
    tos: int = 0,
    ttl: int = 64,
    flags_frag: int = 0,
) -> bytes:
    total = 20 + len(payload)
    header = struct.pack(
        "!BBHHHBBH4s4s",
        0x45,
        tos,
        total,
        ident,
        flags_frag,
        ttl,
        proto,
        0,
        socket.inet_aton(src),
        socket.inet_aton(dst),
    )
    check = checksum(header)
    return header[:10] + struct.pack("!H", check) + header[12:] + payload


def ipv6(src: str, dst: str, next_header: int, payload: bytes, hop: int = 64) -> bytes:
    header = struct.pack(
        "!IHBB16s16s",
        0x60000000,
        len(payload),
        next_header,
        hop,
        socket.inet_pton(socket.AF_INET6, src),
        socket.inet_pton(socket.AF_INET6, dst),
    )
    return header + payload


def tcp(
    src: str,
    dst: str,
    sport: int,
    dport: int,
    seq: int,
    ack: int,
    flags: int,
    window: int = 65535,
    options: bytes = b"",
    payload: bytes = b"",
    family: int = 4,
) -> bytes:
    while len(options) % 4:
        options += b"\x01"
    offset = (20 + len(options)) // 4
    segment = (
        struct.pack(
            "!HHIIBBHHH",
            sport,
            dport,
            seq & 0xFFFFFFFF,
            ack & 0xFFFFFFFF,
            offset << 4,
            flags,
            window & 0xFFFF,
            0,
            0,
        )
        + options
        + payload
    )
    pseudo = (
        pseudo_v6(src, dst, 6, len(segment))
        if family == 6
        else pseudo_v4(src, dst, 6, len(segment))
    )
    check = checksum(pseudo + segment)
    return segment[:16] + struct.pack("!H", check) + segment[18:]


def udp(
    src: str,
    dst: str,
    sport: int,
    dport: int,
    payload: bytes,
    family: int = 4,
) -> bytes:
    datagram = struct.pack("!HHHH", sport, dport, 8 + len(payload), 0) + payload
    pseudo = (
        pseudo_v6(src, dst, 17, len(datagram))
        if family == 6
        else pseudo_v4(src, dst, 17, len(datagram))
    )
    check = checksum(pseudo + datagram) or 0xFFFF
    return datagram[:6] + struct.pack("!H", check) + datagram[8:]


def icmp4(kind: int, code: int, ident: int, seq: int, payload: bytes) -> bytes:
    message = struct.pack("!BBHHH", kind, code, 0, ident, seq) + payload
    check = checksum(message)
    return message[:2] + struct.pack("!H", check) + message[4:]


def icmp6(
    src: str,
    dst: str,
    kind: int,
    code: int,
    ident: int,
    seq: int,
    payload: bytes,
) -> bytes:
    message = struct.pack("!BBHHH", kind, code, 0, ident, seq) + payload
    check = checksum(pseudo_v6(src, dst, 58, len(message)) + message)
    return message[:2] + struct.pack("!H", check) + message[4:]


def ethernet(dst_mac: bytes, src_mac: bytes, ethertype: int, payload: bytes) -> bytes:
    return dst_mac + src_mac + struct.pack("!H", ethertype) + payload


def tcp_options() -> bytes:
    mss = struct.pack("!BBH", 2, 4, 1460)
    sack = struct.pack("!BB", 4, 2)
    wscale = struct.pack("!BBB", 3, 3, 7)
    return mss + sack + wscale + b"\x01\x01\x01"


def dns_query() -> bytes:
    header = struct.pack("!HHHHHH", 0x1234, 0x0100, 1, 0, 0, 0)
    question = b"\x07example\x03com\x00" + struct.pack("!HH", 1, 1)
    return header + question


def dns_response() -> bytes:
    header = struct.pack("!HHHHHH", 0x1234, 0x8180, 1, 1, 0, 0)
    question = b"\x07example\x03com\x00" + struct.pack("!HH", 1, 1)
    answer = b"\xc0\x0c" + struct.pack("!HHIH", 1, 1, 60, 4) + socket.inet_aton(B4)
    return header + question + answer


class Pcap:
    def __init__(self, path: Path) -> None:
        self.handle = path.open("wb")
        self.handle.write(struct.pack("<IHHIIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, 1))
        self.index = 0

    def add(self, frame: bytes) -> None:
        self.add_record(frame, len(frame), len(frame))

    def add_record(self, frame: bytes, caplen: int, origlen: int) -> None:
        self.handle.write(
            struct.pack(
                "<IIII",
                BASE_TIME,
                self.index * 1000,
                caplen,
                origlen,
            )
        )
        self.handle.write(frame)
        self.index += 1

    def close(self) -> None:
        self.handle.close()


def write_tcp_handshake(path: Path) -> None:
    payload = b"GET / HTTP/1.0\r\n\r\n"
    pcap = Pcap(path)
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1000, 0, SYN, options=tcp_options()
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IP, ipv4(B4, A4, 6, tcp(
        B4, A4, 80, 40000, 5000, 1001, SYN | ACK, window=64240, options=tcp_options()
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1001, 5001, ACK
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1001, 5001, PSH | ACK, payload=payload
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IP, ipv4(B4, A4, 6, tcp(
        B4, A4, 80, 40000, 5001, 1001 + len(payload), ACK
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1001 + len(payload), 5001, FIN | ACK
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IP, ipv4(B4, A4, 6, tcp(
        B4, A4, 80, 40000, 5001, 1002 + len(payload), FIN | ACK
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1002 + len(payload), 5002, ACK
    ))))
    pcap.close()


def write_udp_dns(path: Path) -> None:
    pcap = Pcap(path)
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, DNS4, 17, udp(
        A4, DNS4, 53000, 53, dns_query()
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IP, ipv4(DNS4, A4, 17, udp(
        DNS4, A4, 53, 53000, dns_response()
    ))))
    pcap.close()


def write_icmp_echo(path: Path) -> None:
    payload = b"raven-pf"
    pcap = Pcap(path)
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 1, icmp4(
        8, 0, 0x1234, 1, payload
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IP, ipv4(B4, A4, 1, icmp4(
        0, 0, 0x1234, 1, payload
    ))))
    pcap.close()


def write_ipv6_tcp(path: Path) -> None:
    payload = b"GET / HTTP/1.0\r\n\r\n"
    pcap = Pcap(path)
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, B6, 6, tcp(
        A6, B6, 40000, 80, 7000, 0, SYN, options=tcp_options(), family=6
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IPV6, ipv6(B6, A6, 6, tcp(
        B6, A6, 80, 40000, 9000, 7001, SYN | ACK, window=64240,
        options=tcp_options(), family=6
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, B6, 6, tcp(
        A6, B6, 40000, 80, 7001, 9001, ACK, family=6
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, B6, 6, tcp(
        A6, B6, 40000, 80, 7001, 9001, PSH | ACK, payload=payload, family=6
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, B6, 6, tcp(
        A6, B6, 40000, 80, 7001 + len(payload), 9001, FIN | ACK, family=6
    ))))
    pcap.close()


def write_ipv6_udp_icmp(path: Path) -> None:
    payload = b"raven6"
    pcap = Pcap(path)
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, DNS6, 17, udp(
        A6, DNS6, 53000, 53, dns_query(), family=6
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IPV6, ipv6(DNS6, A6, 17, udp(
        DNS6, A6, 53, 53000, dns_response(), family=6
    ))))
    pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IPV6, ipv6(A6, B6, 58, icmp6(
        A6, B6, 128, 0, 0x5678, 2, payload
    ))))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_IPV6, ipv6(B6, A6, 58, icmp6(
        B6, A6, 129, 0, 0x5678, 2, payload
    ))))
    pcap.close()


def write_fragmented_udp(path: Path) -> None:
    payload = bytes((index * 7) % 251 for index in range(3000))
    datagram = udp(A4, B4, 40000, 40000, payload)
    chunks = (datagram[:1480], datagram[1480:2960], datagram[2960:])
    ident = 0x1C46
    pcap = Pcap(path)
    for index, chunk in enumerate(chunks):
        offset = index * 1480
        flags_frag = (offset // 8) | (0x2000 if index < 2 else 0)
        pcap.add(ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(
            A4, B4, 17, chunk, ident=ident, flags_frag=flags_frag
        )))
    pcap.close()


def write_negative_fixtures(out: Path) -> None:
    """K281 additions: an unknown link type and a bad IPv4 header checksum."""
    negative = out / "negative"
    negative.mkdir(parents=True, exist_ok=True)

    header = struct.pack("<IHHIIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, 147)
    (negative / "unknown-linktype.pcap").write_bytes(header)

    payload = b"raven-pf"
    frame = bytearray(ethernet(
        B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 1, icmp4(8, 0, 0x1234, 1, payload))
    ))
    # Ethernet 14 + IPv4 checksum offset 10 = 24. Force a non-zero residual.
    frame[24] = 0xFF
    frame[25] = 0xFF
    pcap = Pcap(negative / "bad-checksum.pcap")
    pcap.add(bytes(frame))
    pcap.close()


def write_arp(path: Path) -> None:
    request = struct.pack(
        "!HHBBH6s4s6s4s",
        1,
        ETH_P_IP,
        6,
        4,
        1,
        A_MAC,
        socket.inet_aton(A4),
        b"\x00" * 6,
        socket.inet_aton(B4),
    )
    reply = struct.pack(
        "!HHBBH6s4s6s4s",
        1,
        ETH_P_IP,
        6,
        4,
        2,
        B_MAC,
        socket.inet_aton(B4),
        A_MAC,
        socket.inet_aton(A4),
    )
    pcap = Pcap(path)
    pcap.add(ethernet(BCAST_MAC, A_MAC, ETH_P_ARP, request))
    pcap.add(ethernet(A_MAC, B_MAC, ETH_P_ARP, reply))
    pcap.close()


def write_malformed_truncated(path: Path) -> None:
    frame = ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1000, 0, SYN, options=tcp_options()
    )))
    pcap = Pcap(path)
    pcap.add_record(frame[:20], 60, 60)
    pcap.close()


def write_malformed_bad_caplen(path: Path) -> None:
    frame = ethernet(B_MAC, A_MAC, ETH_P_IP, ipv4(A4, B4, 6, tcp(
        A4, B4, 40000, 80, 1000, 0, SYN, options=tcp_options()
    )))
    pcap = Pcap(path)
    pcap.add_record(frame, 70000, 70000)
    pcap.close()


GENERATORS = {
    "ipv4-tcp-handshake.pcap": write_tcp_handshake,
    "ipv4-udp-dns.pcap": write_udp_dns,
    "ipv4-icmp-echo.pcap": write_icmp_echo,
    "ipv6-tcp-http.pcap": write_ipv6_tcp,
    "ipv6-udp-icmp.pcap": write_ipv6_udp_icmp,
    "ipv4-fragmented-udp.pcap": write_fragmented_udp,
    "arp.pcap": write_arp,
    "malformed-truncated.pcap": write_malformed_truncated,
    "malformed-bad-caplen.pcap": write_malformed_bad_caplen,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for name, generator in GENERATORS.items():
        generator(args.out / name)
        print(f"wrote {args.out / name}")
    write_negative_fixtures(args.out)
    print(f"wrote {args.out / 'negative'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
