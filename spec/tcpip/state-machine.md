# TCP state machine, RST policy and timers

Status: normative for C08-C20. Every row carries an identifier used by the
C08 table-driven test. A row that is not implemented is a named failure, not a
silent omission.

State names: `CLOSED`, `LISTEN`, `SYN-SENT`, `SYN-RCVD`, `ESTAB`,
`FIN-WAIT-1`, `FIN-WAIT-2`, `CLOSE-WAIT`, `CLOSING`, `LAST-ACK`, `TIME-WAIT`.
`SYN-RCVD` is the RFC's SYN-RECEIVED.

## 1. Variables

| Name | Meaning |
|---|---|
| `ISS` | our initial send sequence number |
| `IRS` | peer's initial send sequence number |
| `SND.UNA` | oldest unacknowledged sequence number |
| `SND.NXT` | next sequence number we will send |
| `SND.WND` | peer's advertised window, scaled |
| `SND.WL1`, `SND.WL2` | sequence and acknowledgment numbers of the last window update |
| `RCV.NXT` | next sequence number we expect |
| `RCV.WND` | our advertised window in bytes |
| `RCV.UP` | highest urgent pointer received |
| `MAX.SND.WND` | largest window ever advertised by the peer, scaled (RFC 5961 §5.2) |
| `SND.MAX` | highest sequence number ever sent (for RACK; C14) |
| `FLIGHT` | `SND.NXT - SND.UNA`, bytes in flight |

## 2. Sequence-number arithmetic

All sequence comparisons use 32-bit serial-number arithmetic, in one audited
module. Let `d(a,b) = a.wrapping_sub(b) as i32`.

```
seq_lt(a,b)  = d(a,b) < 0
seq_le(a,b)  = d(a,b) <= 0
seq_gt(a,b)  = d(a,b) > 0
seq_ge(a,b)  = d(a,b) >= 0
```

A comparison is valid only when the true distance is less than 2^31; the stack
never compares numbers from different incarnations without an ISN/timestamp
check. `SEG.LEN` is payload length plus one for SYN and one for FIN. RFC 9293
§3.4.

## 3. Segment acceptability

The receive window is `[RCV.NXT, RCV.NXT + RCV.WND)`. A segment is acceptable
when any part of it lies in that half-open interval, with the zero-window
exceptions below. RFC 9293 §3.4 and §3.10.7.4 (Table 6).

| SEG.LEN | RCV.WND | Acceptable when |
|---|---|---|
| 0 | 0 | `SEG.SEQ == RCV.NXT` |
| 0 | > 0 | `RCV.NXT <= SEG.SEQ < RCV.NXT + RCV.WND` |
| > 0 | 0 | never (but RST and URG are still processed) |
| > 0 | > 0 | `RCV.NXT <= SEG.SEQ < RCV.NXT + RCV.WND` or `RCV.NXT <= SEG.SEQ + SEG.LEN - 1 < RCV.NXT + RCV.WND` |

A segment that straddles the left edge is trimmed; only the new part is
processed. A segment entirely left of the window is old and is ACKed and
discarded (it may be a retransmission). A segment entirely right of the window
is dropped and counted; it is not queued unless it is within the reassembly
budget and starts at a hole edge (C09). RFC 9293 §3.10.7.4.

The ACK field is checked separately:

* Classic acceptable ACK: `SND.UNA < SEG.ACK <= SND.NXT` (a new ACK).
* Duplicate ACK: `SEG.ACK <= SND.UNA` but still inside the RFC 5961 window.
* RFC 5961 window: `SND.UNA - MAX.SND.WND <= SEG.ACK <= SND.NXT`. An ACK
  outside it is answered with a challenge ACK and the segment is dropped.
  RFC 5961 §5.2.

## 4. User calls

`USER_ACCEPT` is the socket-layer operation from contract C15; RFC 9293
§3.9.1.1 describes the passive-open queue it consumes. Error names are the
contract's `Errno` values, mapped as B07 mapped them (C15).

