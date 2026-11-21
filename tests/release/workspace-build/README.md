<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Workspace test-build gate (K400)

`make check-build` runs `tests/release/workspace-build/check.sh`. K395's
`tests/release/workspace-build/run.sh` is the authoritative per-package sweep; K400's wrapper
classifies its results against the K395 audit and fails on any unowned compile failure.

Why: `make check-fast` builds only the changed packages. K394 showed the blind spot when pristine
master could not compile the `fsd-ufs` tests (`E0063`, missing `root_len`) while `check-fast`
stayed green. K400 adds this pre-tag gate without changing `check-fast`.

## Pre-tag sequence

1. `make check-fast` — fast changed-package gate.
2. `make check-build` — every workspace test target compiles; only the named exclusions/owners
   below may fail.
3. the QEMU/release qtests on the frozen image.

## Classification (K395 audit)

Source: `docs/releases/0.1.0-workspace-build-audit.md`. The explicit list is
`tests/release/workspace-build/exclusions.tsv`; the gate reads it and never guesses.

| Class | Count | Behaviour |
|---|---:|---|
| compiled | 405 | pass |
| K401 host-build exclusions | 71 | expected; target-none/bare fixtures and kernel, not meaningful under the host test build |
| K394 `fsd-ufs` | 1 | owned failure; the gate stays green while K394 is open |
| K402 `raven-blockd`, `raven-ramfsd` | 2 | owned failure; same `Ctx.root_len` gap |
| K403 `raven-pi4-platd` | 1 | owned failure; host link duplicate `_start` |
| unowned | 0 | **the gate exits 1 and names the package with file:line** |

Any package not in the explicit list that fails to compile is unowned and blocks the pre-tag
checkpoint. The gate does not rely on Cargo's first-error stop: the K395 runner builds each
package separately.

## Output

- `check-build: N packages: P pass, E host-build exclusions, O owned, U unowned`.
- One `OWNED <pkg> (<ticket>): <first error> (<file:line>)` line per owned failure.
- One `FAIL <pkg>: <first error> (<file:line>)` line per unowned failure; exit 1.
- Full per-package logs: `build/release/workspace-build/<package>.log` (the path is printed).

## Runtime

K395's audit measured **428.48 s** for 480 packages on the audit host. The ticket cap is
**1800 s** (cold + margin); the per-package cap is 120 s.

## Current status

On the K395 audit base the gate is green with exactly the four owned failures (K394, K402, K403)
and the 71 K401 exclusions. It turns red if any unowned package fails; the gate's mutation proof
removes the `fsd-ufs` K394 entry and shows the gate failing on the same E0063.
