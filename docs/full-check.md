<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Full check on the post-gate tree (K477)

Base: merged `origin/master` `c185d551`; `kevin/K477` then merged `origin/master` `80cf30b3`
(K479 `a345c163`, K475, K463) to pick up the tcpip fix. Branch: `kevin/K477`.

## Cap arithmetic

The prior full-check evidence under `tests/evidence/release-host/*/host-check/make-check.log`
records 173.72 s, 176.87 s, 192.11 s and 245.16 s. The slowest prior run is 245.16 s; a cold
target-clippy pass is the variable cost. The cap is **900 s = 3.67 × 245.16 s**, recorded before
the first run. Every run stayed inside it.

## Run 1 — before the two kernel fixes

`timeout 900 cargo xtask check` → exit 1, wall **311.10 s**.

| Step | Exit | Time |
|---|---:|---:|
| guard | 0 | 12.56 s |
| fmt | 0 | 12.42 s |
| clippy (host) | 0 | 48.17 s |
| tests (host, parallel) | 0 | 116.64 s |
| tests (doc) | 0 | 25.85 s |
| profiles | 0 | 0.20 s |
| provenance | 0 | 62.33 s |
| clippy kernel x86_64 | **101** | 3.82 s |
| clippy kernel aarch64 | **101** | 4.80 s |
| clippy user programs x86_64 | **101** | 11.23 s |
| clippy loader x86_64 | 0 | 10.28 s |
| docs | 0 | 1.78 s |

Reds: `kernel/src/sched/context.rs:625` (missing safety comment),
`kernel/src/syscall/process_ctl.rs:303` (unnecessary `u64 -> u64` cast) and
`crates/raven-tcpip/src/mptcp_hooks.rs:23` (unused `RefCell` import).

## Run 2 — after the two K477 kernel fixes

`timeout 900 cargo xtask check` → exit 1, wall **418.49 s**.

| Step | Exit | Time |
|---|---:|---:|
| guard | 0 | 31.91 s |
| fmt | 0 | 24.65 s |
| clippy (host) | 0 | 2.46 s |
| tests (host, parallel) | 0 | 103.68 s |
| tests (doc) | 0 | 6.31 s |
| profiles | 0 | 0.16 s |
| provenance | 0 | 197.30 s |
| clippy kernel x86_64 | **0** | 24.44 s |
| clippy kernel aarch64 | **0** | 12.22 s |
| clippy user programs x86_64 | **101** | 8.84 s |
| clippy loader x86_64 | 0 | 1.07 s |
| docs | 0 | 3.63 s |

The two kernel reds were gone; only the K479 tcpip import remained.

## Run 3 — after merging `origin/master` `80cf30b3`

`timeout 900 cargo xtask check` → exit 1, wall **374.52 s**.

| Step | Exit | Time |
|---|---:|---:|
| guard | 0 | 6.36 s |
| fmt | 0 | 11.74 s |
| clippy (host) | **101** | 3.53 s |
| tests (host, parallel) | 0 | 175.45 s |
| tests (doc) | 0 | 6.92 s |
| profiles | 0 | 0.26 s |
| provenance | 0 | 158.59 s |
| clippy kernel x86_64 | 0 | 0.74 s |
| clippy kernel aarch64 | 0 | 0.88 s |
| clippy user programs x86_64 | 0 | 7.57 s |
| clippy loader x86_64 | 0 | 0.85 s |
| docs | 0 | 0.94 s |

The K479 tcpip red is gone and both kernel steps are green. A new host red appeared in a file
outside K477 Owns:

```text
error: function `encode_fdmap` is never used
  --> crates/raven-spawn/src/lib.rs:1631:4
   = note: `-D dead-code` implied by `-D warnings`
```

## Remaining red and owner

`crates/raven-spawn/src/lib.rs:1631` — `encode_fdmap` is gated with
`#[cfg(any(feature = "alloc", test))]`, but its only caller, `spawn_elf`, is gated with
`#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]`. With `alloc` enabled by
feature unification and host `target_os = "linux"`, the function is compiled without its caller,
so `-D dead-code` fires. The file was last changed by K463 (`a69bb8855`, "build raven-spawn
without alloc and keep probe allocators"), owned by **stuart**. K477 does not own
`crates/raven-spawn` and did not edit it. The fix is to gate `encode_fdmap` with the same caller
cfg (or the equivalent `alloc` + target cfg), not to add an allow.

## Fixes made by K477

### `kernel/src/sched/context.rs:625` — safety comment

Added the live-trap-frame invariant:

```rust
// SAFETY: `frame` is the live trap frame the syscall stub passed in `rdi`; the
// enclosing `!frame.is_null()` check proves it is non-null, and the stub keeps the
// frame alive for the duration of this call.
let pc = unsafe { (*frame).rip };
```

Mutation: removing the comment restores `unsafe block missing a safety comment` at
`context.rs:625:18` (`/tmp/opencode/k477-mut-safety.log`).

### `kernel/src/syscall/process_ctl.rs:303` — redundant cast

Changed `cpu_runqueue as u64,` to `cpu_runqueue,`.

Mutation: restoring the cast restores `casting to the same type is unnecessary (u64 -> u64)` at
`process_ctl.rs:303:13` (`/tmp/opencode/k477-mut-cast.log`).

Both were restored; targeted kernel clippy is green on x86_64 and aarch64.

## Reviewer/owner action

1. stuart/K463: make `encode_fdmap`'s cfg match `spawn_elf` (the host `alloc` build must not
   compile it without its caller), or otherwise remove the dead-code warning without an allow.
2. Rerun `timeout 900 cargo xtask check`; K477 is DONE only on exit 0.
3. `make check-fast` remains green but skips target clippy; it is not the release gate.

No behaviour change was made by K477.
