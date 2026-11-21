<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image A/B reproducibility

Date: 2026-11-19. Ticket: K146 (Otto). Build evidence, no product code.

## Why

The release gate needs two clean builds of the release image at the same commit to produce
byte-identical output; otherwise the image hashes in the release packet are not evidence. This is
a repeatable A/B check, not a one-off hash.

## Usage

```
timeout 720 sh scripts/release-repro.sh
```

The script requires a **clean worktree** at the commit under test. It builds the x86_64 release
image twice through the product path, never reusing or copying one build for the other:

```
cargo xtask image --arch x86_64 --profile release --output build/release/repro/a.img
cargo xtask image --arch x86_64 --profile release --output build/release/repro/b.img
```

`SOURCE_DATE_EPOCH` is the standard reproducible-build input; if the caller does not set it, the
script uses the HEAD commit time (`git log -1 --format=%ct`) and records it.

## Output (`build/release/repro/`, not committed)

| File | Content |
|---|---|
| `a.img`, `b.img` | the two release images |
| `build-a.log`, `build-b.log` | full build logs |
| `builds.tsv` | build, UTC start/end, elapsed ms, size, sha256, rc |
| `summary.txt` | commit, branch, epoch, host state, exact commands, build rows, equality |
| `sha256.txt` | both hashes |
| `cmp.txt`, `cmp-first-bytes.txt`, `a-range.txt`, `b-range.txt` | only on a mismatch |

The terminal prints one `release-repro: PASS ...` or `release-repro: FAIL ...` line with the
commit, sizes and hashes. On a mismatch the script records the first differing byte offset (from
`cmp -l`), a hex excerpt around it from both images, and a `cmp`/`sha256sum` excerpt; it never
relaxes the comparison.

Exit: `0` byte-identical, `1` mismatch/build failure/dirty tree, `2` bad invocation. A cap expiry
(the acceptance `timeout 720`) is a failure/BLOCKED with the preserved build log, not a retry with
a larger cap. The cap arithmetic is two measured cold builds plus margin: `2 * 315232 ms +
90000 ms = 720464 ms`, hence the 720 s acceptance cap. In a loaded window the measured build is
slower; the cap is not raised to hide that.

## Wave base and after the fix

Run once at the wave base (`d1ad5ca8`) for the A/B table. K139 (loader maps the initfs libc copy
zero-copy) changes the release userland and initfs; **re-run the same command after K139 is
merged**. That after-fix run is the release evidence; the wave-base run is this ticket's
acceptance. Record both tables, the commit and `SOURCE_DATE_EPOCH`.

## Host state

`summary.txt` records `nproc`, the 1/5/15 load averages and the number of QEMU processes at the
start. A loaded-host sample is not an idle-host PASS; the A/B comparison itself is CPU-work, not
a timing gate, so the elapsed times are informational.

## Wave-base attempt (2026-11-19)

At `f382f0a6` (base `d1ad5ca8`), `timeout 360 sh scripts/release-repro.sh` hit the ticket cap:

- build A: **315232 ms**, 335544320 bytes, sha256 `6800ffc260aa484d0813b137324cdf0ab0ca8209c48dc0c61806b15902e343ae`;
- build B: still compiling when the 360 s cap expired; no `b.img` was produced.

Host state at start: load 10.70/8.92/11.46, one other QEMU. At the cap: load 31.80, two QEMU,
cargo/rustc active. No A/B conclusion; per the rule the cap expiry is a failure/BLOCKED, not a
retry. The after-K139 run (below) remains the release evidence.

## After-K139 attempt (2026-11-21)

K139 is merged in current master (`63efc104`). The after-K139 run at
`978899da233994fe46d4fe7e7ff94f98ce633f94` used the K150 measured 720 s cap:

```
timeout 720 sh scripts/release-repro.sh
```

`SOURCE_DATE_EPOCH=1795274353` (HEAD commit time), branch `otto/K150`, nproc 20, host load
20.69/25.02/35.37 at start, 3 QEMU processes.

| Build | Start UTC | End UTC | Elapsed ms | Size | SHA-256 | rc |
|---|---|---|---:|---:|---|---:|
| A | 2026-11-21T15:27:21Z | 2026-11-21T15:35:43Z | 502012 | 335544320 | `3a45fb20c371dbbddb32cd9550a5023b478f0ce9b62421c6ab8ae6b407e7e97e` | 0 |
| B | - | not completed at cap | - | - | - | - |

Result: **BLOCKED (loaded window / cap expiry).** Build A alone took 502 s, not the measured
315 s; two such builds would need about `2 * 502012 + 90000 = 1094 s`, above the 720 s cap. The
script did not produce `b.img`, so there is no A/B equality result and no after-K139 release
evidence yet. The cap was not raised and the run was not retried. A quiet-window rerun with the
same 720 s cap is required; the wave-base pair remains the 360 s cap failure above.

## Donor reconciliation

No donor code is involved: this is build evidence and a shell comparison. DR-52 is not applicable
(per the ticket).
