<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK logging/BBR audit (K352)

Host-only audit of the merged K329 logging/sysctl group (43 donor functions) and
K328 BBR/pacing group (35 donor functions) against FreeBSD
`sys/netinet/tcp_stacks/rack.c`/`tcp_rack.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause). No product code is
changed.

The crate is `tests/rack-audit-log-bbr`:

```text
timeout 180 cargo nextest run -p rack-audit-log-bbr
timeout 180 cargo nextest run -p raven-tcpip
cargo run -q -p rack-audit-log-bbr > /tmp/k352-matrix.md
cargo run -q -p rack-coverage -- --donor /tmp/freebsd-rack.c \
  --matrix /tmp/k352-matrix.md --manifest tests/rack-coverage/fixtures/rack-functions.txt
```

`BBR_ROWS` and `LOG_ROWS` in `src/lib.rs` carry every donor function, its Rust
mapping, named test and mutation. The audit reads the merged source and test
files with `include_str!` and fails if a mapping, test or mutation is missing.

## Audit result

| Check | Result |
|---|---|
| donor rows | 78 (35 BBR + 43 logging), unique |
| mappings/tests/mutations | all present |
| donor constants | all match the pinned header/defaults |
| K315 slice | 78 rows, 0 duplicate/unknown/empty/not PASS; 141 other-group missing |
| group tests | `cargo nextest -p raven-tcpip` 1,156 passed |

### Discrepancies reported to the group owners

1. **Duplicate logging type (8 functions):** `BbrRack` keeps its own
   `BbrLogEvent`/`log_*` methods for `rack_log_gpset`, `rack_log_hybrid_bw`,
   `rack_log_hdwr_pacing`, `rack_log_type_pacing_sizes`, `rack_log_timely`,
   `rack_log_gp_calc`, `rack_log_hybrid` and `rack_log_pacing_delay_calc`.
   K329 already owns and tests the same donor functions in `RackLog`. This is
   the "duplicate type" finding: resolve by reusing `RackLog` or record an
   approved divergence.
2. **Foreign K314 rows in K328's 35-row matrix (4):** `rack_set_cc_pacing`,
   `rack_undo_cc_pacing`, `rack_remove_pacing` and `rack_get_pacing_len` are
   `RackOs` context functions, not K328-owned donor functions. They are valid
   code, but K328 must not claim them as its coverage rows.
3. **Unclaimed K334 donor functions implemented in BBR (5):**
   `rack_get_decrease`, `rack_decrease_highrtt`, `rack_set_prtt_target`,
   `rack_set_pace_segments` and `rack_set_dgp` exist as private `BbrRack`
   helpers but are absent from K328's 35-row matrix. They are K334-owned in the
   K269 split; either K334 claims them or K328 documents the private helper
   divergence.
4. **K338 register is stale for this group:** it still marks the 35 K328 rows
   `ASSIGNED K328` because K328 merged after the register was written. K338
   must replace them with the K328 PASS rows; K352 does not own
   `docs/rack-coverage.md`.
5. No missing functions, no wrong donor constants, no missing named test and no
   missing mutation were found in the 78-row slice.

## K315 output

```text
donor: 219 function(s)
matrix: 78 row(s)
manifest: 219 entries match
missing (141): ... the other K328-K336/269 groups ...
result: FAIL (141 missing, 0 duplicate, 0 unknown, 0 empty Rust, 0 empty test, 0 empty mutation, 0 not PASS)
```

The 78 audited rows are all PASS in the matrix; the 141 missing functions are
outside the assigned logging/BBR group.

## Mutation / negative check

* A test-only mutation changed the expected `RACK_MIN_BW` from `8_000` to
  `8_001`; `donor_constants_match_the_pinned_header` failed (exit 100). The
  constant was restored and the full 6-test audit passed.
* The audit's `known_discrepancies_are_exactly_reported` test asserts the exact
  duplicate/foreign/unclaimed lists above, so a resolver cannot silently make a
  finding disappear.

## Donor evidence

Sources read (read-only): FreeBSD `sys/netinet/tcp_stacks/rack.c`,
`tcp_rack.h`, `tcp_bbr.h`, `rack_bbr_common.h`, `tcp_var.h`, `tcp.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb`. Constants checked include
`BBR_UNIT=256`, `RACK_MIN_BW=8000`, `MIN_GP_WIN=6`, `RACK_REQ_AVG=3`,
`RACK_TIMELY_CNT_BOOST=5`, `RACK_INITIAL_RTO=1000000`,
`PACE_MAX_IP_BYTES=65536`, hybrid pacing flags and TRK flags. No donor code is
copied.
