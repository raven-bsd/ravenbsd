<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Full check on the post-gate tree (K477)

Base: merged `origin/master` `c185d551`; `kevin/K477` merged `origin/master` `80cf30b3`
(`379657e9`). The final full run is **green**.

## Cap arithmetic

Prior full-check evidence under `tests/evidence/release-host/*/host-check/make-check.log` records
173.72 s, 176.87 s, 192.11 s and 245.16 s. The slowest prior run is 245.16 s; a cold target-clippy
pass is the variable cost. Cap **900 s = 3.67 × 245.16 s**, recorded before run 1. The final green
run took 477.05 s = 1.95× the slowest prior run and stayed inside the cap.

## Final step table (green run 5)

`timeout 900 cargo xtask check` → exit **0**, wall **477.05 s**.

| Step | Exit | Time |
|---|---:|---:|
| guard | 0 | 22.09 s |
| fmt | 0 | 43.51 s |
| clippy (host) | 0 | 16.39 s |
| tests (host, parallel) | 0 | 216.29 s |
| tests (doc) | 0 | 29.23 s |
| profiles | 0 | 0.41 s |
| provenance | 0 | 142.37 s |
| clippy kernel x86_64 | 0 | 0.33 s |
| clippy kernel aarch64 | 0 | 0.33 s |
| clippy user programs x86_64 | 0 | 3.61 s |
| clippy loader x86_64 | 0 | 0.32 s |
| docs | 0 | 0.46 s |

`check: PASSED`.

## Run history

- **Run 1** (311.10 s, exit 1): three reds — `kernel/src/sched/context.rs:625` missing safety
  comment, `kernel/src/syscall/process_ctl.rs:303` redundant `u64` cast, and
  `crates/raven-tcpip/src/mptcp_hooks.rs:23` unused import.
- **Run 2** (418.49 s, exit 1): the two K477 kernel fixes green; K479 tcpip red remained.
- **Run 3** after merging `80cf30b3` (374.52 s, exit 1): tcpip red gone; new host red
  `crates/raven-spawn/src/lib.rs` `encode_fdmap` dead code.
- **Run 4** (460.39 s, exit 1): cfg fix applied but `no_alloc_fdmap`'s targeted allow was removed;
  the user-program (ravenbsd target + alloc) build reported `no_alloc_fdmap` dead code. The allow
  was restored with an accurate reason.
- **Run 5** (477.05 s, exit 0): all 12 steps green.

## The `raven-spawn` cfg proof and fix

The production call site is `spawn_elf`, gated:

```rust
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn spawn_elf<S: Sys>(...
```

Inside it the fd-map branch and the call are:

```rust
#[cfg(any(feature = "alloc", test))]
let fdmap_info = if fdmap.is_empty() { ... } else {
    ...
    let block = match encode_fdmap(&entries) { ... };
```

Before the fix the function was gated only by `#[cfg(any(feature = "alloc", test))]`. In the
**host lib target with feature `alloc`, target_os = linux and not test**, `encode_fdmap` was
compiled while `spawn_elf` (its only caller) was not, so `-D dead-code` fired. With the `alloc`
feature unified by the workspace, this is exactly the host clippy failure in run 3.

The fix aligns the function with the caller's real configuration, the same shape as
`image_open_flags` and `no_alloc_fdmap`:

```rust
#[cfg(any(
    test,
    all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")
))]
fn encode_fdmap(...) -> Result<alloc_crate::vec::Vec<u8>, SpawnError> {
```

`no_alloc_fdmap` keeps its targeted `#[allow(dead_code)]`, because the ravenbsd target with
`alloc` still compiles it (target cfg) while the alloc branch uses `encode_fdmap`; it is only
called by the `#[cfg(not(any(feature = "alloc", test)))]` branch. The allow reason was corrected
to state that the ravenbsd target compiles it for the no-alloc path while the alloc build uses the
other function. It is not papering over `encode_fdmap`; that function's cfg is now correct.

Proofs:

| Command | Exit |
|---|---:|
| `cargo clippy -p raven-spawn --all-targets -- -D warnings` | 0 |
| `cargo clippy -p raven-spawn --all-targets --features alloc -- -D warnings` | 0 |
| `cargo build -p raven-spawn --no-default-features` | 0 |

Mutation: reverting the cfg to `#[cfg(any(feature = "alloc", test))]` and running
`cargo clippy -p raven-spawn --all-targets --features alloc -- -D warnings` returns
`function encode_fdmap is never used` at `crates/raven-spawn/src/lib.rs` (`k477-mut-spawn-cfg-alloc.log`).
Restoring the aligned cfg is green.

## K477 kernel fixes and mutations

### `kernel/src/sched/context.rs:625`

Added the live-trap-frame invariant:

```rust
// SAFETY: `frame` is the live trap frame the syscall stub passed in `rdi`; the
// enclosing `!frame.is_null()` check proves it is non-null, and the stub keeps the
// frame alive for the duration of this call.
let pc = unsafe { (*frame).rip };
```

Mutation: removing the comment restores `unsafe block missing a safety comment` at `625:18`
(`k477-mut-safety.log`).

### `kernel/src/syscall/process_ctl.rs:303`

Changed `cpu_runqueue as u64,` to `cpu_runqueue,`.

Mutation: restoring the cast restores `casting to the same type is unnecessary (u64 -> u64)` at
`303:13` (`k477-mut-cast.log`).

Both were restored; targeted kernel clippy is green on x86_64 and aarch64.

## Gates

| Command | Cap | Exit | Result | Artifact (sha256) |
|---|---:|---:|---|---|
| `cargo xtask check` (run 5, no `--fast`) | 900 | 0 | **PASSED**, 477.05 s | `/tmp/opencode/k477-check5.log` `806888b4…f6d5` |
| `make check-fast` | 900 | 0 | **check-fast: PASSED** (40.39 s) | `/tmp/opencode/k477-checkfast4.log` `3c26565c…46cc` |

No `#[allow]` was added for `encode_fdmap`; the only allow is the pre-existing `no_alloc_fdmap`
dead-code waiver, now with an accurate reason.

## Reviewer/owner action

None; the full gate is green. K486 is next.