| ID | State | Call | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-U-01 | CLOSED | OPEN passive | Create listener TCB, bind local endpoint, set `RCV.WND` from receive buffer, arm listener limits. | LISTEN | none | 9293 §3.10.1 |
| SM-U-02 | CLOSED | OPEN active | Choose ISS (C16), create TCB, set `SND.UNA=SND.NXT=ISS`, `SND.NXT=ISS+1` after SYN, arm connect timer. | SYN-SENT | SYN with options | 9293 §3.10.1 |
| SM-U-03 | LISTEN | OPEN passive again on same endpoint | Idempotent: return the existing listener. | LISTEN | none | 9293 §3.9.1.1; C15 |
| SM-U-04 | LISTEN | ACCEPT with completed connection | Remove one entry from the accept queue and hand its socket to the caller. | LISTEN | none | 9293 §3.9.1.1; C15 |
| SM-U-05 | LISTEN | ACCEPT with empty accept queue | Return `EAGAIN`; no state change. | LISTEN | none | C15 |
| SM-U-06 | LISTEN | CLOSE | Return any pending RECEIVEs with "closing"; delete listener and all half-open entries; abort established children that were never accepted. | CLOSED | none | 9293 §3.10.4 |
| SM-U-07 | LISTEN | ABORT | Delete listener and children; queued calls get "reset". | CLOSED | none | 9293 §3.10.5 |
| SM-U-08 | LISTEN | SEND or RECV | Return `ENOTCONN`; no state change. | LISTEN | none | 9293 §3.10.2, §3.10.3 |
| SM-U-09 | SYN-SENT | SEND | Queue bytes until ESTAB; return accepted count. Do not put data on the SYN (C08 scope). | SYN-SENT | none | 9293 §3.9.1.2; this spec `UNSPECIFIED` |
| SM-U-10 | SYN-SENT | RECV | Return `EAGAIN`; no state change. | SYN-SENT | none | C15 |
| SM-U-11 | SYN-SENT | SHUTDOWN_WRITE or CLOSE | Delete TCB; queued SEND/RECV get "closing"; do not emit a FIN. | CLOSED | none | 9293 §3.10.4 |
| SM-U-12 | SYN-SENT | ABORT | Delete TCB; queued calls get "reset"; no RST is emitted because no synchronized connection exists. | CLOSED | none | 9293 §3.10.5 |
| SM-U-13 | SYN-RCVD | SEND | Queue bytes; return accepted count. | SYN-RCVD | none | 9293 §3.9.1.2 |
| SM-U-14 | SYN-RCVD | SHUTDOWN_WRITE or CLOSE with no pending data | Send FIN; enter FIN-WAIT-1. | FIN-WAIT-1 | FIN | 9293 §3.10.4 |
| SM-U-15 | SYN-RCVD | SHUTDOWN_WRITE or CLOSE with pending data | Mark close requested; send FIN after ESTAB and after queued data. | SYN-RCVD | none yet | 9293 §3.10.4 |
| SM-U-16 | SYN-RCVD | ABORT | Send RST with `SEQ=SND.NXT`; delete TCB. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-17 | ESTAB | SEND | Queue bytes, return accepted count, emit segments subject to window, SWS, Nagle, congestion window (C09/C13). | ESTAB | data segments as allowed | 9293 §3.9.1.2 |
| SM-U-18 | ESTAB | RECV | Return queued bytes or `EAGAIN`. `Ok(0)` only after a FIN and after the byte queue is drained. | ESTAB | ACK/window update if needed | 9293 §3.9.1.3; C15 |
| SM-U-19 | ESTAB | SHUTDOWN_WRITE | Queue FIN after all queued data; no second FIN. | FIN-WAIT-1 | FIN after data | 9293 §3.10.4 |
| SM-U-20 | ESTAB | CLOSE | Same as SHUTDOWN_WRITE; release the socket after the FIN is acknowledged. | FIN-WAIT-1 | FIN after data | 9293 §3.10.4 |
| SM-U-21 | ESTAB | ABORT | Send RST with `SEQ=SND.NXT`; flush queues; notify "reset". | CLOSED | RST | 9293 §3.10.5 |
| SM-U-22 | FIN-WAIT-1 | SEND | Queue bytes; the peer still has a receive window until it acknowledges our FIN. | FIN-WAIT-1 | data as allowed | 9293 §3.9.1.2 |
| SM-U-23 | FIN-WAIT-1 | CLOSE or SHUTDOWN_WRITE | Idempotent: do not emit a second FIN. | FIN-WAIT-1 | none | 9293 §3.10.4 |
| SM-U-24 | FIN-WAIT-1 | ABORT | Send RST; flush; CLOSED. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-25 | FIN-WAIT-2 | SEND | Queue bytes; no FIN is outstanding. | FIN-WAIT-2 | data as allowed | 9293 §3.9.1.2 |
| SM-U-26 | FIN-WAIT-2 | CLOSE or SHUTDOWN_WRITE | Return "closing" or `Ok`; do not emit a second FIN. | FIN-WAIT-2 | none | 9293 §3.10.4 |
| SM-U-27 | FIN-WAIT-2 | ABORT | Send RST; flush; CLOSED. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-28 | CLOSE-WAIT | SEND | Queue bytes (we have received FIN, we may still send). | CLOSE-WAIT | data as allowed | 9293 §3.9.1.2 |
| SM-U-29 | CLOSE-WAIT | RECV | Return data; `Ok(0)` after the FIN was consumed and the queue drained. | CLOSE-WAIT | none | 9293 §3.9.1.3 |
| SM-U-30 | CLOSE-WAIT | CLOSE or SHUTDOWN_WRITE | Queue FIN after pending data; enter LAST-ACK. | LAST-ACK | FIN after data | 9293 §3.10.4 |
| SM-U-31 | CLOSE-WAIT | ABORT | Send RST; flush; CLOSED. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-32 | CLOSING | CLOSE or SHUTDOWN_WRITE | Return "closing"; do not emit a second FIN. | CLOSING | none | 9293 §3.10.4 |
| SM-U-33 | CLOSING | ABORT | Send RST; flush; CLOSED. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-34 | LAST-ACK | CLOSE or SHUTDOWN_WRITE | Return "closing"; do not emit a second FIN. | LAST-ACK | none | 9293 §3.10.4 |
| SM-U-35 | LAST-ACK | ABORT | Send RST; flush; CLOSED. | CLOSED | RST | 9293 §3.10.5 |
| SM-U-36 | TIME-WAIT | CLOSE or SHUTDOWN_WRITE | Return "closing"; keep the TIME-WAIT timer. | TIME-WAIT | none | 9293 §3.10.4 |
| SM-U-37 | TIME-WAIT | ABORT | Delete TCB; CLOSED. No RST (RFC 1337). | CLOSED | none | 9293 §3.10.5; RFC 1337 |
| SM-U-38 | any synchronized state | RECV on a reset connection | Return `ECONNRESET` before any queued data. | unchanged | none | 9293 §3.9.1.3 |

## 5. Incoming segments: CLOSED and LISTEN

### 5.1 CLOSED (no TCB)

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-C-01 | RST | any | Discard; never answer a RST with a RST. | CLOSED | none | 9293 §3.10.7.1 |
| SM-C-02 | any with ACK | not RST | Discard data; send reset taking `SEQ=SEG.ACK`. | CLOSED | RST | 9293 §3.10.7.1 |
| SM-C-03 | any without ACK | not RST | Send `SEQ=0, ACK=SEG.SEQ+SEG.LEN`. | CLOSED | RST,ACK | 9293 §3.10.7.1 |
| SM-C-04 | SYN to an unbound port | no listener matches 4-tuple | Treat as SM-C-03; do not create state. | CLOSED | RST,ACK | 9293 §3.10.7.1; §3.5.2 |
| SM-C-05 | broadcast or multicast with no listener | IPv4 limited broadcast or IPv6 multicast | Drop silently; do not send a RST to a group address. | CLOSED | none | 1122 §4.2.3.10; 9293 §3.5.2 |

