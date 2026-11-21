<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Workspace test-build gate (K400)

`make check-build` runs `tests/release/workspace-build/run.sh`. It compiles every host workspace
test target with `cargo test --no-run --offline --locked` package-by-package, never executes a
test and never touches the network.

Why: `make check-fast` builds only the packages changed by the branch. K394 showed the blind spot
when a pristine master could not compile the `fsd-ufs` tests (`E0063`, missing `root_len`) while
`check-fast` stayed green. K400 adds this gate without changing `check-fast`.

## Pre-tag sequence

1. `make check-fast` — fast changed-package gate.
2. `make check-build` — every host workspace test target compiles.
3. the QEMU/release qtests on the frozen image.

## Output

- Concise per-package failures with the first compiler error and `file:line`.
- Pass/fail per top-level group (`crates`, `libc`, `userland`, `tests`, `kernel`, `boot`, `tools`,
  `xtask`).
- Full per-package logs: `build/release/check-build/<package>.log`.
- Exit code 0 only when every package test target compiles.

A package that times out at `CHECK_BUILD_PACKAGE_TIMEOUT` (default 180 s) is a failure, not a
silent skip.

## Measured runtime (2026-11-21, base `4371e8ea`)

| Run | Packages | Result | Seconds |
|---|---:|---|---:|
| cold, first full workspace test build in this clone | 390 | 7 failures (incl. kernel/bare fixtures before the filter) | 569.42 |
| warm, filtered host workspace | 382 | 3 failures | 121.89 |

The ticket cap is **1800 s** (cold + margin). The warm gate is ~2 minutes; the per-package
`CHECK_BUILD_PACKAGE_TIMEOUT` bounds a single crate at 180 s.

## Current master status

On `4371e8ea` the gate is red on the K394/K395 class even though every other group passes:

```text
check-build: FAIL fsd-ufs: error[E0063]: missing field `root_len` (userland/servers/fsd-ufs/src/handler_ut.rs:149:5)
check-build: FAIL raven-ramfsd: error[E0063]: missing field `root_len` (userland/servers/ramfsd/src/handler_ut.rs:29:5)
check-build: FAIL raven-blockd: error[E0063]: missing field `root_len` (userland/servers/blockd/src/handler_ut.rs:22:5)
```

That is the gate doing its job: `check-fast` does not see these unchanged packages. K394 owns the
`fsd-ufs` fix; the `raven-ramfsd`/`raven-blockd` rows are the same regression and need the same
one-line initializer update. The gate stays in the pre-tag sequence and must be green before tag.
