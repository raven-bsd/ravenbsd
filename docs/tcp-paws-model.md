<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP timestamps/PAWS host model (K312)

Status: host-only evidence package. **OBSERVED** by `cargo nextest run -p tcp-paws-model`; no
product source is changed.

## What it does

`tests/tcp-paws-model` drives the public `raven-tcpip` `Stack`/`SocketId` API through a
timestamped passive handshake and direct IPv4/TCP segments. It compares the real stack against an
independent RFC 7323 reference model for:

* timestamp negotiation and SYN-ACK echo;
* PAWS reject/accept and `ts_recent` update;
* the 24-day (`TCP_PAWS_IDLE`) exception;
* sequence wrap and old duplicates;
* zero-window timestamp probes after the receive queue is full;
* 10,000 randomized sequence/timestamp events with an FNV-1a stream hash.

The handshake uses the merged K291 syncache path: the SYN is injected, the syncache entry supplies
`iss`/`irs`, the SYN-ACK timestamp is read from `Stack::test_trace`, and the final ACK promotes the
connection. The receive queue is inspected with the public host-test hooks
(`test_tcp_rcv_nxt`, `test_tcp_window`, `test_tcp_options_info`, `recv_queue_len`).

## Model matrix

| Case | Reference model | Real-stack test |
|---|---|---|
| timestamp negotiation/echo | SYN-ACK `TSecr` equals the SYN `TSval`; `ts_recent` starts at the SYN timestamp | `timestamps_are_negotiated_and_echoed` |
| PAWS stale/fresh | stale `TSval < ts_recent` within 24 days -> drop+ACK; fresh -> accept and record | `paws_rejects_stale_and_accepts_new` |
| 24-day boundary | FreeBSD/NetBSD drop at exactly 24 days; after 24 days invalidate `ts_recent` and accept | `reference_uses_the_freebsd_24_day_boundary` |
| sequence wrap | 32-bit modulo comparisons; data crossing `u32::MAX` advances `rcv_nxt` | `sequence_wrap_and_old_duplicate_match_the_reference` |
| old duplicate | stale duplicate is dropped by PAWS and never advances `rcv_nxt` | same |
| zero window | queue at `rx_queue_bytes`; stale probe drops, fresh probe does not advance `rcv_nxt` | `zero_window_timestamp_edges` |
| randomized | 10,000 events, 70% in-order, 20% stale old duplicate, 10% pure ACK; stream hash | `ten_thousand_randomized_sequences_match_the_reference` |

Randomized seed: `0x0312_7a77_5e01_0001`. The initial receive sequence is `u32::MAX - 50_000`, so
the accepted stream crosses `u32::MAX`; the test asserts the wrap happened. The final product and
reference streams are compared byte-for-byte by length and FNV-1a hash.

## Findings

1. **Exact 24-day boundary.** FreeBSD `sys/netinet/tcp_input.c:2179` drops when
   `tcp_ts_getticks() - ts_recent_age <= TCP_PAWS_IDLE` (age-out only on `>`). The product's
   `tcp_state.rs` Established/FinWait1 condition drops only when `age < 24 days`, so at exactly 24
   days it accepts a stale timestamp. `product_accepts_a_stale_timestamp_at_exactly_24_days`
   characterizes the product; the reference test proves the donor behaviour. This is a TCP-owner
   finding, not a K312 product edit.
2. **Handshake `ts_recent`.** The syncache promotes with the SYN's `TSval`; the final ACK's `TSval`
   is not installed as `ts_recent`. The model test records this so later PAWS comparisons start
   from the actual stack value.
3. **Delayed ACK.** A data segment may not produce an immediate ACK in the trace. Timestamp echo
   is therefore asserted on the SYN-ACK (the handshake echo) and `ts_recent` acceptance on the
   socket; the randomized test exercises the update path directly.

## Donor reconciliation

FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`:

* `sys/netinet/tcp_input.c` `tcp_do_segment` PAWS block :2173-2196, `ts_recent` update :2320-2346;
* `sys/netinet/tcp_seq.h` `SEQ_*` :39-46, `TSTMP_LT` :57, `TCP_PAWS_IDLE` :80.

NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`:
`sys/netinet/tcp_input.c` :2179-2202/:2340-2356, `TCP_PAWS_IDLE` :241, `TSTMP_LT` :244.

Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL, behaviour only):
`usr/src/uts/common/inet/tcp/tcp_input.c` `tcp_paws_check` :873-897, `PAWS_TIMEOUT` :71,
`ts_recent` update :3407-3415.

Linux `adc218676eef25575469234709c2d87185ca223a`: no TCP source in the sparse checkout; outcome
only. RFC 7323 sections 3, 5.3 and 5.4 are the protocol reference.

No donor code is copied. The model is original Rust, the crate is not linked into the product, and
no `PROVENANCE.toml` import applies.

## Using it

```sh
timeout 180 cargo nextest run -p tcp-paws-model
timeout 420 make check-fast
```

The tests print the seed in their source, not at runtime, so a failing run can be reproduced with
the same event sequence. This is test/evidence tooling; findings go to the TCP owners.