### 5.2 LISTEN

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-L-01 | RST | any | Ignore. A RST cannot be valid for this incarnation. | LISTEN | none | 9293 §3.10.7.2 |
| SM-L-02 | ACK | any | Send reset with `SEQ=SEG.ACK`; do not create state. | LISTEN | RST | 9293 §3.10.7.2 |
| SM-L-03 | SYN | no ACK, half-open queue has room | Create half-open record; `IRS=SEG.SEQ`, `RCV.NXT=SEG.SEQ+1`, choose ISS; send SYN,ACK with options. | SYN-RCVD | SYN,ACK | 9293 §3.10.7.2 |
| SM-L-04 | SYN | no ACK, half-open queue full, cookies disabled | Drop the SYN; count `tcp.half_open_drops`; do not send a RST (a RST would confirm the port). | LISTEN | none | 4987 §3; this spec |
| SM-L-05 | SYN | no ACK, half-open queue full, cookies enabled | Send SYN,ACK whose ISS is a cookie (C16); keep no per-connection state until the ACK returns. | LISTEN | SYN,ACK | 4987 §4; C16 |
| SM-L-06 | SYN with data | handshake can proceed | Record the data with the half-open entry and process it after ESTAB (C09). | SYN-RCVD | SYN,ACK | 9293 §3.10.7.2 |
| SM-L-07 | FIN or data without SYN | any | If ACK is set, SM-L-02; otherwise drop. No state is created. | LISTEN | RST or none | 9293 §3.10.7.2 |
| SM-L-08 | ACK of a SYN cookie | cookie valid (checked before SM-L-02) | Reconstruct the connection in SYN-RCVD with the cookie's sequence numbers; then complete the handshake. | SYN-RCVD | none | 4987 §4; C16 |
| SM-L-09 | ACK of a SYN cookie | cookie invalid | Send RST; count `tcp.syn_cookie_invalid`. | LISTEN | RST | 4987 §4; C16 |

## 6. Incoming segments: SYN-SENT and SYN-RCVD

### 6.1 SYN-SENT

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-SS-01 | ACK unacceptable | `SEG.ACK <= ISS` or `SEG.ACK > SND.NXT`, RST not set | Send RST `SEQ=SEG.ACK`; drop segment. | SYN-SENT | RST | 9293 §3.10.7.3 |
| SM-SS-02 | ACK unacceptable and RST set | same | Drop silently; do not answer. | SYN-SENT | none | 9293 §3.10.7.3 |
| SM-SS-03 | RST, acceptable ACK | `SND.UNA < SEG.ACK <= SND.NXT` | Signal `ECONNREFUSED`; delete TCB. | CLOSED | none | 9293 §3.10.7.3 |
| SM-SS-04 | RST, no acceptable ACK | otherwise | Drop. | SYN-SENT | none | 9293 §3.10.7.3 |
| SM-SS-05 | SYN,ACK acceptable | ACK acceptable | `IRS=SEG.SEQ`, `RCV.NXT=SEG.SEQ+1`, `SND.UNA=SEG.ACK`; negotiate options; send ACK. Queue data for delivery. | ESTAB | ACK | 9293 §3.10.7.3 |
| SM-SS-06 | SYN, no ACK | any | Simultaneous open: `IRS=SEG.SEQ`, `RCV.NXT=SEG.SEQ+1`; send SYN,ACK (retransmit our SYN with the new ACK). | SYN-RCVD | SYN,ACK | 9293 §3.10.7.3 |
| SM-SS-07 | SYN,ACK with data | ACK acceptable | Establish as SM-SS-05; deliver data after ESTAB. | ESTAB | ACK | 9293 §3.10.7.3 |
| SM-SS-08 | duplicate SYN,ACK | same `IRS`, ACK acceptable | Resend ACK; remain. | SYN-SENT | ACK | 9293 §3.10.7.3 |
| SM-SS-09 | any other segment | no SYN/RST, or ACK not acceptable | Drop. | SYN-SENT | none | 9293 §3.10.7.3 |
| SM-SS-10 | connect timeout | retransmission limit reached | Signal `ETIMEDOUT`; delete TCB. | CLOSED | none | 9293 §3.8.3; C08 |
| SM-SS-11 | RST with acceptable ACK and data | any | Treat as SM-SS-03; data is discarded. | CLOSED | none | 9293 §3.10.7.3 |
| SM-SS-12 | ICMP hard error | validated against this connection (C05) | Map to `EHOSTUNREACH`/`ENETUNREACH`; delete TCB. | CLOSED | none | 1122 §4.2.3.9; 9293 §3.9.2.2 |

### 6.2 SYN-RCVD

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-SR-01 | RST, exact `RCV.NXT` | RFC 5961 validation | Passive open: remove half-open entry, return to LISTEN. Active open: `ECONNREFUSED`, CLOSED. | LISTEN or CLOSED | none | 9293 §3.10.7.4; 5961 §3 |
| SM-SR-02 | RST, in-window mismatch | `RCV.NXT < SEG.SEQ < RCV.NXT+RCV.WND` | Challenge ACK; drop; count. | SYN-RCVD | ACK | 5961 §3.2 |
| SM-SR-03 | RST, out of window | any | Drop; count. | SYN-RCVD | none | 5961 §3.2 |
| SM-SR-04 | duplicate SYN, no ACK | `SEG.SEQ == IRS` | Resend SYN,ACK; do not create a second half-open entry. | SYN-RCVD | SYN,ACK | 9293 §3.10.7.2, §3.10.7.4 |
| SM-SR-05 | SYN, no ACK, different SEQ | passive open | Per RFC 9293 §3.10.7.4 fourth, return the connection to LISTEN and drop; a retransmitted SYN can then create a fresh entry. | LISTEN | none | 9293 §3.10.7.4 |
| SM-SR-06 | SYN,ACK acceptable | our SYN is acknowledged | Establish (simultaneous open completion); send ACK. | ESTAB | ACK | 9293 §3.10.7.3, §3.10.7.4 |
| SM-SR-07 | ACK acceptable | `SND.UNA < SEG.ACK <= SND.NXT` | `SND.WND=SEG.WND`, update `SND.WL1/WL2`; move to accept queue if passive; send no extra ACK unless data/FIN requires one. | ESTAB | none or ACK | 9293 §3.10.7.4 |
| SM-SR-08 | ACK unacceptable | otherwise | Send RST `SEQ=SEG.ACK`; remain SYN-RCVD (the half-open entry may be retried). | SYN-RCVD | RST | 9293 §3.10.7.4 |
| SM-SR-09 | DATA with acceptable ACK | any | Establish, then process data; ACK. | ESTAB | ACK | 9293 §3.10.7.4 |
| SM-SR-10 | FIN with acceptable ACK | any | Establish, process FIN, enter CLOSE-WAIT, ACK. | CLOSE-WAIT | ACK | 9293 §3.10.7.4 |
| SM-SR-11 | FIN without acceptable ACK | any | Send RST; remain. | SYN-RCVD | RST | 9293 §3.10.7.4 |
| SM-SR-12 | URG | any | Record urgent pointer; deliver after ESTAB. | SYN-RCVD | none | 9293 §3.10.7.4 |
| SM-SR-13 | ACK acceptable, accept queue has room | passive open | Move the connection to the accept queue; the socket becomes readable-to-accept. | ESTAB | none | 9293 §3.10.7.4; C16 |
| SM-SR-14 | ACK acceptable, accept queue full | passive open | Drop the ACK, keep the SYN-RCVD entry; do not send a RST; count `tcp.accept_queue_drops`; the peer retransmits and succeeds after `tcp_accept`. | SYN-RCVD | none | 9293 §3.10.7.4; C16 |

