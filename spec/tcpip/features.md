# Feature decisions and smoltcp comparison

Status: normative decision table for C02-C27. The smoltcp column is an
observation, not a requirement: it records what the donor stack we are
replacing actually does, verified in its source at revision
`8b0e5c8598647fe92e2a1b87932d1932354fee0b` (directory
`ravenbsd-resources/smoltcp`, crate version 0.14.0). Every comparison carries
a `file:line` mark. A row marked `UNVERIFIED` may not be turned into a
requirement until it is checked.

Decision vocabulary is defined in `README.md` §2. `YES`, `PARTIAL` and `NO`
describe smoltcp only; they are not our decision.

## 1. Summary of the gaps that justify this stack

DR-29 names the gaps. The verified source reading adds detail and corrects two
README claims:

* smoltcp **does** generate SACK blocks from its reassembly state
  (`src/socket/tcp.rs:1582-1610`) and negotiates SACK-permitted
  (`:2007`, `:2702`), but it never processes SACK blocks received from a peer:
  there is no scoreboard and the only use of `sack_permitted` is the
  negotiation flag (`:2007`, `:2056`). The README's "Selective
  acknowledgements are not implemented" (`README.md:133`) is therefore coarse:
  the receiver advertises SACK, the sender ignores it.
* smoltcp **does** support TCP timestamps when a `tsval` generator is
  installed (`src/socket/tcp.rs:667-673`): it emits TSval, echoes TSecr
  (`:2664-2666`) and records the peer's TSval (`:2253-2256`), but it does not
  use the timestamp for RTT measurement and does not implement PAWS; the
  stored value is only echoed back. The README's "Timestamping is not
  supported" (`README.md:136`) is therefore also coarse.
* smoltcp's RTO is no longer just "starts at an estimate and doubles": it has
  an RFC 6298-style estimator with a 1 s floor and a 60 s cap
  (`src/socket/tcp.rs:143-159`, `:227-244`) and exponential backoff
  (`:278-283`). The README (`README.md:128`) is outdated.
* smoltcp has **both** CUBIC and Reno controllers
  (`src/socket/tcp/congestion.rs:1-100`), but they are optional features and
  the CUBIC implementation uses `f64` (`src/socket/tcp/congestion/cubic.rs:10-12`,
  `README.md:135`); ours is integer-only.
* smoltcp has **no** RACK-TLP, no PRR, no ECN processing, no PMTUD or PLPMTUD,
  no SWS avoidance, no listen backlog, no SYN cookies, no RFC 6528 ISN, no
  connection hash table and no counters. Each row below cites the place that
  proves the absence or the partial behaviour.

## 2. Link layer and ARP

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| Ethernet II parse/emit, unicast and broadcast | 894 | REQUIRED | C02 | YES | `src/wire/ethernet.rs:1-424`; `src/iface/interface/ethernet.rs:1-120` |
| ARP request/reply, cache | 826 | REQUIRED | C05 | YES | `src/wire/arp.rs`; `src/iface/neighbor.rs:45-60` |
| ARP cache expiry (60 s) and one-request-per-second rate limit | 826 | REQUIRED | C05 | YES | `src/iface/neighbor.rs:52-56` (`SILENT_TIME`, `ENTRY_LIFETIME`) |
| Gratuitous ARP accepted only to refresh an existing entry | 5227 | REQUIRED | C05 | PARTIAL: greedy source refresh exists, unconditional create is not the policy | `src/iface/neighbor.rs:70-95` |
| ARP probe retry with backoff, `EHOSTUNREACH` on failure | 826 | REQUIRED | C05 | PARTIAL: retry/backoff exists, error mapping is interface-specific | `src/iface/neighbor.rs:150-180` |
| VLAN 802.1Q, 802.3 frames | 894 | OUT-OF-SCOPE (no VLAN in M5N) | — | NO | `README.md:37` |
| Jumbo frames | 894 | OUT-OF-SCOPE | — | NO | `README.md:38` |

