# RACK misc/transport audit

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Independent audit of the merged K334 (`rack_misc.rs`) and K335 (`rack_transport.rs`) groups
against FreeBSD `rack.c` / `tcp_rack.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause). Test-only; no product source is
edited. Package: `tests/rack-audit-misc-transport`.

## Group coverage

The group has 37 reported rows: K334 28, K335 9. All 37 donor names are present in the merged
sources, all 37 have a named test, and all 37 exist in the 219-function K315 manifest. The group
has **36 unique functions** because `rack_pru_options` is claimed by both groups.

K334 tests: 28; K335 unit tests: 13; K335 public-surface tests: 2. K334 tests the private
`rack_process_option` dispatcher, but its donor row belongs to K332 and is not asserted here.

## Findings

| ID | Finding | Donor evidence | Merged Rust / evidence | Action |
|---|---|---|---|---|
| K355-F01 | `rack_pru_options` is claimed by both K334 and K335 | `rack.c:24041` | `rack_misc.rs` and `rack_transport.rs` both implement and test it | K334 should drop the row; K335 is the transport callback owner |
| K355-F02 | `RACK_FREE_CACHE` is wrong | `rack.c:272 static const int32_t rack_free_cache = 2` | `rack_misc.rs` uses 4; tests assert 4 | Correct to 2 and add a donor-value test |
| K355-F03 | `RACK_FREE_CNT_MAX` is wrong | `tcp_rack.h:611 #define RACK_FREE_CNT_MAX 0x2f` (47) | `rack_misc.rs` uses 8; tests only assert `<= 8` | Correct to 47 and test the donor boundary |
| K355-F04 | `RACK_DEFERRED_MAX = 16` is invented | `rack_add_deferred_option` allocates an unbounded `deferred_opt_list`; no such donor constant | `rack_misc.rs:34`; `rack_add_deferred_option_is_bounded` locks the model-only limit | Add a recorded divergence and reviewer approval, or remove the limit |
| K355-F05 | K334 has no per-function mutation evidence in its report | K334 contract required one per function | K334 report shows only `RACK_FREE_CACHE 4 -> 5`; no durable K315 mutation table | Supply the 28 per-function mutations or correct the claim |
| K355-F06 | K315 union has one duplicate | - | K334 28 + K335 9 = 37 rows, 36 unique | Remove the K334 `rack_pru_options` row |

No wrong K335 transport constant was found. K335's nine functions each have a named test and a
recorded mutation.

## K315 union

Raw group union (37 rows, duplicate retained):

```text
donor: 219 function(s)
matrix: 37 row(s)
manifest: 219 entries match
result: FAIL (183 missing, 1 duplicate, 0 unknown, 0 empty Rust, 0 empty test,
              0 empty mutation, 0 not PASS)
```

Corrected union (K334 `rack_pru_options` removed, 36 rows):

```text
donor: 219 function(s)
matrix: 36 row(s)
manifest: 219 entries match
result: FAIL (183 missing, 0 duplicate, 0 unknown, 0 empty Rust, 0 empty test,
              0 empty mutation, 0 not PASS)
```

The 183 missing functions belong to K269/K328-K336 groups that are still open; the union cannot
report `PASS` until those rows land. The audit's own group rows are all `PASS`.

## Running

```text
timeout 180 cargo nextest run -p rack-audit-misc-transport
timeout 420 make check-fast
```

The audit fixtures are `fixtures/group.tsv` (owner, donor name, Rust path, test, mutation status)
and `fixtures/constants.tsv` (donor value, Rust source, expected match/mismatch/invented status).
