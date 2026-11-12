<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP conformance results

Fresh BSD-3-Clause scripts run through the C04 simulator. packetdrill
(GPL, tier C) is a tool whose script language we accept; no packetdrill
code or scripts are present. See `SCRIPTS.md` for the subset.

Totals: **65 scripts, 47 pass, 1 fail, 17 skip.**

| Script | Area | RFC / spec | Verdict | Reason |
|---|---|---|---|---|
| `congestion/01-slow-start.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | pass |  |
| `congestion/02-fast-retransmit.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | pass |  |
| `congestion/03-newreno-vs-cubic.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | skip | C20-D08 no getsockopt for cc_algorithm |
| `congestion/04-cubic-curve.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | skip | C20-D09 cwnd curve is not exposed to a script |
| `congestion/05-ecn-ce.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | skip | C20-D10 live ECN CE response is not scripted |
| `congestion/06-cwnd-cap.pkt` | congestion | RFC 5681 / RFC 8312; spec/tcpip/features.md 5 | skip | C20-D11 cwnd cap is internal state |
| `establishment/01-handshake.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/02-active-open.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/03-graceful-close-active.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/04-graceful-close-passive.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/05-accept-after-handshake.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/06-close-wait-data-after-fin.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/07-fin-wait1-and-fin.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | fail | C20-D01 stack defect: FIN+ACK in FIN-WAIT-1 enters TIME-WAIT without the final ACK |
| `establishment/08-established-delayed-ack.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | pass |  |
| `establishment/09-simultaneous-open.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | skip | C20-D02 simultaneous-open transition is not implemented |
| `establishment/10-time-wait-reuse.pkt` | establishment | RFC 9293; spec/tcpip/state-machine.md 2 | skip | C20-D03 RFC 6191 TIME-WAIT reuse policy is not implemented |
| `keepalive/01-keepalive-probe.pkt` | keepalive | RFC 1122 / RFC 9293; spec/tcpip/features.md 7 | pass |  |
| `keepalive/02-keepalive-timeout.pkt` | keepalive | RFC 1122 / RFC 9293; spec/tcpip/features.md 7 | pass |  |
| `keepalive/03-fin-wait2-timeout.pkt` | keepalive | RFC 1122 / RFC 9293; spec/tcpip/features.md 7 | pass |  |
| `keepalive/04-fin-wait2-data.pkt` | keepalive | RFC 1122 / RFC 9293; spec/tcpip/features.md 7 | pass |  |
| `keepalive/05-user-timeout.pkt` | keepalive | RFC 1122 / RFC 9293; spec/tcpip/features.md 7 | pass |  |
| `listen/01-backlog-cookie.pkt` | listen | RFC 9293 / RFC 4987; spec/tcpip/state-machine.md 5 | pass |  |
| `listen/02-backlog-drop.pkt` | listen | RFC 9293 / RFC 4987; spec/tcpip/state-machine.md 5 | pass |  |
| `listen/03-half-open-limit.pkt` | listen | RFC 9293 / RFC 4987; spec/tcpip/state-machine.md 5 | pass |  |
| `listen/04-accept-queue-full.pkt` | listen | RFC 9293 / RFC 4987; spec/tcpip/state-machine.md 5 | skip | C20-D14 two full handshakes need a second peer ISN |
| `listen/05-no-reset-on-overflow.pkt` | listen | RFC 9293 / RFC 4987; spec/tcpip/state-machine.md 5 | pass |  |
| `mss-pmtu/01-mss-announce.pkt` | mss-pmtu | RFC 9293 / RFC 1191 / RFC 8899; spec/tcpip/features.md 3,8 | pass |  |
| `mss-pmtu/02-mss-large-syn.pkt` | mss-pmtu | RFC 9293 / RFC 1191 / RFC 8899; spec/tcpip/features.md 3,8 | pass |  |
| `mss-pmtu/03-icmp-pmtu.pkt` | mss-pmtu | RFC 9293 / RFC 1191 / RFC 8899; spec/tcpip/features.md 3,8 | skip | C20-D16 the script subset only injects TCP; ICMP needs an extension |
| `mss-pmtu/04-ignore-path-mtu.pkt` | mss-pmtu | RFC 9293 / RFC 1191 / RFC 8899; spec/tcpip/features.md 3,8 | skip | C20-D17 ICMP PMTU injection is not in the script subset |
| `mss-pmtu/05-plpmtud.pkt` | mss-pmtu | RFC 9293 / RFC 1191 / RFC 8899; spec/tcpip/features.md 3,8 | skip | C20-D18 PLPMTUD is not wired into the live send path |
| `options/01-window-scale.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/02-timestamps.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/03-sack-permitted.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/04-mss-option.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/05-unknown-option.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/06-mss-announce.pkt` | options | RFC 9293; spec/tcpip/features.md 3 | pass |  |
| `options/07-paws.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/08-ecn-negotiation.pkt` | options | RFC 7323 / RFC 2018; spec/tcpip/features.md 3 | pass |  |
| `options/09-mss-clamp-internal.pkt` | options | RFC 9293; spec/tcpip/features.md 3 | skip | C20-D15 the clamped peer MSS is only visible through SocketInfo |
| `rst/01-rst-established.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/02-rst-syn-sent.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/03-rst-out-of-window.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/04-challenge-ack-on-syn.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/05-challenge-ack-on-rst.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/06-ack-to-listener-rst.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rst/07-rst-to-listener-ignored.pkt` | rst | RFC 9293 / RFC 5961; spec/tcpip/state-machine.md 4 | pass |  |
| `rto/01-rto-retransmit.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | pass |  |
| `rto/02-rto-backoff.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | pass |  |
| `rto/03-rto-give-up.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | skip | C20-D06 full RTO exhaustion needs an ignore-retransmissions directive |
| `rto/04-karn.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | skip | C20-D07 Karn sampling is internal estimator state |
| `rto/05-rto-disarm.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | pass |  |
| `rto/06-connect-retries.pkt` | rto | RFC 6298; spec/tcpip/state-machine.md 6 | pass |  |
| `sack/01-sack-permitted.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | pass |  |
| `sack/02-sack-block.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | pass |  |
| `sack/03-sack-multiple-blocks.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | pass |  |
| `sack/04-sack-cap.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | pass |  |
| `sack/05-dsack.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | skip | C20-D04 DSACK reception and undo are not wired into the live path |
| `sack/06-sack-recovery.pkt` | sack | RFC 2018; spec/tcpip/features.md 4 | skip | C20-D05 SACK scoreboard recovery is not wired into the live send path |
| `zero-window/01-zero-window-probe.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | pass |  |
| `zero-window/02-zero-window-drop.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | skip | C20-D12 filling the receive window needs an application read pattern |
| `zero-window/03-window-update.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | pass |  |
| `zero-window/04-nagle.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | pass |  |
| `zero-window/05-sws.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | skip | C20-D13 SWS threshold is not exposed by a script directive |
| `zero-window/06-delayed-ack.pkt` | zero-window | RFC 9293 / RFC 5681; spec/tcpip/features.md 6 | pass |  |