## 3. IPv4

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| IPv4 header parse/emit and header checksum | 791 | REQUIRED | C02, C05 | YES | `src/wire/ipv4.rs:543-553` (parse and checksum gate) |
| IPv4 options validated and skipped, never acted on | 791 | REQUIRED | C02, C05 | PARTIAL: "silently ignored" | `README.md:53`; `src/wire/ipv4.rs:543-553` |
| DF bit exposed; default DF policy is C15's | 791, 1191 | REQUIRED | C05, C15 | PARTIAL: fragment fields exposed, no PMTU policy | `src/wire/ipv4.rs:312-319`, `src/iface/interface/mod.rs:1297` |
| Outbound fragmentation to the interface MTU | 791 | REQUIRED | C05 | YES (feature-gated) | `src/iface/interface/mod.rs:1297`, `:1409`; `Cargo.toml:55` |
| Inbound reassembly with a byte cap, key cap and timeout | 791, 5722 | REQUIRED | C05 | YES (feature-gated) | `src/iface/fragmentation.rs:45-176`; `Cargo.toml:55` |
| Overlapping fragments discarded (RFC 5722 behaviour) | 5722 | REQUIRED | C05 | UNVERIFIED: assembler exists, overlap policy not traced | `src/iface/fragmentation.rs:133-176` |
| Longest-prefix route table, default route | 1812 | REQUIRED | C05 | YES | `src/iface/route.rs`; `src/iface/interface/mod.rs:858-860` |
| IPv4 broadcast/multicast receive | 1112 | OPTIONAL (unicast is the gate) | C05 | YES | `src/iface/interface/multicast.rs:1-560` |
| IPv4 header checksum recomputed on forwarding/emit | 791 | REQUIRED | C05 | YES | `src/wire/ipv4.rs:505-512` |

## 4. IPv6

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| IPv6 fixed header parse/emit | 8200 | REQUIRED | C02, C07 | YES | `src/wire/ipv6.rs:606-615` |
| Hop-by-hop extension header parse, skip, option validation | 8200 | REQUIRED | C07 | YES | `src/wire/ipv6ext_header.rs:134-154`; `src/iface/interface/ipv6.rs` |
| Fragment extension header and reassembly | 8200 | REQUIRED | C07 | YES (feature-gated) | `src/wire/ipv6fragment.rs`; `src/iface/fragmentation.rs`; `Cargo.toml:59` |
| Routing extension header | 8200 | OUT-OF-SCOPE: discard with ICMPv6 parameter problem if present | C07 | UNVERIFIED: wire type exists, forwarding path not traced | `src/wire/ipv6routing.rs:1-607` |
| Destination-options extension header | 8200 | OUT-OF-SCOPE: discard unknown options per RFC | C07 | PARTIAL: wire type exists | `src/wire/ipv6option.rs:1-735` |
| Neighbour solicitation/advertisement | 4861 | REQUIRED | C07 | YES | `src/iface/interface/ipv6.rs:457-491` |
| Router advertisement receive and SLAAC | 4861, 4862 | REQUIRED | C07 | YES | `src/iface/slaac.rs:82-230` |
| Router solicitation generation | 4861 | OPTIONAL | C07 | YES: generated, not read per README | `README.md` NDISC section; `src/wire/ndisc.rs:195-265` |
| Redirect messages | 4861 | OUT-OF-SCOPE: parse only, never act | C07 | PARTIAL: parsed, not acted on | `src/wire/ndisc.rs:284` |
| SLAAC address/prefix/route lifetimes | 4862 | REQUIRED | C07 | YES | `src/iface/slaac.rs:36-80`, `:186-230` |
| IPv6 multicast (solicited-node, all-nodes) | 4291 | REQUIRED for ND, no MLD | C07 | PARTIAL: interface multicast support, no MLD host | `src/iface/interface/multicast.rs:1-560`; `Cargo.toml:49-95` has no MLD socket |
| IPv6 jumbograms | 2675 | OUT-OF-SCOPE | — | NO | `src/wire/ipv6.rs:459-615` (payload length parsed, no jumbogram path) |
| IPv6 forwarding / routing protocols | 8200 | OUT-OF-SCOPE | — | NO | `src/iface/interface/ipv6.rs:1-753` (no forwarding path) |