## 7. Incoming segments: synchronized states

### 7.1 ESTAB

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-E-01 | RST, exact `RCV.NXT` | RFC 5961 | Flush queues; signal `ECONNRESET`; delete TCB. | CLOSED | none | 9293 §3.10.7.4; 5961 §3 |
| SM-E-02 | RST, in-window mismatch | `RCV.NXT < SEG.SEQ < RCV.NXT+RCV.WND` | Challenge ACK; drop; count `tcp.challenge_acks_sent`, `tcp.rst_challenge`. | ESTAB | ACK | 5961 §3.2 |
| SM-E-03 | RST, out of window | any | Drop; count `tcp.rst_out_of_window`. | ESTAB | none | 5961 §3.2 |
| SM-E-04 | SYN | any sequence | Challenge ACK; drop; rate-limit. Never reset. | ESTAB | ACK | 5961 §4.2; 9293 §3.10.7.4 |
| SM-E-05 | ACK outside RFC 5961 window | `SEG.ACK < SND.UNA-MAX.SND.WND` or `SEG.ACK > SND.NXT` | Challenge ACK; drop; count `tcp.ack_challenge`. | ESTAB | ACK | 5961 §5.2 |
| SM-E-06 | ACK acceptable, new | `SND.UNA < SEG.ACK <= SND.NXT` | Advance `SND.UNA`; free acknowledged bytes; update `SND.WND` if `SND.WL1/WL2` allow; restart retransmission timer; notify sender. | ESTAB | none yet | 9293 §3.10.7.4 |
| SM-E-07 | ACK duplicate | `SEG.ACK <= SND.UNA`, inside RFC 5961 window | Count duplicate ACK; at threshold trigger fast retransmit (C13); do not challenge. | ESTAB | maybe retransmit | 5681 §3.2; C13 |
| SM-E-08 | data in order | acceptable, `SEG.SEQ == RCV.NXT` | Deliver up to receive buffer; advance `RCV.NXT`; schedule ACK (C09). | ESTAB | ACK per delayed rules | 9293 §3.10.7.4 |
| SM-E-09 | data out of order | acceptable, not at left edge | Queue in reassembly within the hole budget; ACK immediately; count `tcp.in_ooo_segments`. | ESTAB | ACK | 9293 §3.10.7.4; C09 |
| SM-E-10 | duplicate data | entirely below `RCV.NXT` | ACK current `RCV.NXT`; count `tcp.in_dup_segments`; deliver nothing. | ESTAB | ACK | 9293 §3.10.7.4 |
| SM-E-11 | data beyond window | starts at or beyond right edge | Trim/drop; ACK current `RCV.NXT`; count `tcp.past_window_drops`. | ESTAB | ACK | 9293 §3.10.7.4 |
| SM-E-12 | FIN in order | `SEG.SEQ+SEG.LEN-1 == RCV.NXT` after data | Deliver data, advance over FIN, signal EOF, ACK; enter CLOSE-WAIT. | CLOSE-WAIT | ACK | 9293 §3.10.7.4 |
| SM-E-13 | FIN with data | acceptable | Process data first, then FIN, then CLOSE-WAIT. | CLOSE-WAIT | ACK | 9293 §3.10.7.4 |
| SM-E-14 | URG | acceptable | `RCV.UP=max(RCV.UP,SEG.UP)`; signal the application once per urgent contiguous run; never drop the byte (C09). | ESTAB | ACK as normal | 9293 §3.10.7.4, §3.8.5 |
| SM-E-15 | zero-window probe | `SEG.LEN == 1`, window zero, sequence `RCV.NXT-1` | ACK with current window; count `tcp.in_zwnd_probes`. | ESTAB | ACK | 9293 §3.8.6.1 |
| SM-E-16 | window update | acceptable ACK with new window, `SND.WL1/WL2` rule | Update `SND.WND`; if the window grew from zero or by at least the SWS threshold, allow sending. | ESTAB | maybe data/ACK | 9293 §3.10.7.4; 1122 §4.2.3.3 |
| SM-E-17 | keepalive probe | `SEG.LEN == 0`, sequence `RCV.NXT-1` | ACK; count `tcp.keepalive_probes_rcvd`; no application event. | ESTAB | ACK | 1122 §4.2.3.6 |
| SM-E-18 | RST with data | any | RST processing takes precedence; data is discarded. | CLOSED | none | 9293 §3.10.7.4 |
| SM-E-19 | unrecognized option | any | Ignore option; continue; count `tcp.option_unknown`. | ESTAB | normal | 9293 §3.1 |
| SM-E-20 | malformed option length | any | Drop segment, send challenge ACK (synchronized) or RST (non-synchronized); count `tcp.option_malformed`. | ESTAB | ACK or RST | 9293 §3.1 (MUST-7) |
| SM-E-21 | PAWS stale timestamp | timestamps negotiated, `TSval < TS.Recent` and not the 24-day exception | Drop; ACK if data advanced nothing? ACK current `RCV.NXT`; count `tcp.paws_drops`. | ESTAB | ACK | 7323 §5.2, §5.3 |
| SM-E-22 | ACK for unsent data | `SEG.ACK > SND.NXT` | Send ACK; drop; count `tcp.acks_for_unsent`. | ESTAB | ACK | 9293 §3.10.7.4; 5961 §5.2 |

