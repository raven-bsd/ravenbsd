<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK replay/union audit

Status: K357, 2026-11-21. Evidence tags: **OBSERVED**, **PROPOSED**, **UNVERIFIED**.

`tests/rack-audit-replay-union` is an independent audit of the merged RACK replay/union evidence.
It reads the maintained `docs/rack-coverage.md` register, the live group reports, the pinned
219-function manifest and the merged `raven-tcpip` source. Findings are recorded as data; the
audit test is green because it detects them, while the owning groups must fix them.

## 1. Register and union shape

| View | Donor | Rows | Missing | Duplicate | Unknown | PASS | Not PASS |
|---|---:|---:|---:|---:|---:|---:|---:|
| `docs/rack-coverage.md` (K315) | 219 | 219 | 0 | 0 | 0 | 45 | 174 |
| live group-report union | 219 | 219 | 0 | 0 | 0 | 111 | 108 |

The group union additionally has **73** empty Rust/test/mutation rows and **60**
cross-report-duplicate functions. The K338 register snapshot had 174 assigned rows; the live
union has advanced because more group reports landed.

The ticket says 218 rows; the pinned parser and manifest are **219**. The register keeps all 219
and records the one-row discrepancy.

## 2. Exact discrepancies

### Wrong donor constants

| Constant | Donor (`tcp_var.h`/`tcp_rack.h`) | Merged | Finding |
|---|---|---|---|
| `TCP_FUNC_OUTPUT_CANDROP` | `0x02` | `0x01` | wrong value |
| `TCP_FUNC_DEFAULT_OK` | `0x04` | `0x02` | wrong value |
| `RACK_FREE_CNT_MAX` | `0x2f` (47) | `8` | wrong value |

These match the K337 vtable finding (`tfb_flags` expected 6, got 3) and add the misc free-list
constant.

### PASS row without a merged Rust function

| Donor row | Register mapping | Merged source |
|---|---|---|
| `rack_cong_signal` | `Rack::cong_signal` | no `fn cong_signal` in `crates/raven-tcpip/src` |
| `rack_update_prr` | `Rack::rack_update_prr` | no `fn rack_update_prr` in `crates/raven-tcpip/src` |

K338 already flagged these as the only K269 `PASS` rows; this audit shows the Rust mapping is
stale: the functions are not in the merged tree. K269/K333 must correct or remap them.

### Union gaps

The live group union still has 108 non-PASS functions, 73 rows with empty Rust/test/mutation and 60
cross-report duplicates. This matches the K337 red replay classes and the K338 assignment
findings; the owning groups are K269/K328-K336. There are **no missing, duplicate, unknown or
empty-cell failures in the 219-row shape itself**.

## 3. Replay findings

`cargo run -q -p rack-replay -- --replay` is still red for the known classes:

| Class | Mismatches | First mismatch |
|---|---:|---|
| loss | 7948/105000 | seed 1 pipe 2000 vs 1000 |
| recovery | 1025/121925 | seed 11 pipe 1000 vs 0 |
| tlp | 1747/10000 | DSACK reo_wnd 25 vs 50 |
| bbr | 10000/10000 | no product BBR/timely/pacing engine |
| vtable | 1/10000 | tfb_flags 6 vs 3 |

These are the same findings as K337; K357 records them as still open, not fixed.

## 4. Commands and K315

```sh
timeout 180 cargo nextest run -p rack-replay
timeout 120 cargo run -q -p rack-replay -- --replay
timeout 120 cargo run -q -p rack-replay -- --union --reports <tickets>
timeout 120 cargo run -q -p rack-coverage -- \
  --donor /tmp/opencode/fbsd-rack.c \
  --matrix docs/rack-coverage.md \
  --manifest tests/rack-coverage/fixtures/rack-functions.txt
timeout 180 cargo nextest run -p rack-audit-replay-union
```

Observed K315 register line:

```text
result: FAIL (0 missing, 0 duplicate, 0 unknown, 174 empty Rust, 174 empty test,
              174 empty mutation, 174 not PASS)
```

Observed union line:

```text
union: donors 219 rows 219 groups K269,K328,K329,K330,K331,K332,K333,K334,K335,K336
missing=0 duplicate=0 unknown=0 empty_rust=73 empty_test=73 empty_mutation=73
not_pass=108 cross_report_duplicate=60
```

## 5. Audit mutation/negative checks

| Check | Result |
|---|---|
| Synthetic missing row | `check` reports `missing: ["beta"]` |
| Synthetic duplicate row | `check` reports `duplicates: ["alpha"]` |
| Synthetic blank test | `check` reports `empty_test: ["alpha"]` |
| PASS mapping audit | detects `rack_cong_signal` and `rack_update_prr` as stale |
| Constant audit | detects the three wrong values |
| Full audit tests | 5 passed |

## 6. Donor reconciliation

Test-only audit of FreeBSD `sys/netinet/tcp_stacks/rack.c` and `tcp_rack.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2). No donor code is copied; the audit reads the
function manifest, the constants and the merged Rust source only.

## 7. Gaps

* The group reports are live; the 108/73/60 union counts are the snapshot at
  `eee05a07` and move as K328-K336 land. Rerun the audit for the current count.
* The K315 register has 174 assigned rows; the audit does not invent mappings for them.
* Two PASS rows are stale and three constants are wrong; the owning groups must correct them.
* The replay classes are red for the reasons above; K357 does not weaken or hide them.