## 5. ICMP

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| ICMPv4 echo request/reply, payload intact | 792 | REQUIRED | C06 | YES | `src/iface/interface/ipv4.rs:346-360` |
| ICMPv4 destination-unreachable input validation and delivery to the transport | 792, 1122 | REQUIRED | C05, C06 | PARTIAL: generated for a local unreachable protocol, not delivered to a socket | `src/iface/interface/ipv4.rs:244-245` |
| ICMPv4 time-exceeded and parameter-problem input | 792 | REQUIRED for PMTUD/error mapping | C05 | UNVERIFIED: no dispatch path found in `ipv4.rs` | `src/iface/interface/ipv4.rs:230-280` |
| ICMPv4 error generation (port unreachable, protocol unreachable) | 792 | REQUIRED | C06 | YES | `src/iface/interface/ipv4.rs:244` |
| ICMPv4 redirect processing | 792 | OUT-OF-SCOPE | C06 | NO | `src/iface/interface/ipv4.rs:230-280` (no redirect handling) |
| ICMPv6 echo request/reply | 4443 | REQUIRED | C07 | YES | `src/iface/interface/ipv6.rs:402-416` |
| ICMPv6 destination-unreachable, packet-too-big, time-exceeded | 4443 | REQUIRED | C07, C15 | PARTIAL: PTB is not fed to TCP; no path-MTU cache | `src/iface/interface/ipv6.rs:356-360`; `src/iface/interface/mod.rs:858-860` |
| ICMPv6 parameter problem generation | 4443 | REQUIRED | C07 | YES | `src/iface/interface/ipv6.rs:273-274`, `:356-357` |
| ICMPv6 ND (NS/NA/RA/RS) | 4861 | REQUIRED | C07 | YES | `src/iface/interface/ipv6.rs:457-510` |

## 6. UDP

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| UDP header parse/emit, pseudo-header checksum | 768 | REQUIRED | C02, C06 | YES | `src/wire/udp.rs:129-145` |
| IPv4 zero checksum accepted as "not computed" | 768 | REQUIRED | C06 | YES | `src/wire/udp.rs:135-138` |
| IPv6 zero checksum rejected | 8200 | REQUIRED | C06, C07 | **NO**: the zero check is family-independent and returns true for IPv6 too | `src/wire/udp.rs:129-140` |
| UDP length validation and truncation rejection | 768 | REQUIRED | C02, C06 | YES | `src/wire/udp.rs:129-145` |
| ICMP port-unreachable on no listener | 1122 | REQUIRED | C06 | YES | `README.md:105-108`; `src/iface/interface/udp.rs:1-79` |
| UDP checksum offload | 768 | OPTIONAL | C06 | YES | `src/wire/udp.rs:129-145`; device checksum caps in `src/phy/mod.rs` |
| UDP fragmentation/reassembly | 791/8200 | REQUIRED via IP layer | C05, C07 | YES | `src/iface/fragmentation.rs:45-176` |

## 7. TCP core and options

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| TCP header parse/emit and mandatory checksum | 9293 | REQUIRED | C02, C08 | YES | `src/wire/tcp.rs:898-917`, `:1032` |
| Option parsing: EOL, NOP, MSS, WS, SACK-permitted, SACK blocks, timestamps, unknown lengths | 9293, 7323, 2018 | REQUIRED | C02 | YES | `src/wire/tcp.rs:678-762` |
| MSS option, negotiation and effective MSS after options | 9293, 6691 | REQUIRED | C11 | YES | `src/socket/tcp.rs:1993-1996`, `:2406-2407` |
| MSS floor and ceiling | 6691 | REQUIRED | C11 | PARTIAL: floor 48 bytes, no configured ceiling | `src/socket/tcp.rs:598`, `:1993` |
| Window scale negotiation, SYN window unscaled, shift from buffer | 7323 | REQUIRED | C11 | YES | `src/socket/tcp.rs:2008`, `:2701-2705`, `:809-810` |
| Timestamps: SYN negotiation, TSval/TSecr, echo under reordering | 7323 | REQUIRED | C11 | PARTIAL: optional generator and echo, no RTTM | `src/socket/tcp.rs:667-673`, `:2253-2256`, `:2664-2666` |
| PAWS (reject stale TSval, 24-day exception) | 7323 | REQUIRED | C11 | **NO**: `last_remote_tsval` is stored and echoed, never compared | `src/socket/tcp.rs:573-577`, `:2253-2256` |
| SACK-permitted negotiation | 2018 | REQUIRED | C12 | YES | `src/socket/tcp.rs:2007`, `:2702` |
| SACK block generation on receive (hole-first) | 2018 | REQUIRED | C12 | PARTIAL: generated, only one block, no block-count reduction for timestamps | `src/socket/tcp.rs:1582-1610` |
| SACK scoreboard, `HighRxt`, `RescueRxt`, `Pipe`, `IsLost`, `NextSeg` | 6675 | REQUIRED | C12 | **NO**: received SACK blocks are not parsed into state | `src/socket/tcp.rs:2007`, `:2056`; no `Pipe`/`NextSeg` symbol in `src/` |
| DSACK | 2883 | REQUIRED | C12 | **NO** | `src/socket/tcp.rs:1582-1610` has no duplicate-block path |
| ECN negotiation (ECT, ECE, CWR) | 3168 | REQUIRED | C14 | **NO**: flags are parsed by the wire only | `src/wire/tcp.rs:116-117`, `:250-263`; `src/socket/tcp.rs` never calls `set_ece`/`set_cwr` |
| Accurate ECN | 9331 | OUT-OF-SCOPE | — | NO | `src/wire/tcp.rs:116-117` |
| Urgent pointer: parse, deliver out-of-band, never silently ignore | 9293 | REQUIRED (parse + documented delivery) | C09 | PARTIAL: parsed, then explicitly ignored | `src/wire/tcp.rs:296-306`, `:934` |
| TCP Fast Open | 7413 | OUT-OF-SCOPE | — | NO | no TFO option in `src/wire/tcp.rs:673-760` |
| TCP MD5 signature | 2385 | OPTIONAL | C18 | NO | no option kind 19 in `src/wire/tcp.rs:673-760` |
| TCP-AO | 5925 | OPTIONAL | C18 | NO | no option kind 29 in `src/wire/tcp.rs:673-760` |
| MPTCP | 8684 | OUT-OF-SCOPE | — | NO | no MPTCP option in `src/wire/tcp.rs:673-760` |

