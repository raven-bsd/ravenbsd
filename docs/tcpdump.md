<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# tcpdump (RavenBSD)

Status: K281. Offline classic-pcap decoder for the K277 fixture matrix. **OBSERVED** on the
host fixture suite; the binary is not staged in a boot profile yet.

`raven-tcpdump` ports the behaviour of FreeBSD `usr.sbin/tcpdump` for the packets the pf/tooling
gate needs: pcap open/read, Ethernet/ARP/IPv4/IPv6 decode, TCP/UDP/ICMP and the BPF-style filter
expressions used by the fixtures. It is self-contained Rust (no C) and does not depend on
unpublished internals.

## Usage

```sh
raven-tcpdump -nn -tt -r capture.pcap
raven-tcpdump -nn -tt -v -r ipv4-fragmented-udp.pcap
raven-tcpdump -nn -tt -r capture.pcap 'tcp port 80'
raven-tcpdump -nn -c 10 -w filtered.pcap -r capture.pcap 'src host 10.0.0.5'
```

Options:

| Option | Behaviour |
|---|---|
| `-r FILE` | Read a classic pcap savefile (little/big endian, micro/nanosecond). |
| `-w FILE` | Write the packets that pass the filter as a little-endian classic pcap. |
| `-c N` | Stop after `N` matching packets. |
| `-n`, `-nn` | Numeric addresses and ports (the only implemented mode). |
| `-v`, `-vv` | Verbose IP header/checksum display; `-vv` also prints TCP sequence/ack for empty segments. |
| `-t`, `-tt` | `-t` suppresses the timestamp; `-tt` prints Unix seconds and the savefile fraction. |
| `-S` | Absolute TCP sequence numbers. |
| `-i IFACE` | Accepted and rejected with a clear error: the userspace tool has no BPF capture device. Use `-r`. |
| `--` | End options; the rest is the filter expression. |

Exit codes: `0` on a decoded savefile (including packets with a bad checksum marker), `1` for a
usage error, a malformed/unsupported savefile, a bad filter or an unreadable input/output file.

## Decode subset

* Ethernet II and 802.1Q VLAN tags; ARP request/reply for Ethernet/IPv4.
* IPv4 with IHL, total length, fragmentation (`offset`, `[+]`, `[DF]`), verbose header and
  header-checksum marker; first fragments are handed to TCP/UDP, later fragments print
  `ip-proto-N`.
* IPv6 with hop-by-hop, routing, destination, fragment and authentication extension headers.
* TCP flags/options (`mss`, `sackOK`, `wscale`, `nop`, `eol`, `TS`, `sack`), relative sequence
  numbers and the `HTTP:` payload summary used by the fixtures.
* UDP with the DNS query/answer subset (`A`/`AAAA`) on ports 53/5353.
* ICMPv4 and ICMPv6 echo request/reply.

Not implemented: pcapng, live capture, service-name lookup, the full tcpdump protocol printer
catalogue, IP options beyond display, and transport checksum verification (the IPv4 header
checksum is checked in verbose mode). These are open gaps, not silent fallbacks.

## Filter grammar

```text
expr      := or
or        := and ("or" and)*
and       := unary ("and"? unary)*
unary     := "not" unary | "(" expr ")" | primitive
primitive := tcp | udp | icmp | icmp6 | ip | ip6 | arp
           | ["src"|"dst"] "host" ADDRESS
           | ["src"|"dst"] "port" NUMBER
           | PROTO "[" OFFSET [":" SIZE] "]" ["&" MASK] CMP VALUE
```

`OFFSET` accepts `tcpflags`, `icmptype`, `icmpcode` or a number; `VALUE`/`MASK` accept decimal,
`0x` hex and the named constants `tcp-fin/syn/rst/push/ack/urg`, `icmp-echo/echoreply` and
`icmp6-echo/echoreply`. The fixtures exercise `tcp`, `udp port 53`, `icmp`, `icmp6`, `ip6 and
tcp`, `src host`, and `tcp[tcpflags] & tcp-syn != 0`, `icmp[icmptype] == icmp-echo`,
`ip[6:2] & 0x1fff != 0`, `ip[6:2] & 0x2000 != 0`.

## Donor reconciliation

FreeBSD donor revision `133344405f532bd54ff3629a77d4c4e062f01fdb`:

| Topic | Donor files/functions | Licence finding | Raven decision |
|---|---|---|---|
| pcap read/write | `contrib/libpcap/sf-pcap.c` `pcap_check_header` :219, `pcap_next_packet` :518, `pcap_dump` :820; `contrib/libpcap/savefile.c` `pcap_fopen_offline_with_tstamp_precision` :480, `pcapint_offline_read` :627 | `LICENSE` says BSD-3, the read per-file headers are 4-clause (advertising/acknowledgement) | clean-room reader/writer |
| CLI/loop | `contrib/tcpdump/tcpdump.c` `main` :1520, `read_infile` :1112, `dump_packet` :3412, `print_usage` :3542 | 4-clause | clean-room CLI |
| IPv4/IPv6 | `contrib/tcpdump/print-ip.c` `ip_print` :317, `print-ip6.c` `ip6_print` :224 | 4-clause | clean-room |
| TCP/UDP/ICMP/ARP | `contrib/tcpdump/print-tcp.c` `tcp_print` :160; `print-udp.c` `udp_print` :366; `print-icmp.c` `icmp_print` :312; `print-arp.c` `arp_print` :346 | 4-clause | clean-room |
| filter grammar | `contrib/libpcap/grammar.y`; `contrib/libpcap/gencode.c` `gen_cmp` :501, `gen_hostop` :540, `gen_portrangeatom` :566 | 4-clause | clean-room recursive-descent parser |

No CDDL/GPL code was read or copied. The NetBSD full-tree copy
(`external/bsd/tcpdump/dist`, revision `36eddebcc36cb0775aa2962005b9c05419a6ff01`) carries the
same upstream per-file grant and was used only as a corroborating behavioural reference. The
fixtures are first-party BSD-3-Clause generated data (`tests/fixtures/tcpdump/**`); their
`.tcpdump` references were produced by Linux `tcpdump 4.99.1` / `libpcap 1.10.1` and are the
compatibility oracle. No `PROVENANCE.toml` import entry applies because no donor source was
copied.

## Evidence

The K281 report (`tickets/tim/reports/K281.md`) maps requirements to the unit/oracle tests, the
test-first red run, the mutation check and the gate logs. Fixture SHA-256 values are pinned in
`tests/fixtures/tcpdump/manifest.tsv`; the negative cases are
`malformed.tsv` (truncated frame, bad caplen) and `negative/negatives.tsv` (unknown link type,
bad IPv4 checksum).
