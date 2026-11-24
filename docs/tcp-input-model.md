# TCP input model and property suite

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only evidence tooling for the K282 TCP input port. The package is
`tests/tcp-input-model` (`cargo nextest run -p tcp-input-model`); it never edits product source.

## Model

`src/model.rs` is a clean-room model of the RFC 9293 / RFC 5961 input rules, independent of
`raven-tcpip`. `src/harness.rs` drives the real stack through the same cases:

| Case | Model rule | Real-stack check |
|---|---|---|
| SYN to a listener | `Listen -> SynReceived`, SYN|ACK | half-open exists, SYN|ACK traced |
| Exact duplicate SYN | retransmit SYN|ACK | SYN|ACK traced, state kept |
| Duplicate SYN with another sequence | ACK, never close | state kept, bare ACK traced |
| SYN below IRS (LAND) | `tcp_dropwithreset`: RST|ACK | RST|ACK traced, state kept |
| SYN_RECEIVED final ACK | valid ACK per `snd_una < ACK <= snd_max` -> ESTABLISHED | state and accept queue |
| SYN_RECEIVED final ACK payload | payload advances `rcv_nxt` | accepted socket receives the bytes |
| Bad ACK in SYN_RECEIVED | RST, state kept | RST traced |
| RST at `last_ack_sent`/`RCV.NXT` | close, `ECONNRESET` (SYN_RECEIVED: `ECONNREFUSED`) | socket error |
| RST in window | challenge ACK | state kept |
| RST out of window | ignore | state kept |
| SYN in a synchronized state | challenge ACK | state kept |
| PAWS stale timestamp | drop with ACK within 24 days | `paws_drops`, no data |
| Data beyond `RCV.NXT + RCV.WND` | ACK and drop | no data delivered |
| Sequence wraparound | wrapping `SEQ_*` arithmetic | handshake and data cross `u32::MAX` |

The randomized property (`random_input_invariants`) asserts that a non-accepted RST never changes
state, a non-land duplicate SYN never closes a half-open, an out-of-range SYN_RECEIVED ACK never
establishes, `data_bytes` never decreases, and `RCV.NXT` never moves backwards.

## Donors

FreeBSD `sys/netinet/tcp_input.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause) is
the behavioural donor; Illumos `tcp_input.c` is read for behaviour (CDDL) and Linux
`net/ipv4/tcp_input.c` for outcome (GPL) only. No donor expression is copied into product code.

## Running

```text
cargo nextest run -p tcp-input-model
```