## 8. TCP behaviour: opening, closing, politeness

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| Active and passive open, three-way handshake | 9293 | REQUIRED | C08 | YES | `src/socket/tcp.rs:1027-1065`, `:1098-1150` |
| Simultaneous open | 9293 | REQUIRED | C08 | YES | `src/socket/tcp.rs:1740-1750`, `:2068` |
| Simultaneous close | 9293 | REQUIRED | C08 | YES | `src/socket/tcp.rs:2093-2100` |
| Eleven states, exact transitions | 9293 | REQUIRED | C08 | YES (state enum only; some RFC 5961 rows are partial) | `src/socket/tcp.rs:120-138`, `:1960-2135` |
| RST validation, exact-sequence rule | 5961 | REQUIRED | C08 | YES | `src/socket/tcp.rs:1875-1885` |
| Challenge ACK for in-window RST, SYN, bad ACK | 5961 | REQUIRED | C08 | PARTIAL: challenge ACK exists for RST/SYN, with a 1 s per-socket limit; no RFC 5961 ACK-window check | `src/socket/tcp.rs:1624-1635`, `:1786`, `:1904-1915` |
| RST never sent in reply to a RST | 9293 | REQUIRED | C08 | YES | `src/socket/tcp.rs:1966` |
| RST for closed port / no connection | 9293 | REQUIRED | C08 | YES | `src/socket/tcp.rs:2685-2690` |
| TIME-WAIT, fixed interval | 9293 | REQUIRED | C08 | YES, 10 s hardcoded | `src/socket/tcp.rs:325-326`, `:425-428` |
| TIME-WAIT reuse per RFC 6191 | 6191 | REQUIRED | C17 | NO: only the 10 s timer, no reuse rule | `src/socket/tcp.rs:425-428`, `:1880-1885` |
| FIN-WAIT-2 timeout | 1122 | REQUIRED | C08 | **NO**: no close timer is armed in FIN-WAIT-2 | `src/socket/tcp.rs:2085-2110` (no `set_for_close`) |
| Keepalive timer, interval and probe count | 1122 | REQUIRED | C08 | YES, interval only; probe count not exposed | `src/socket/tcp.rs:335-340`, `:927` |
| User timeout | 1122 | OPTIONAL | C08 | YES | `src/socket/tcp.rs:843`, `:2379-2385` |
| Delayed ACK, configurable delay, immediate ACK on out-of-order and second full segment | 1122 | REQUIRED | C09 | YES, 10 ms default; second-full-segment rule not verified | `src/socket/tcp.rs:325`, `:648`, `:2462` |
| Nagle algorithm, default on, configurable off | 1122 | REQUIRED | C09 | YES | `src/socket/tcp.rs:567-568`, `:892-905`, `:2444-2449` |
| SWS avoidance, sender and receiver | 1122, 813 | REQUIRED | C09 | **NO** | `README.md:134`; no sender-window floor in `src/socket/tcp.rs:2400-2500` |
| Zero-window probing with backoff | 9293, 6429 | REQUIRED | C09 | YES | `src/socket/tcp.rs:359-362`, `:431-442` |
| Out-of-order reassembly with a bounded hole count | 9293 | REQUIRED | C09 | YES, 4 or 32 gaps depending on storage | `README.md:126`; `src/storage/assembler.rs` |
| Data on SYN accepted and queued | 9293 | OPTIONAL (C08 keeps SYN data out of scope) | C09 | YES | `src/socket/tcp.rs:1960-2070` |

