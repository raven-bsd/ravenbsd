<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# tcpdump conformance (K299)

Status: host conformance harness over the merged K281 `raven-tcpdump`. **OBSERVED** by
`sh tests/tcpdump-conformance/run.sh`; no product source is changed.

## What it proves

`tests/tcpdump-conformance/run.sh` builds `raven-tcpdump` and checks it against the K277 fixture
oracle:

1. every `manifest.tsv` SHA-256, link type and packet count;
2. every reference decode in `*.tcpdump` by exact `diff -u`;
3. every `filters.tsv` expression and expected packet count;
4. the `malformed.tsv` captures (truncated frame, bad record lengths) fail with the declared
   reason;
5. the K281 negative fixtures (unknown link type, bad IPv4 checksum marker) behave as declared;
6. `-c 3` and a filter that must reject (`tcp port 9999`).

The harness is fail-fast per check but continues the matrix, printing `sha256 ok`,
`filter ok`, `malformed ok` and `negative ok` lines; it exits 1 with a failure count at the end.

## Fixture hash matrix

| File | SHA-256 |
|---|---|
| `arp.pcap` | `3dc01b07e3a95722baa15e56703a53455c5a459b7b781d1cb09d33f05a5a0044` |
| `ipv4-tcp-handshake.pcap` | `15465803371a264dffed11f36754fd5c8741de3f2183f8c28baf329f0d33fa73` |
| `ipv4-udp-dns.pcap` | `a456558d615010a509b852cfdf6a86850fca647a119b637744d42ddab01f4ab2` |
| `ipv4-icmp-echo.pcap` | `458eae668d63eed61ec8ef932f48d69bc18c791e96d5f3bd7e4da78466f79cfd` |
| `ipv6-tcp-http.pcap` | `64c086d78663e0d69992211244f6e3d2730381777f4767f97e269185bb068979` |
| `ipv6-udp-icmp.pcap` | `a99d543bb1620bc463bdd831e101b53d80b0ecee746b6411bfeef859d19ddcb4` |
| `ipv4-fragmented-udp.pcap` | `96cd79c2a690d5ea410f2b657e8d610fc409a64381db5c4653bde46d66a6f0fb` |

Negative additions: `negative/unknown-linktype.pcap`
`327df82944f79c75b1e8828971033757dfc411bdcae408e942afbcf3a08c60bc`,
`negative/bad-checksum.pcap`
`ed251416c8322ad02a7e473c4c82c93f971e8b7c87e7d5e9815b586826311ac6`.

## Coverage

| Area | Fixtures / cases |
|---|---|
| ARP | `arp.pcap` request/reply |
| IPv4 TCP | handshake, options, payload, FIN; `-c`, reject filter |
| IPv4 UDP/DNS | query/response |
| IPv4 ICMP | echo request/reply |
| IPv6 TCP/UDP/ICMP | `ipv6-tcp-http.pcap`, `ipv6-udp-icmp.pcap` |
| fragmentation | `ipv4-fragmented-udp.pcap` with `-v` |
| filters | `tcp`, `tcp port 80`, `tcp[tcpflags] & tcp-syn != 0`, `src host`, `ip6`, `udp port 53`, `icmp`, `icmp[icmptype]`, `ip6 and tcp`, `ip6 and udp`, `icmp6`, `ip[6:2] & 0x1fff`, `ip[6:2] & 0x2000`, `arp`, reject |
| malformed | truncated frame, bad record lengths, unknown link type |
| checksum | bad IPv4 header checksum marker |

## Donor rules

Test-only. The behaviour under test is the K281 clean-room implementation of FreeBSD
`contrib/tcpdump`/`contrib/libpcap` at `133344405f532bd54ff3629a77d4c4e062f01fdb`; those per-file
headers are 4-clause, so no donor code is copied. The K277 fixtures are first-party BSD-3-Clause
generated data; Linux `tcpdump 4.99.1` produced the reference text only. No `PROVENANCE.toml`
entry applies.

## Running

```sh
timeout 300 sh tests/tcpdump-conformance/run.sh
timeout 420 make check-fast
```

The harness uses the standard host tools `cargo`, `sha256sum`, `od`, `awk`, `grep`, `diff` and
`mktemp`; temporary files are removed on exit. The evidence table and command transcripts are in
`tickets/tim/reports/K299.md`.
