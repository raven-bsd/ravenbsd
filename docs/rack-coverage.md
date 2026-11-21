# RACK donor-function coverage

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only checker for the K269 full RACK acceptance. The package is `tests/rack-coverage`
(`cargo nextest run -p rack-coverage`); it never edits product source.

## Donor

FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`,
`sys/netinet/tcp_stacks/rack.c` (BSD-2-Clause). The pinned file has **219 top-level function
definitions**. The checked-in `fixtures/rack-functions.txt` records their line/name manifest so the
parser can be compared on any host; the CLI also parses the real donor when given its path.

## Matrix format

The checker reads the first markdown table whose header names a function, Rust, test and mutation
column. A `Status` column is required for acceptance; a table without it (such as the historical
K269 report table) is parsed but every row is reported `not PASS`.

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `helper` | `crate::helper` | `helper_returns_one` | condition inverted -> test fails | PASS |
| `public_fn` | `crate::public_fn` | `public_fn_returns_length` | branch removed -> test fails | PASS |

Rules:

* every donor function appears exactly once;
* `Rust`, `Test` and `Mutation` are non-empty (`-`, `GAP`, `UNMAPPED`, `N/A`, `TODO`, `BLOCKED`
  and empty cells are failures);
* `Status` is `PASS` (case-insensitive);
* rows naming a function absent from the donor are `unknown` failures.

## Running

```text
cargo run -q -p rack-coverage -- \
  --donor /path/to/rack.c \
  --matrix /path/to/K269.md \
  --manifest tests/rack-coverage/fixtures/rack-functions.txt
```

The CLI prints the donor count, matrix rows, the missing list and one line per failure, then
`result: PASS` or `result: FAIL`. It exits non-zero on any failure or a manifest mismatch.

## Current K269 status

K269 is still BLOCKED on the literal 219-function port. Its report carries the 20 core mappings
(`rack_ack_received`, `rack_process_ack`, ...); the rest of the donor functions are not ported. The
checker is expected to report the remaining gap until K269 fills the matrix and marks every row
`PASS`. That is the checker's purpose, not a K315 failure.