## 9. TCP recovery and congestion control

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| RFC 6298 SRTT/RTTVAR estimator, 1 s floor, 60 s cap, clock granularity | 6298 | REQUIRED | C10 | YES (floor 1 s, cap 60 s) | `src/socket/tcp.rs:143-159`, `:227-244` |
| Karn's algorithm (no RTT sample from a retransmitted segment) | 6298 | REQUIRED | C10 | YES: the sample timestamp is cleared on retransmit | `src/socket/tcp.rs:255-302` |
| Exponential RTO backoff with cap | 6298 | REQUIRED | C10 | YES | `src/socket/tcp.rs:278-283` |
| Retransmit oldest unacknowledged segment on timeout | 9293 | REQUIRED | C10 | YES | `src/socket/tcp.rs:2388-2395` |
| Fast retransmit after three duplicate ACKs, configurable threshold | 5681 | REQUIRED | C13 | PARTIAL: threshold is a literal 3 | `src/socket/tcp.rs:2204-2205` |
| Fast recovery | 5681 | REQUIRED | C13 | PARTIAL: Reno-style; no NewReno partial-ACK handling | `src/socket/tcp/congestion/reno.rs:46-80` |
| NewReno `recover` variable and partial-ACK retransmission | 6582 | REQUIRED | C13 | **NO**: first new-data ACK exits recovery | `src/socket/tcp/congestion/reno.rs:46-56` |
| CUBIC (RFC 8312 curve) | 8312 | REQUIRED | C13 | PARTIAL: implements the newer RFC 9438 variant with `f64`; not checked against the 8312 vectors | `src/socket/tcp/congestion/cubic.rs:1-90`; `README.md:135` |
| PRR | 6937 | REQUIRED | C13 | **NO** | `src/socket/tcp/congestion.rs:1-106` (only None/Reno/Cubic controllers, no PRR module) |
| Initial window of 10 MSS (RFC 6928) | 6928 | REQUIRED | C13 | **NO**: Reno starts at 2 MSS | `src/socket/tcp/congestion/reno.rs:25` |
| Appropriate byte counting | 3465 | REQUIRED | C13 | PARTIAL: `len.min(mss)` in slow start, `mss*mss/cwnd` in CA | `src/socket/tcp/congestion/reno.rs:59-66` |
| cwnd cap and idle restart | 5681 | REQUIRED | C13 | PARTIAL: cap to receive window only; no idle restart | `src/socket/tcp/congestion/reno.rs:67`; no idle logic in `src/socket/tcp.rs` |
| RACK loss detection | 8985 | REQUIRED | C14 | **NO** | `src/socket/tcp.rs:501-577` (socket state has no RACK fields) |
| TLP (tail loss probe) | 8985 | REQUIRED | C14 | **NO** | `src/socket/tcp.rs:501-577` (no PTO/TLP state) |
| Spurious retransmission undo (Eifel/DSACK) | 3522, 2883 | REQUIRED | C14 | **NO** | `src/socket/tcp/congestion/reno.rs:46-80` (no undo/restore path) |
| Classic PMTUD (ICMP frag-needed / packet-too-big) | 1191, 8201 | REQUIRED | C15 | **NO**: TCP uses `cx.ip_mtu()` from the interface, no per-path cache | `src/socket/tcp.rs:2406`, `:2725`; `src/iface/interface/mod.rs:858-860` |
| PLPMTUD probing state machine | 8899 | REQUIRED | C15 | **NO** | no probe state in `src/socket/tcp*`; `README.md:139` |
| MSS/PMTU floor per family | 1191, 8201 | REQUIRED | C15 | PARTIAL: `ip_mtu()` only, no path cache | `src/socket/tcp.rs:2406` |
| ECN congestion response, once per RTT | 3168 | REQUIRED | C14 | **NO** | `src/socket/tcp.rs:2660-2710` (segment emission sets no ECE/CWR) |

