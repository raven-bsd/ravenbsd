<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK SACK/sockopt audit (K356)

Independent, read-only audit of the merged K336 SACK/DSACK group and the K340
sockopt/query matrix. The audit does not edit product source; findings go to the
group owners.

## Donor

FreeBSD `sys/netinet/tcp_stacks/rack.c` and `tcp_rack.h` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause). The SACK flags are
from `tcp_rack.h`; the sockopt defaults/clamps are from `rack_set_sockopt`,
`rack_get_sockopt` and `tcp.h`. No code is copied.

## Method

`tests/rack-audit-sack-sockopt` parses the merged `rack_sack.rs` constants and
`rack_misc.rs` duplicate definitions, compares them with the donor `tcp_rack.h`
values, checks the K269 round-7 ownership split against the K336 report's six
claimed rows, and exercises the K334 `RackMisc` surface for the K340 matrix
values. The CLI prints every finding and exits 1 when any is present.

```sh
timeout 60 cargo run -q -p rack-audit-sack-sockopt
timeout 180 cargo nextest run -p rack-audit-sack-sockopt
timeout 180 cargo nextest run -p raven-tcpip -E 'test(rack_sack)'
timeout 180 cargo nextest run -p rack-sockopt
```

## SACK findings

| Finding | Subject | Detail |
|---|---|---|
| `SACK-WRONG-CONSTANT` | `RACK_SACK_PASSED` | code `0x000002`; donor `tcp_rack.h` `0x000010` |
| `SACK-WRONG-CONSTANT` | `RACK_WAS_SACKPASS` | code `0x000004`; donor `0x000020` |
| `SACK-WRONG-CONSTANT` | `RACK_RWND_COLLAPSED` | code `0x000010`; donor `0x000100` |
| `SACK-WRONG-CONSTANT` | `RACK_WAS_LOST` | code `0x000008`; donor `0x400000` |
| `SACK-DUPLICATE-CONSTANT` | `RACK_ACKED`, `RACK_SACK_PASSED`, `RACK_WAS_SACKPASS`, `RACK_RWND_COLLAPSED`, `RACK_WAS_LOST` | defined in both `rack_sack.rs` and `rack_misc.rs`; the duplicate values disagree for four of them |
| `SACK-DUPLICATE-OWNER` | `rack_log_dsack_event` | K336 report claims it; K269 assigns it to K329 |
| `SACK-DUPLICATE-OWNER` | `rack_log_sack_passed` | K336 report claims it; K269 assigns it to K329 |
| `SACK-DUPLICATE-OWNER` | `rack_process_to_cumack` | K336 report claims it; K269 assigns it to K331 |
| `SACK-MISSING-OWNER` | `rack_handle_might_revert` | K269 assigns it to K336; the K336 report does not claim a row (the function lives in `rack_state.rs`) |

The 9/9 `rack_sack` tests pass because they use the module's own constants; they
therefore cannot detect the donor-value mismatch. This is the weak-test finding
for K336.

## Sockopt findings

| Finding | Subject | Detail |
|---|---|---|
| `SOCKOPT-DONOR-DIVERGENCE` | `TCP_RACK_MEASURE_CNT=0` | K334 clamps to 1; donor `rack.c:22985` returns `EINVAL` |
| `SOCKOPT-DONOR-DIVERGENCE` | `TCP_RACK_ABC_VAL=300` | K334 clamps to 255; donor `rack.c:22999` returns `EINVAL` |
| `SOCKOPT-DONOR-DIVERGENCE` | `TCP_RACK_PACE_MIN_SEG=70000` | K334 stores 65535; donor `rack.c:23241` masks `0xffff` -> 4464 |
| `SOCKOPT-DONOR-DIVERGENCE` | `TCP_RACK_PACING_BETA` set | K334 returns `EOPNOTSUPP`; donor `rack.c:22929` returns `EINVAL` |
| `SOCKOPT-DONOR-DIVERGENCE` | `TCP_INFO` get | K334 returns `EOPNOTSUPP`; donor `rack.c:24373` calls `rack_fill_info` |
| `SOCKOPT-MISSING-OPTION` | `TCP_RACK_PACING_BETA_ECN` (1139) | K334/K340 do not expose it; donor `tcp.h:322` / `rack.c:22945` handle it |

The K340 matrix itself passes 14/14; the findings are donor divergences that the
matrix records or omits.

## K315 row

```text
$ cargo run -q -p rack-coverage -- \
    --donor /tmp/opencode/k328-rack.c \
    --matrix tickets/tim/reports/K336.md \
    --manifest tests/rack-coverage/fixtures/rack-functions.txt
donor: 219 function(s)
matrix: 6 row(s)
manifest: 219 entries match
result: FAIL (213 missing, 0 duplicate, 0 unknown, 0 empty Rust, 0 empty test,
              0 empty mutation, 0 not PASS)
```

The six rows are well-formed and PASS, but three of them are K329/K331 rows in
the K269 ownership split, and the assigned `rack_handle_might_revert` row is
absent. The 213 missing rows belong to the other RACK groups.

## Negative check

`wrong_constant_is_detected_and_correct_one_is_accepted` feeds a fixture with
`RACK_SACK_PASSED: u32 = 0x10` (no finding) and then mutates it to `0x02`, which
the auditor reports with the observed/donor values. `missing_test_is_detected`
removes a test name and expects the `SACK-MISSING-TEST` finding. Both pass; a
correct fixture is not reported.