### 7.2 FIN-WAIT-1

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-F1-01 | RST, exact | RFC 5961 | Abort; signal reset. | CLOSED | none | 9293 §3.10.7.4 |
| SM-F1-02 | RST, in-window mismatch | any | Challenge ACK; drop. | FIN-WAIT-1 | ACK | 5961 §3.2 |
| SM-F1-03 | RST, out of window | any | Drop. | FIN-WAIT-1 | none | 5961 §3.2 |
| SM-F1-04 | SYN | any | Challenge ACK; drop. | FIN-WAIT-1 | ACK | 5961 §4.2 |
| SM-F1-05 | ACK of our FIN | `SEG.ACK == SND.NXT` | Enter FIN-WAIT-2; no RST. | FIN-WAIT-2 | none | 9293 §3.10.7.4 |
| SM-F1-06 | ACK of our FIN plus FIN | ACK acceptable and FIN in order | Enter TIME-WAIT; start timer; ACK. | TIME-WAIT | ACK | 9293 §3.10.7.4 |
| SM-F1-07 | FIN without ACK of our FIN | FIN in order, `SEG.ACK < SND.NXT` | ACK; enter CLOSING. | CLOSING | ACK | 9293 §3.10.7.4 |
| SM-F1-08 | duplicate FIN | FIN at `RCV.NXT-1` | ACK; remain. | FIN-WAIT-1 | ACK | 9293 §3.10.7.4 |
| SM-F1-09 | data before FIN | acceptable | Deliver, ACK; still FIN-WAIT-1 until FIN. | FIN-WAIT-1 | ACK | 9293 §3.10.7.4 |
| SM-F1-10 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | FIN-WAIT-1 | ACK | 5961 §5.2 |
| SM-F1-11 | zero-window probe | window zero | ACK current window. | FIN-WAIT-1 | ACK | 9293 §3.8.6.1 |

### 7.3 FIN-WAIT-2

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-F2-01 | RST, exact | RFC 5961 | Abort; signal reset. | CLOSED | none | 9293 §3.10.7.4 |
| SM-F2-02 | RST, in-window mismatch | any | Challenge ACK; drop. | FIN-WAIT-2 | ACK | 5961 §3.2 |
| SM-F2-03 | RST, out of window | any | Drop. | FIN-WAIT-2 | none | 5961 §3.2 |
| SM-F2-04 | SYN | any | Challenge ACK; drop. (TIME-WAIT reuse does not apply here.) | FIN-WAIT-2 | ACK | 5961 §4.2 |
| SM-F2-05 | FIN in order | any | ACK; enter TIME-WAIT; start timer. | TIME-WAIT | ACK | 9293 §3.10.7.4 |
| SM-F2-06 | duplicate FIN | at `RCV.NXT-1` | ACK; remain. | FIN-WAIT-2 | ACK | 9293 §3.10.7.4 |
| SM-F2-07 | data | acceptable | Deliver; ACK. | FIN-WAIT-2 | ACK | 9293 §3.10.7.4 |
| SM-F2-08 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | FIN-WAIT-2 | ACK | 5961 §5.2 |
| SM-F2-09 | FIN-WAIT-2 timeout | no acceptable data or ACK for `tcp.finwait2_timeout` | Abort locally without RST; signal `ETIMEDOUT`; count `tcp.finwait2_timeouts`. | CLOSED | none | 1122 §4.2.2.13; this spec `DEVIATION-01` |

### 7.4 CLOSE-WAIT

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-CW-01 | RST, exact | RFC 5961 | Abort; signal reset. | CLOSED | none | 9293 §3.10.7.4 |
| SM-CW-02 | RST, in-window mismatch | any | Challenge ACK; drop. | CLOSE-WAIT | ACK | 5961 §3.2 |
| SM-CW-03 | SYN | any | Challenge ACK; drop. | CLOSE-WAIT | ACK | 5961 §4.2 |
| SM-CW-04 | data | acceptable | Deliver; ACK. | CLOSE-WAIT | ACK | 9293 §3.10.7.4 |
| SM-CW-05 | duplicate FIN | at `RCV.NXT-1` | ACK; remain. | CLOSE-WAIT | ACK | 9293 §3.10.7.4 |
| SM-CW-06 | ACK | acceptable | Update window; no state change. | CLOSE-WAIT | none | 9293 §3.10.7.4 |
| SM-CW-07 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | CLOSE-WAIT | ACK | 5961 §5.2 |
| SM-CW-08 | keepalive probe | `SEG.LEN == 0`, `SEG.SEQ == RCV.NXT-1` | ACK. | CLOSE-WAIT | ACK | 1122 §4.2.3.6 |

### 7.5 CLOSING

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-CL-01 | RST, exact | RFC 5961 | Abort; signal reset. | CLOSED | none | 9293 §3.10.7.4 |
| SM-CL-02 | RST, in-window mismatch | any | Challenge ACK; drop. | CLOSING | ACK | 5961 §3.2 |
| SM-CL-03 | SYN | any | Challenge ACK; drop. | CLOSING | ACK | 5961 §4.2 |
| SM-CL-04 | ACK of our FIN | `SEG.ACK == SND.NXT` | Enter TIME-WAIT; start timer. | TIME-WAIT | none | 9293 §3.10.7.4 |
| SM-CL-05 | duplicate FIN | at `RCV.NXT-1` | ACK; remain. | CLOSING | ACK | 9293 §3.10.7.4 |
| SM-CL-06 | data after FIN | any | Ignore data, ACK; count `tcp.data_after_fin`. | CLOSING | ACK | 9293 §3.10.7.4 |
| SM-CL-07 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | CLOSING | ACK | 5961 §5.2 |

