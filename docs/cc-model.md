<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# CUBIC/NewReno/RACK congestion-control model (K341)

Host-only reference model for the K269 congestion-control port. This is test/evidence code, not
product code. It does not link or copy donor source; it implements the state machines from the
FreeBSD `cc_newreno.c`, `cc_cubic.c`, `rack.c` behaviour and RFC 9438/8985.

## Model

`tests/cc-model` contains:

- `src/lib.rs` — deterministic scenario generator, NewReno/CUBIC/RACK state model and trace row.
- `src/lib_ut.rs` — behavioural tests, a NewReno comparison against the merged `raven-tcpip`
  crate, determinism checks and the 10k golden-trace check.
- `src/main.rs` — `cc-model --generate <path>` regenerates the trace file.
- `traces/cc-10k.tsv` — 10,000 deterministic traces, one row per seed.

Algorithms:

- **NewReno**: ABC slow start (`min(acked, 2*MSS)`), congestion avoidance (`MSS^2/cwnd`), 50%
  multiplicative decrease and recovery exit.
- **CUBIC**: `W_cubic = C*(t-K)^3 + W_max`, Reno-friendly lower bound, fast convergence
  (`W_max = cwnd` or `cwnd*217/256`) and 0.7 multiplicative decrease.
- **RACK**: loss declarations, recovery entry/exit (`recover = snd_max`) and RFC 6675-style pipe
  (`outstanding - lost`, floor one MSS).

## Trace format

The header is:

```
# seed  events  cwnd  ssthresh  in_recovery  lost  retransmits  pipe  digest
```

The digest is a stable FNV-style fold over every event's ACK and timestamp.

## Comparison findings

- The NewReno model matches the merged `raven-tcpip::NewReno` on an ACK-only slow-start trace
  (`newreno_slow_start_matches_the_ki_implementation`).
- K269's CUBIC/`CubicContext` and expanded RACK core are on `dave/K269`, not `origin/master`.
  A direct link from this host-only crate is impossible without a product edit or a cross-branch
  dependency. The model therefore records the independent oracle and the NewReno comparison; the
  CUBIC/RACK comparison remains a K269 finding until the branch is merged.
- The merged CUBIC (RFC 8312 arithmetic) is not the K269 donor CUBIC; traces for
  `Kind::Cubic` are expected to differ from the merged crate and are not asserted equal.

## Donor reconciliation

Test-only: FreeBSD `sys/netinet/cc/cc_newreno.c`, `cc_cubic.c`, `sys/netinet/tcp_stacks/rack.c`
at `133344405f` (BSD-2-Clause), and RFC 9438/8985. No donor code is copied; the model is
first-party test code.

## Acceptance

```
timeout 180 cargo nextest run -p cc-model
timeout 420 make check-fast
```
