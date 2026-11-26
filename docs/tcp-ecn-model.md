<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP ECN/PMTU/PLPMTUD host model (K308)

Status: **OBSERVED** on the merged K270 tree (`origin/master` `efbbcb93`). This is a test-only
model; it makes no product-source change and no wire claim.

`tests/tcp-ecn-model` is an independent reference state machine plus a deterministic random driver
over the public `raven-tcpip` API. It re-derives the merged FreeBSD-aligned behaviour and RFC
3168/8899/1191 rules; divergences are reported to K270, not patched here.

## Model matrix

| Model | Product API | Reference | Corpus |
|---|---|---|---|
| once-per-RTT CE latch, CWR | `recovery::Ecn` (`negotiate`, `on_ce`, `on_rtt`, `take_cwr`) | `ecn::RefEcn` | 3 seeds x 1,000 ops |
| classic RFC 3168 ECN | `ecn::EcnState` mode 2 (`input_syn_sent`, `input_segment`, `output_established`), `EcnCounters` | `ecn::RefClassic`, `RefCounters` | 3 seeds x 1,000 ops |
| per-destination PMTU cache | `MtuCache` (`insert`, `get`, `expire`, `len`) | `pmtu::RefCache` | 3 seeds x 300 ops, 6 destinations |
| IPv4 PMTU/MSS clamp | `next_pmtu`, `on_icmp_frag_needed`, `mssopt`, `mss_update` | `pmtu::ref_*` | 3 seeds x 500 cases |
| IPv6 PMTU/MSS clamp | `next_pmtu6`, `on_icmp6_frag_needed`, `mssopt`/`mss_update` V6, `floor_for` | `pmtu::ref_*` | 3 seeds x 300 cases |
| RFC 8899 search | `Plpmtud` (`start`, `next_probe`, `on_probe_ack`, `on_probe_loss`) | `plpmtud::RefPlpmtud` | 3 seeds x 100 searches x 100 steps |
| probe sequence | `probe_sequence` | `plpmtud::ref_probe_sequence` | 3 seeds x 100 |
| black-hole ladder | `Plpmtud::new(floor, mss, budget)` + `start` + losses | `plpmtud::ref_blackhole` | 3 seeds x 100 ladders |

Seeds: `0x123456789abcdef0`, `0x0badf00ddeadbeef`, `0xa5a55a5a12344321`. The generator is a local
xorshift64*; no external dependency and no wall clock.

## Findings

* No divergence was found in the modeled surface between the product and the reference.
* `EcnState` mode 2 only *raises* the permit on a bare ECE; a later non-ECE SYN does not clear it.
  The reference mirrors the donor/product rule (the first model draft cleared it and the random
  test caught the disagreement).
* `plpmtud_live::plpmtud_blackhole` is `pub(crate)` and cannot be driven from an external model.
  The model covers the public `Plpmtud` fallback: a fresh search from the floor to the current MSS,
  and the all-loss ladder returning to the floor. **K270 finding:** expose the live black-hole
  policy behind a test seam if direct coverage is required.
* Accurate ECN (modes 3/4, ACE counters) is not modeled here; K270's `ecn_ut.rs` covers the known
  Accurate ECN vectors. This is an evidence gap, not a product defect.

## Mutation / negative check

The model-only defect injection mutates the **reference**, not the product (the ticket forbids
product edits); the named model test must fail:

| Reference mutation | Named test | Observed |
|---|---|---|
| `next_pmtu` floor `minmss + 40` -> `+39` | `v4_pmtu_and_mss_match_the_reference` | failed `left: 378, right: 377` |
| `RefPlpmtud::on_probe_loss` does not lower `high` | `plpmtud_search_matches_the_reference` | failed `left: Some(1602), right: Some(1839)` |
| `RefEcn::on_ce` does not latch once per RTT | `ecn_once_per_rtt_matches_the_reference` | failed `left: false, right: true` |

`the_model_detects_a_mutated_reference` keeps a permanent negative guard for the PMTU and ECN
latches.

## Run

```text
timeout 180 cargo nextest run -p tcp-ecn-model
timeout 420 make check-fast
```

The crate is a workspace member (`tests/tcp-ecn-model` in the root `Cargo.toml`; the one-line
new-crate registration allowed by handbook rule 5). No product source file is touched.

## Provenance

Test-only: FreeBSD `sys/netinet/tcp_ecn.c`, `tcp_output.c`, `tcp_subr.c`, `tcp_pmtud.c` behaviour at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause), RFC 3168/8899/1191/8200 and the merged
`raven-tcpip` public API. No donor code is copied; the reference is original Rust derived from the
documented rules.
