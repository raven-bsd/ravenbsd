<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Packet-level TCP conformance harness (K253)

`tests/net-conformance/harness.py` drives the tested TCP connection as raw
Ethernet/IPv4/TCP frames from an `AF_PACKET` socket. The peer is not a host
kernel socket, so every segment, option, advertised window and ACK is chosen by
the test. The same scenarios run against:

* the host kernel as the reference baseline, in a disposable network namespace
  (a veth pair, one end in a namespace that only runs the baseline server); and
* the Raven guest over the K227 tap (`10.0.2.15`, gateway `10.0.2.2`).

The host-side script re-executes in a user+net+mount namespace, creates the tap,
and starts QEMU. This keeps the tap privilege out of the test code and isolates
the baseline from the host network.

## Scenarios

| Scenario | Wire assertion |
|---|---|
| `handshake` | SYN-ACK has MSS and SACK; a peer that offered window scaling gets a scale, a peer that did not offer it gets **no** window-scale option and an unscaled window |
| `zero-window` | after the final ACK advertises zero, the server makes a persist/window probe; the probe byte is accepted and the window reopened; the reassembled stream is exactly the 44-byte fixture, and the accepted probe byte is not replayed |
| `window-update` | a stale zero window followed by a nonzero update must flush all 44 bytes |
| `retransmit` | the first 44-byte segment is left unacknowledged; an RTO retransmission carries the same sequence and bytes |
| `fin-rst` | the server delivers 44 bytes, accepts a peer FIN and returns its own FIN |

`dup-ack`, `out-of-order`, `partial-ack`, `backlog` and `busy` are narrowed, not
faked. The reasons are printed by `--scenario <name>` and are listed in
`NARROWED` in the harness: a single raw client cannot make the receiver see
duplicate ACKs, the fixture has one in-order server write, a partial ACK needs a
multi-segment write with selective loss, the guest listener accepts one
connection at a time, and device-busy injection needs a driver shim.

## Commands

```sh
timeout 120 sh scripts/net-conformance-selftest.sh
timeout 300 sh scripts/net-conformance.sh --scenario handshake --baseline linux
timeout 300 sh scripts/net-conformance.sh --scenario zero-window --baseline linux
timeout 300 sh scripts/net-conformance.sh --scenario zero-window --target raven
```

Add `--no-build` to reuse `build/qtest/net-conformance.img`; the target build
uses the existing `network-config-test` profile and overlays only
`/bin/netconf` and `/etc/services.d/netconf.toml` from
`tests/fixtures/net-conformance/`. No profile or product source is changed.

Every run writes a pcap under `tests/net-conformance/results/`; target runs also
write the guest serial log. The pcap contains both transmitted and received
frames, so a failing assertion can be replayed with
`tcpdump -nn -vvv -r <pcap>`.

## Scenario references

The harness is original test code. The scenario expectations were read from
behaviour, not copied:

| Topic | Illumos (CDDL, behaviour only) | BSD (permissive reference) | Linux (GPL, read-only outcome) |
|---|---|---|---|
| SYN/SYN-ACK options, window scale | `usr/src/uts/common/inet/tcp/tcp_output.c` `tcp_output`, `tcp_input.c` `tcp_input` option negotiation | NetBSD `sys/netinet/tcp_output.c` `tcp_output`, `tcp_input.c` `tcp_input`/`tcp_dooptions` | `net/ipv4/tcp_output.c` `tcp_connect`/`tcp_transmit_skb`, `tcp_input.c` `tcp_parse_options` |
| Zero-window persist/reopen | `tcp_output.c` persist path, `tcp_timer.c` `tcp_timer` | NetBSD `tcp_output.c` `tcp_setpersist`, `tcp_timer.c` `tcp_timer_persist`, `tcp_input.c` window update | `tcp_output.c` `tcp_write_xmit`/`tcp_send_probe0`, `tcp_timer.c` `tcp_write_timer`, `tcp_input.c` `tcp_ack` |
| RTO retransmission | `tcp_output.c`/`tcp_input.c` behaviour | NetBSD `tcp_output.c`/`tcp_input.c` | `tcp_output.c` `tcp_retransmit_skb`, `tcp_input.c` `tcp_ack` |
| FIN/half-close | `tcp_output.c`/`tcp_input.c` behaviour | NetBSD `tcp_output.c`/`tcp_input.c`/`tcp_subr.c` | `tcp_output.c` `tcp_send_fin`, `tcp_input.c` reset path |

No donor code is linked, vendored or translated. The Linux baseline is a black
box running the host kernel.

## Result on `bcc9ba1f`

The Linux baseline passes all five scenarios. The Raven guest:
`window-update`, `retransmit` and `fin-rst` pass; `handshake` fails because the
guest offers `wscale=1` to a peer that sent no window-scale option; and
`zero-window` fails because after the probe byte is acknowledged the guest
replays it from `snd_una` instead of resuming at the accepted byte (the
reassembled stream is correct, but the accepted probe byte is retransmitted).
Those are stack findings for the owning tickets; this ticket changes no product
code.
