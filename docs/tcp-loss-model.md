<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP loss/retransmit/reordering model (K317)

Host-only evidence for the merged K265 output and K267 timer paths under loss, duplicate ACKs,
reordering, fast retransmit and RTO. The crate `tests/tcp-loss-model` drives the public
`raven-tcpip` `Stack` over the deterministic sim link and compares every wire data segment with
an independent byte-offset reference oracle. No product change.

## Matrix

| Test | Scenario | Assertion |
|---|---|---|
| `ten_thousand_random_loss_reorder_sequences_hash_the_stream` | seeds 0..9999: loss 0..10 %, duplicate 0..5 %, reorder 0..10 %, RTT 5..10 ms, 3..8 segments of 128..512 bytes | every delivered stream hashes to the sent stream; every segment's sequence/payload matches the oracle; the segment ranges cover the stream; no ACK covers unsent bytes |
| `duplicate_acks_trigger_a_fast_retransmit` | three duplicate ACKs for the first unacked byte | `fast_retransmits >= 1` and a data segment with the first-unacked sequence is on the wire |
| `rto_recovers_a_stream_dropped_by_the_device` | client device drops all outgoing data until the RTO fires | `rto_fires >= 1`, `retrans_segments >= 1`, the stream completes after the device recovers |
| `a_corrupted_reference_stream_is_detected` | one byte flipped in the reference copy | the oracle rejects the segment (`payload mismatch`), proving every byte is compared |

Measured run: 10k seeds in 18–39 s (virtual time; the wall time depends on the host). Counts from
one run: loss 9098, reorder 9089, duplicate 8395, stack retransmissions 800, trace overlaps 16,
RTO 520, fast retransmit 19. All 10k hashes and oracle checks pass.

## Reference oracle

`validate_trace(trace, stream, first_seq)`:

* every data segment's sequence maps to a byte offset from the connection's `snd_una` at send time;
* the payload bytes equal `stream[offset..offset+len]`;
* the union of all segment ranges covers the whole stream (a lost tail is a failure);
* retransmissions are counted by range overlap and reordered new ranges by starting behind the
  high-water mark;
* an ACK may not acknowledge a byte beyond the highest offset sent so far in the trace.

The first sequence is taken from the stack (`test_tcp_window`) rather than the first traced frame,
so a device-dropped first segment still maps correctly.

## Mutation / negative

A temporary product mutation in `crates/raven-tcpip/src/tcp_state.rs`: the live RTO retransmit
sequence was changed from `snd_una` to `snd_nxt`. The 10k matrix fails at **seed 42** with a
delivered-length mismatch; reverting the mutation restores the byte-identical file and all 10k
seeds pass. The corrupted-reference test is the direct per-segment negative.

## Run

```sh
timeout 180 cargo nextest run -p tcp-loss-model
timeout 420 make check-fast
```

## Donor

FreeBSD `sys/netinet/tcp_output.c`, `sys/netinet/tcp_timer.c` and the RACK loss/reordering rules at
`133344405f532bd54ff3629a77d4c4e062f01fdb`; RFC 5681/6675. Test-only; no donor code copied.
