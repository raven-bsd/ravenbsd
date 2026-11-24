<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP pcap regression suite (K342)

Host-only regression for the committed tcpdump fixtures and the K323
loss/reordering captures. The suite decodes every pcap with an independent
Python parser and checks the same bytes with the host `tcpdump 4.99.1`; a
future TCP/RACK change that silently changes a field, payload, filter result or
retransmission count fails the run. No product code is changed.

```text
timeout 300 sh tests/tcp-pcap-regression/run.sh
```

The runner is `tests/tcp-pcap-regression/run.py`. It fails closed on a missing
pcap, a truncated/bad-caplen record, an unknown link type, a filter error or any
changed outcome. The K323 captures are committed as `.pcap.hex` (the binary
pcaps are materialized under `/tmp/k342-fixtures`), with `SHA256SUMS` over the
decoded bytes; K323 kept the originals in `/tmp` only.

## Matrix

| Case | Source | Assertion |
|---|---|---|
| handshake | `tests/fixtures/tcpdump/ipv4-tcp-handshake.pcap` | 8 TCP frames; SYN, SYN-ACK, ACK, 18-byte `GET / HTTP/1.0`, FIN/FIN; `tcp`=8, `tcp port 80`=8, SYN filter=2, `tcp port 9999`=0 |
| ipv6 | `tests/fixtures/tcpdump/ipv6-tcp-http.pcap` | 5 TCP frames; SYN/SYN-ACK/HTTP/FIN; `ip6 and tcp`=5 |
| single-loss | `fixtures/k323/single-loss.pcap.hex` | 2 data frames, 1 retransmit, exact 44-byte payload |
| multiple-loss | `fixtures/k323/multiple-loss.pcap.hex` | 3 data frames, 2 retransmits, exact payload |
| dup-ack | `fixtures/k323/dup-ack.pcap.hex` | 2 data frames, at least 1 retransmit |
| fast-retransmit | `fixtures/k323/fast-retransmit.pcap.hex` | 2 data frames, at least 1 retransmit |
| reordering | `fixtures/k323/reordering.pcap.hex` | 1 data frame, no retransmit after the stale ACK |
| rto | `fixtures/k323/rto.pcap.hex` | 2 data frames, 1 retransmit |
| manifest | `fixtures/k323/SHA256SUMS` | all six pcap hashes match |
| malformed | `tests/fixtures/tcpdump/malformed-*.pcap`, `negative/unknown-linktype.pcap` | parser rejects; tcpdump exits non-zero for truncated/bad-caplen |
| bad checksum | `tests/fixtures/tcpdump/negative/bad-checksum.pcap` | parser reports the IP checksum invalid; `tcpdump -vv` prints `bad cksum` |
| negative | missing path + mutated payload | both rejected, never a pass |

## K323 pcap hashes

| Scenario | SHA-256 |
|---|---|
| single-loss | `8cfff7706a5f3b1d40145c174d96cf6913f64f14fa7b5542ab32fb228aa0c098` |
| multiple-loss | `555193a78402317626356489079c04b6419b880071cfa1483eff8c84f63f0bb8` |
| dup-ack | `78c76478423e0a7795b44d299802757d183d5bea5d8ceb873b3426160887eaab` |
| fast-retransmit | `9d839c43535df9346091ac335a1d50e6114827e2cc6a087e4774914aca31d338` |
| reordering | `970029fc69d2b885695bdbb66b8f3e2cf5a2622c9a328ecfffe870420054a61d` |
| rto | `a5a807a5b4e4bed7aebda349b188a03237cfa177851d29eaf6794568af3241e4` |

The complete list is `tests/tcp-pcap-regression/fixtures/k323/SHA256SUMS`.

## Reference behaviour

The decode fields and filters follow the K281 `decode.tsv`/`filters.tsv`
reference (host `tcpdump 4.99.1`/`libpcap 1.10.1` for NetBSD
`external/bsd/tcpdump/dist` behaviour). The K323 outcomes follow
`tests/net-conformance/loss/verify.py`: exact 44-byte payload, server port
23090, and repeated sequence numbers for retransmissions. No donor code is
copied or imported.

## Mutation / negative proof

* A missing pcap path raises `PcapError` before any pass is printed.
* A copy of `single-loss.pcap` with one payload byte flipped fails the data
  assertion.
* Truncated/bad-caplen fixtures are rejected by the independent parser and by
  `tcpdump`.
* Unknown link type is rejected with `unsupported link type`.
* `bad-checksum.pcap` still decodes but the parser marks the checksum invalid
  and `tcpdump -vv` prints `bad cksum`.