### 7.6 LAST-ACK

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-LA-01 | RST, exact | RFC 5961 | Abort; signal reset. | CLOSED | none | 9293 §3.10.7.4 |
| SM-LA-02 | RST, in-window mismatch | any | Challenge ACK; drop. | LAST-ACK | ACK | 5961 §3.2 |
| SM-LA-03 | SYN | any | Challenge ACK; drop. | LAST-ACK | ACK | 5961 §4.2 |
| SM-LA-04 | ACK of our FIN | `SEG.ACK == SND.NXT` | Delete TCB; signal closed. | CLOSED | none | 9293 §3.10.7.4 |
| SM-LA-05 | duplicate ACK | `SEG.ACK == SND.UNA < SND.NXT` | Challenge ACK; count `tcp.challenge_acks_sent`. | LAST-ACK | ACK | 9293 §3.10.7.4; this spec `UNSPECIFIED` |
| SM-LA-06 | duplicate FIN | at `RCV.NXT-1` | ACK; remain. | LAST-ACK | ACK | 9293 §3.10.7.4 |
| SM-LA-07 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | LAST-ACK | ACK | 5961 §5.2 |

### 7.7 TIME-WAIT

| ID | Event | Precondition | Action | Next | Emit | RFC |
|---|---|---|---|---|---|---|
| SM-TW-01 | RST | any | Ignore; count `tcp.timewait_rst_ignored`; do not delete the TCB. | TIME-WAIT | none | 1337 §3; this spec `DEVIATION-02` |
| SM-TW-02 | duplicate FIN | at `RCV.NXT-1` | ACK; restart the TIME-WAIT timer. | TIME-WAIT | ACK | 9293 §3.10.7.4 |
| SM-TW-03 | new SYN, timestamps used | timestamp greater than last seen | Honor the connection request: create SYN-RCVD, choose ISS, send SYN,ACK. | SYN-RCVD | SYN,ACK | 6191 §2 |
| SM-TW-04 | new SYN, timestamps used, equal timestamp | timestamp equal and `SEG.SEQ` greater than last sequence | Honor as SM-TW-03. | SYN-RCVD | SYN,ACK | 6191 §2 |
| SM-TW-05 | new SYN, timestamps not used | `SEG.SEQ` greater than last sequence | Honor as SM-TW-03. | SYN-RCVD | SYN,ACK | 6191 §2 |
| SM-TW-06 | new SYN, reuse not allowed | otherwise | Drop; keep TIME-WAIT; count `tcp.timewait_syn_drops`. | TIME-WAIT | none | 6191 §2 |
| SM-TW-07 | data | any | ACK current `RCV.NXT`; do not deliver; count `tcp.timewait_data_drops`. | TIME-WAIT | ACK | 1337 §3 |
| SM-TW-08 | ACK outside RFC 5961 window | any | Challenge ACK; drop. | TIME-WAIT | ACK | 5961 §5.2 |
| SM-TW-09 | TIME-WAIT timeout | timer expires | Delete TCB; release port. | CLOSED | none | 9293 §3.10.8 |

## 8. RST policy

The complete rule set; no other RST is ever generated.

| ID | Situation | RST form | RFC |
|---|---|---|---|
| SM-RST-01 | Segment to CLOSED with ACK set | `SEQ=SEG.ACK`, no ACK flag | 9293 §3.10.7.1 |
| SM-RST-02 | Segment to CLOSED without ACK | `SEQ=0, ACK=SEG.SEQ+SEG.LEN` | 9293 §3.10.7.1 |
| SM-RST-03 | ACK to LISTEN | `SEQ=SEG.ACK` | 9293 §3.10.7.2 |
| SM-RST-04 | Unacceptable ACK in SYN-SENT | `SEQ=SEG.ACK` | 9293 §3.10.7.3 |
| SM-RST-05 | Unacceptable ACK in SYN-RCVD | `SEQ=SEG.ACK` | 9293 §3.10.7.4 |
| SM-RST-06 | User ABORT in a synchronized state | `SEQ=SND.NXT` | 9293 §3.10.5 |
| SM-RST-07 | Any incoming RST | never answer with a RST | 9293 §3.10.7.1 |
| SM-RST-08 | Broadcast/multicast, no listener | no RST | 1122 §4.2.3.10 |
| SM-RST-09 | Segment with source address 0.0.0.0 or unspecified IPv6 | no RST | 1122 §4.2.3.10; C05 validates |
| SM-RST-10 | SYN to a full listen queue with cookies disabled | no RST (drop) | 4987 §3 |
| SM-RST-11 | Segment failing the IPv4/IPv6 checksum | no RST; drop and count | 9293 §3.1; C02 |
| SM-RST-12 | ICMP error quoted header does not match a connection | no RST; ignore and count | 1122 §4.2.3.9; C15 |

A RST is validated before it changes state. In all states except SYN-SENT the
exact-sequence rule of RFC 5961 §3.2 applies; in SYN-SENT the ACK must
acknowledge the SYN. A validated RST deletes the TCB immediately; no FIN is
sent and no data is delivered after the reset indication.

## 9. Challenge ACK policy

A challenge ACK is an empty ACK:

```
<SEQ=SND.NXT><ACK=RCV.NXT><CTL=ACK><WND=current receive window>
```

It is generated for exactly these cases:

| ID | Case | RFC |
|---|---|---|
| SM-CH-01 | In-window RST whose `SEG.SEQ != RCV.NXT` | 5961 §3.2 |
| SM-CH-02 | Any SYN in a synchronized state (ESTAB, FIN-WAIT-1/2, CLOSE-WAIT, CLOSING, LAST-ACK, TIME-WAIT except the reuse path) | 5961 §4.2 |
| SM-CH-03 | Incoming ACK outside `[SND.UNA-MAX.SND.WND, SND.NXT]` | 5961 §5.2 |
| SM-CH-04 | Duplicate ACK in LAST-ACK that does not acknowledge our FIN | 9293 §3.10.7.4; `UNSPECIFIED` |
| SM-CH-05 | Segment that is not acceptable in a synchronized state and is not a RST | 9293 §3.10.7.4 |

