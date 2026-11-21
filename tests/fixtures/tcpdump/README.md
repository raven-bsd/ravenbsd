<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# tcpdump pcap fixtures

Deterministic pcap files for the K274 tcpdump decoder and the K277 conformance
matrix. Regenerate with:

```sh
timeout 30 python3 tests/fixtures/tcpdump/generate.py --out /tmp/pf-pcaps
```

Every timestamp, MAC address, sequence number and payload is fixed, so the
committed bytes are reproducible. `manifest.tsv` pins the packet count and
SHA-256 of each file. `decode.tsv` and `filters.tsv` are checked against the
host `tcpdump` by `tests/pf-conformance/check_fixtures.py`.

The `.tcpdump` files are the actual reference output from Linux
`tcpdump 4.99.1` / `libpcap 1.10.1`:

```sh
tcpdump -nn -tt -r ipv4-tcp-handshake.pcap
tcpdump -nn -tt -v -r ipv4-fragmented-udp.pcap
```

| File | Packets | Coverage |
|---|---:|---|
| `ipv4-tcp-handshake.pcap` | 8 | SYN options, SYN-ACK, ACK, payload, FIN teardown |
| `ipv4-udp-dns.pcap` | 2 | DNS query/response |
| `ipv4-icmp-echo.pcap` | 2 | ICMP echo request/reply |
| `ipv6-tcp-http.pcap` | 5 | IPv6 TCP handshake, payload, FIN |
| `ipv6-udp-icmp.pcap` | 4 | IPv6 UDP DNS and ICMPv6 echo |
| `ipv4-fragmented-udp.pcap` | 3 | UDP split 1480/1480/48 with MF and offsets |
| `arp.pcap` | 2 | ARP request/reply |

Negative fixtures in `malformed.tsv` must be rejected, not decoded:
`malformed-truncated.pcap` declares a 60-byte record but carries 20 bytes, and
`malformed-bad-caplen.pcap` declares 70000 bytes, above the 65535 snapshot
length. `check_fixtures.py` validates the parser error and `tcpdump` must exit
non-zero for both.

K281 adds `negative/` (see `negative/negatives.tsv`): `unknown-linktype.pcap`
must fail with `unsupported link type`, and `bad-checksum.pcap` must decode with
a visible `bad cksum` marker. `generate.py` writes both. `filters.tsv` also pins
a filter that must reject (`tcp port 9999` -> 0 packets). This fixture set is
imported from the K277 branch (`d1ba36af`) and is maintained under K281's
`tests/fixtures/tcpdump/**` Owns.

Donor reference for the decoder behaviour: NetBSD
`external/bsd/tcpdump/dist/{tcpdump.c,print-ip.c,print-ip6.c,print-tcp.c,print-udp.c,print-icmp.c,print-arp.c,netdissect.h}`
at `36eddebcc36cb0775aa2962005b9c05419a6ff01`. The pcaps are first-party
generated data; no donor code is copied.
