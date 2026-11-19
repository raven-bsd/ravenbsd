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
timeout 360 sh scripts/release-repro.sh
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
(the acceptance `timeout 360`) is a failure/BLOCKED with the preserved build log, not a retry.

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

## Donor reconciliation

No donor code is involved: this is build evidence and a shell comparison. DR-52 is not applicable
(per the ticket).
