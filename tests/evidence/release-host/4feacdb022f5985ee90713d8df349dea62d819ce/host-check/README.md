<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Host check evidence: `4feacdb022f5985ee90713d8df349dea62d819ce`

Date: 2026-11-21 (UTC). Ticket: K416. No QEMU and no qtest was run.

## Commands

| Command | Exit | Elapsed | Result |
|---|---:|---:|---|
| `timeout 3600 make check` | 2 | 245.16 s | `check: FAILED` (three target-clippy steps) |
| `timeout 900 make check-fast` | 0 | 188.30 s | `check-fast: PASSED` |

`make check` is `cargo xtask check`: guard, fmt, host clippy, host nextest, doc tests, profiles,
provenance, target clippy (kernel x86_64/aarch64, user programs x86_64, loader x86_64) and docs.
The make wrapper returns 2 when xtask returns 1.

## Host test result

```text
Starting 13009 tests across 346 binaries (4 tests skipped)
Summary [ 119.993s] 13009 tests run: 13009 passed (3 slow), 4 skipped
```

No host test failed. The 4 skipped tests are pre-existing nextest skips in the workspace, not a
change in this audit. The full run did not use `--retry`; there is one run.

## Failures

| Package | Step | Error | File:line | Proposed owner |
|---|---|---|---|---|
| `raven-kernel` | `clippy kernel x86_64` | `clippy::too_many_arguments` (8/7) | `kernel/src/process/exec.rs:63:1` | K418 |
| `raven-kernel` | `clippy kernel aarch64` | `clippy::too_many_arguments` (8/7) | `kernel/src/process/exec.rs:63:1` | K418 |
| `raven-devd` | `clippy user programs x86_64` | `clippy::unnecessary_sort_by` | `userland/servers/devd/src/pci.rs:46:5` | K419 |

Both lints are `-D warnings` failures, not compile errors. Branch `phil/K416` is exactly
`origin/master` `4feacdb0` with no product diff (`git diff --stat origin/master` is empty), so both
are pre-existing on the merged tree. `kernel/src/process/exec.rs:63`
`replace_image_from_user` came from K220 (`fbd1b1cb`); `userland/servers/devd/src/pci.rs:46` came
from K179 (`5822ca45`).

## Files

| File | Contents |
|---|---|
| `make-check.log` | Complete `make check` output, 1083 lines |
| `check-fast.log` | Complete `make check-fast` output, 867 lines |
| `summary.tsv` | Per-step exit code, seconds and result |
| `failures.tsv` | Machine-readable failure rows and proposed owners |
| `SHA256SUMS` | Hashes of this directory's files |

`make check-fast` passes because `--fast` skips the target clippy passes that fail in `make check`.
That is the same blind spot K395 recorded for `check-fast`; it is not evidence that the full gate
is green.