Rate limiting: a per-connection token bucket of `tcp.challenge_ack_burst`
(default 10) tokens per `tcp.challenge_ack_interval` (default 5000 ms), plus a
global bucket of `tcp.challenge_ack_global_burst` (default 1000) per
`tcp.challenge_ack_global_interval` (default 1000 ms). A challenge ACK that
would exceed the bucket is suppressed and counted
`tcp.challenge_ack_suppressed`. This is the RFC 5961 §7 mechanism with our
defaults; both buckets are tunable.

## 10. TIME-WAIT behaviour

* Duration: `tcp.time_wait_interval`, default 60000 ms (2*MSL with MSL=30 s),
  range 1000..600000 ms. RFC 9293 §3.6; RFC 1122 §4.2.2.13.
* The TCB is kept in a separate bounded TIME-WAIT table (C17), not in the main
  connection table. The table has `tcp.timewait_max` entries (default 65536).
  When full, the oldest entry is recycled and counted
  `tcp.timewait_recycled`.
* The timer is restarted on a duplicate FIN (SM-TW-02) and on a retransmitted
  ACK that acknowledges our FIN, but not on arbitrary data (SM-TW-07).
* A RST in TIME-WAIT is ignored (SM-TW-01, RFC 1337). This differs from
  RFC 9293 §3.10.7.4, which says a RST enters CLOSED; we choose the
  assassination-resistant behaviour and record it as `DEVIATION-02`.
* Port reuse: only the RFC 6191 rules (SM-TW-03..06) may replace a TIME-WAIT
  entry with a new incarnation. If the peer offers no timestamps, a SYN whose
  sequence number is greater than the last sequence number seen on the old
  incarnation may reuse the entry; otherwise the SYN is dropped. A new
  connection from the same 4-tuple is never accepted silently, and the old
  entry is never removed merely because a SYN arrived.
* A TIME-WAIT entry never accepts data. It answers with the current ACK and
  window, which lets a confused peer learn the state and stop retransmitting.

## 11. Timers

The stack has one unified timer set (C10). Each connection has at most one
armed deadline per timer class; `poll(now)` fires everything due at `now`
before returning. No timer callback runs outside `poll`.

| ID | Timer | Start | Stop / restart | Action | Default | RFC |
|---|---|---|---|---|---|---|
| SM-T-01 | connect (SYN) | active OPEN sends SYN | ACK acceptable or abort | Retransmit SYN with exponential backoff; after `tcp.connect_retries` (default 6) signal `ETIMEDOUT`. | initial 1000 ms, cap 60000 ms | 9293 §3.8.3 |
| SM-T-02 | retransmission (RTO) | data segment sent | all data acknowledged, or RTO | Retransmit oldest unacknowledged segment; double RTO; C10. | `tcp.rto_initial` 1000 ms | 6298 §5; 9293 §3.8.1 |
| SM-T-03 | delayed ACK | data received that is not ACKed immediately | ACK sent or timer fires | Send ACK; count `tcp.delayed_acks_sent`. | `tcp.delack_delay` 40 ms | 1122 §4.2.3.2; 9293 §3.8.6.3 |
| SM-T-04 | persist / zero-window probe | send blocked by zero window | window opens or abort | Send one-byte probe at `RCV.NXT-1`; exponential backoff, cap `tcp.zwprobe_max_interval`; count `tcp.zwprobe_fires`. | 1000 ms, cap 60000 ms | 9293 §3.8.6.1; 6429 |
| SM-T-05 | keepalive | idle for `tcp.keepalive_interval` and keepalive enabled | any activity or abort | Send probe; after `tcp.keepalive_probes` unanswered probes signal `ETIMEDOUT`; count `tcp.keepalive_fires`. | interval 7200000 ms, probes 9, off by default | 1122 §4.2.3.6; 9293 §3.8.4 |
| SM-T-06 | TIME-WAIT | enter TIME-WAIT | duplicate FIN restarts; timeout | Delete TCB; count `tcp.timewait_expiries`. | `tcp.time_wait_interval` 60000 ms | 9293 §3.10.8 |
| SM-T-07 | FIN-WAIT-2 | enter FIN-WAIT-2; restart on any acceptable data, window update or duplicate FIN | FIN or timer | Abort locally without RST; signal `ETIMEDOUT`; count `tcp.finwait2_timeouts`. | `tcp.finwait2_timeout` 60000 ms | 1122 §4.2.2.13; `DEVIATION-01` |
| SM-T-08 | user timeout | caller sets `SO_RCVTIMEO`-like limit | caller clears or timeout | Abort, flush, signal "timed out"; count `tcp.user_timeouts`. | off | 9293 §3.10.8 |
| SM-T-09 | reassembly | first out-of-order segment queued | hole filled or timer | Drop the hole set, count `tcp.reassembly_timeouts`; do not reset the connection. | 60000 ms | 1122 §4.2.2.21; C09 |
| SM-T-10 | TLP / PTO | unacknowledged data and no earlier timer | ACK or probe sent | Send a tail loss probe (C14); count `tcp.tlp_fires`. | PTO per RFC 8985 | 8985 §7 |
| SM-T-11 | challenge ACK rate window | challenge ACK sent | window expires | Refill the bucket. | 5000 ms | 5961 §7 |

## 12. Simultaneous open

```
A                                                B
ESTAB? no: both are in SYN-SENT after active OPEN
      SYN(seq=ISS_A)  ->
                                    <-  SYN(seq=ISS_B)
receive SYN without ACK:
  IRS=ISS_B, RCV.NXT=ISS_B+1
  -> SYN-RCVD, send SYN,ACK(seq=ISS_A, ack=ISS_B+1)
                                    receive SYN,ACK:
                                      ACK acceptable, IRS=ISS_A
                                      -> ESTAB, send ACK
      <- SYN,ACK(seq=ISS_B, ack=ISS_A+1)
  ACK acceptable -> ESTAB, send ACK
```