## 10. Listen path, security and scale

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| Separate half-open and accept queues | 9293, 4987 | REQUIRED | C16 | **NO**: a listener is a socket in `Listen`; no queue | `src/socket/tcp.rs:1027-1055` |
| Half-open limit and global listener limit | 4987 | REQUIRED | C16 | **NO** | `src/socket/tcp.rs:1027-1055` (listener is one socket, no queue state) |
| SYN cookies with rotated secret | 4987 | REQUIRED, default on | C16 | **NO** | `src/socket/tcp.rs:1098-1161` (connect chooses a random ISS; no cookie path) |
| RFC 6528 ISN generation (keyed hash + clock) | 6528 | REQUIRED | C16 | **NO**: random `u32` only | `src/socket/tcp.rs:1161` |
| ISN strength tunable, deterministic test mode is not the default | 6528 | REQUIRED | C16 | **NO** | `src/socket/tcp.rs:1161` |
| Connection classification by hash, O(1) | — | REQUIRED | C17 | **NO**: an array walked linearly per packet | `src/iface/socket_set.rs:44`, `:133-149`; `src/iface/interface/tcp.rs:34-40` |
| Ephemeral port randomisation, range, exhaustion | 6056 | REQUIRED | C17 | **NO**: the caller supplies the local port; no allocator | `src/socket/tcp.rs:1068-1075` |
| TIME-WAIT held in a separate bounded structure, O(1) expiry | 1337 | REQUIRED | C17 | **NO**: the socket itself is the state | `src/socket/tcp.rs:425-442` |
| Rebind matrix (documented) | 1122 | REQUIRED | C17 | PARTIAL: `listen` on the same endpoint is idempotent, nothing else documented | `src/socket/tcp.rs:1040-1050` |

## 11. Observability, utilities and out-of-scope protocols

| Feature | RFC | Decision | Ticket | smoltcp | Evidence (`file:line`) |
|---|---|---|---|---|---|
| Counter set (MIB-style and internal) | 4022 | REQUIRED | C19 | **NO**: no counter type or increment in the TCP socket | `src/socket/tcp.rs:501-577` (per-connection state only) |
| Per-socket info (state, queues, cwnd, srtt, rto, options) | — | REQUIRED | C19 | **NO** | `src/socket/tcp.rs:501-577` (no `SocketInfo`/`socket_info`) |
| Typed tunable property table with min/max/default/unit/stability | — | REQUIRED | C19 | **NO**: only ad-hoc setters (`set_timeout`, `set_ack_delay`, `set_keep_alive`, `set_nagle_enabled`) | `src/socket/tcp.rs:843-935` |
| DHCPv4 client | 2131 | OUT-OF-SCOPE for C02-C27; a later netd ticket may add it | — | YES | `src/socket/dhcpv4.rs`; `Cargo.toml:78` |
| DNS stub client | 1035 | OUT-OF-SCOPE (userspace) | — | YES | `src/socket/dns.rs`; `Cargo.toml:79` |
| Raw IP sockets | 791 | OUT-OF-SCOPE | — | YES | `src/socket/raw.rs`; `Cargo.toml:73` |
| IGMP / multicast group management | 2236 | OUT-OF-SCOPE | — | YES (IPv4), no MLD host | `src/wire/igmp.rs:1-447`; `src/iface/interface/multicast.rs:1-560` |
| 6LoWPAN / IEEE 802.15.4 | 4944, 6282 | OUT-OF-SCOPE | — | YES | `src/wire/sixlowpan/`, `src/wire/ieee802154.rs`; `Cargo.toml:49` |

## 12. Rows deliberately marked UNVERIFIED

These rows are informational only and may not become requirements until they
are checked in the named source:

* IPv4 overlapping-fragment handling (`src/iface/fragmentation.rs:133-176`):
  the assembler exists and rejects some overlaps, but the exact RFC 5722
  policy was not traced line by line.
* IPv6 routing-extension processing (`src/wire/ipv6routing.rs`): the wire type
  exists; whether the interface skips it or rejects it was not traced.
* ICMPv4 time-exceeded/parameter-problem dispatch
  (`src/iface/interface/ipv4.rs:230-280`): generation paths were found, but no
  delivery-to-socket path was confirmed.
* The second-full-sized-segment delayed-ACK rule
  (`src/socket/tcp.rs:2462-2470`): the timer exists; the exact rule was not
  traced.
* Gratuitous-ARP creation policy (`src/iface/neighbor.rs:70-95`): refresh
  behaviour was seen; the "only refresh an existing entry" rule was not
  confirmed.

## 13. Accepted deviations from the smoltcp comparison

None yet. C24 owns this section: it appends a row here for every
`DEVIATION-ACCEPTED` in `oracle-results.md`, next to the feature it affects.
The only deviations in this revision are the local choices in
`state-machine.md` §15, not deviations from smoltcp.
