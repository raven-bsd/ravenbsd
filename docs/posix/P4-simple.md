<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P46 - POSIX asa and cal

First-party Rust implementations of `asa` and `cal`, with host tests and executable evidence
rows.

## Tool matrix

| Tool | Covered | Tests |
|---|---|---:|
| `asa` | space/`0`/`1`/`+` carriage controls, unknown controls, first-line rule, empty and unterminated input, bytes, stdin/files, option rejection | 18 lib |
| `cal` | current month, `month year`, one-operand year, `-y`, `-3`, leap years, Gregorian month lengths, weekday/epoch conversion, grid layout, rollover and usage errors | 23 lib |

## Method

* `asa` keeps a byte-level interpreter in `userland/bin/asa/src/lib.rs`. The target wrapper reads
  file operands with `raven_rt::fs` and the standard input with the console read; both feed the
  same interpreter. The first line suppresses the leading advance, matching the printer model.
* `cal` keeps a pure proleptic Gregorian model in `userland/bin/cal/src/lib.rs`: leap-year rule,
  month lengths, first-weekday calculation, Unix-to-civil conversion and the one-, three- and
  twelve-month layouts. The target wrapper supplies the current month from
  `clock_get(REALTIME)`.
* Both packages are workspace members with a host-testable library and a target-only binary; the
  host binary is a no-op, as with `more` and `write`.

## Limits

| Item | Detail |
|---|---|
| `cal` current month | protocol version 1 has no wall clock: `clock_get(REALTIME)` is 0, so the no-operand view is January 1970 until the kernel provides realtime. Explicit `month year` is unaffected |
| `cal` calendar | proleptic Gregorian only; the 1752 British reform gap and Julian dates are out of scope |
| `cal` locale | C-locale English month and weekday names only |
| `cal -3` range | the previous/next months need years 2 through 9998 |
| `asa` input | the target standard input is the console; there is no generic stdin handle |
| `asa` output | printable text is written through the console; control bytes are emitted as-is |

## Acceptance

```
sh tests/posix/utility/run-simple.sh
cargo nextest run -p asa -p cal
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

The runner reports `2 pass, 0 fail`; both rows are registered in
`docs/posix/evidence/P4-simple.toml`. No board claim.