Rows: SM-SS-06 (SYN without ACK in SYN-SENT), SM-SR-04 (duplicate SYN),
SM-SR-06 (SYN,ACK in SYN-RCVD). The peer's SYN is retransmitted with the same
ISS; the connection uses one ISS/IRS pair, not two.

## 13. Simultaneous close

```
A                                                B
USER_CLOSE -> FIN-WAIT-1, send FIN
      FIN(seq=x)  ->
                                    <-  FIN(seq=y)
receive FIN, our FIN not ACKed:
  -> CLOSING, send ACK
      ACK(ack=y+1)  ->
                                    receive FIN, our FIN not ACKed:
                                      -> CLOSING, send ACK
      <- ACK(ack=x+1)
receive ACK of our FIN -> TIME-WAIT
                                    receive ACK of our FIN -> TIME-WAIT
```

Rows: SM-F1-07, SM-CL-04, SM-TW-02, SM-TW-09. Neither side sends a second FIN.

## 14. Option emission order

Our order is fixed; it is a local choice because RFC 9293 §3.2 only defines
the option formats. The order was chosen so that MSS is first (useful even to
a peer that stops parsing early), the two window/timestamp options are
word-aligned without relying on EOL, and SACK blocks on data segments have
the most room.

SYN and SYN-ACK, all options enabled (maximum 28 bytes used, 40 available):

| Offset | Bytes | Meaning |
|---|---|---|
| 0 | `02 04 MM MM` | MSS |
| 4 | `04 02` | SACK-permitted |
| 6 | `01 01` | two NOPs, aligns the timestamp |
| 8 | `08 0a TS TS TS TS EC EC EC EC` | Timestamps |
| 18 | `01 01` | two NOPs, aligns window scale |
| 20 | `03 03 SS` | Window scale |
| 23 | `00` | End of option list, then zero padding to the data offset |

If an option is disabled, it is omitted and the remaining options keep this
relative order; padding is chosen to end on a 4-byte boundary. The data offset
is `5 + ceil(options_len/4)`.

Data segments: Timestamps first (`08 0a ...`), then SACK blocks if any, then
NOP padding. MSS, window scale and SACK-permitted MUST NOT appear outside SYN
and SYN-ACK. RFC 9293 §3.2; RFC 2018 §3; RFC 7323 §4.

## 15. Deviations register

| ID | Behaviour | RFC position | Our choice and reason |
|---|---|---|---|
| `DEVIATION-01` | FIN-WAIT-2 timeout | RFC 9293 §3.10.8 defines no timer for FIN-WAIT-2; RFC 1122 §4.2.2.13 permits a timeout. | We close after `tcp.finwait2_timeout` (default 60 s) without a RST and signal `ETIMEDOUT`. Reason: bounded state, C08 requirement, no danger of killing a live connection because the peer has already sent FIN. |
| `DEVIATION-02` | RST in TIME-WAIT | RFC 9293 §3.10.7.4 says a valid RST enters CLOSED. | We ignore the RST and keep TIME-WAIT (RFC 1337). Reason: prevents TIME-WAIT assassination by an off-path attacker; the only cost is that a genuinely restarted peer must retransmit its SYN, which RFC 6191 already handles. |
| `DEVIATION-03` | Keepalive enablement surface | RFC 1122 §4.2.3.6 says keepalive is off by default for a connection. | A connection still starts with keepalive off; `tcp.keepalive_enable` controls whether the facility is available to the socket layer, and the per-socket opt-in is the default-off switch. Recorded because C19 exposes both levels. |
| `DEVIATION-04` | Initial window | RFC 5681 §3.1 allows 2-4 segments; RFC 6928 updates it to 10. | 10 MSS by default, configurable. Reason: C13 requires RFC 6928; the peer's window still caps the flight. |
| `DEVIATION-05` | Challenge ACK defaults | RFC 5961 §7 gives an example of 10 per 5 s and says both values are tunable. | Defaults 10 per 5 s per connection and 1000 per 1 s globally. Reason: protects the stack from reflection while allowing a legitimate peer's RST to be challenged promptly. |
| `DEVIATION-06` | RTO floor | RFC 6298 §2.4 says an RTO below 1 s SHOULD be rounded up to 1 s. | `tcp.rto_min` defaults to 200 ms; set it to 1000 to recover the RFC default. Reason: low-latency paths; the estimator arithmetic is unchanged. Detailed in `vectors/rto.md` §3. |

`UNSPECIFIED` choices that are not deviations: SYN data is queued rather than
processed on the SYN (C08 scope); `USER_ACCEPT` is a socket-layer operation;
a duplicate ACK in LAST-ACK is challenged (SM-LA-05); SYN cookies lose
timestamp and window-scale state (C16 documents what is lost).

## 16. Row index for C08

The C08 test enumerates these identifiers by name. A row is implemented when
the named transition can be driven through the C04 simulator and observed in
the trace. The list is generated from the tables above and must stay in sync
with them:

```
SM-U-01 ... SM-U-38
SM-C-01 ... SM-C-05
SM-L-01 ... SM-L-09
SM-SS-01 ... SM-SS-12
SM-SR-01 ... SM-SR-14
SM-E-01 ... SM-E-22
SM-F1-01 ... SM-F1-11
SM-F2-01 ... SM-F2-09
SM-CW-01 ... SM-CW-08
SM-CL-01 ... SM-CL-07
SM-LA-01 ... SM-LA-07
SM-TW-01 ... SM-TW-09
SM-RST-01 ... SM-RST-12
SM-CH-01 ... SM-CH-05
SM-T-01 ... SM-T-11
```

Counts: 38 user-call rows, 5 CLOSED rows, 9 LISTEN rows, 12 SYN-SENT rows,
14 SYN-RCVD rows, 22 ESTAB rows, 11 FIN-WAIT-1 rows, 9 FIN-WAIT-2 rows,
8 CLOSE-WAIT rows, 7 CLOSING rows, 7 LAST-ACK rows, 9 TIME-WAIT rows,
12 RST rows, 5 challenge-ACK rows, 11 timer rows. 179 rows in total.
