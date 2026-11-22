<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Full check on the post-gate tree (K477)

Base: merged `origin/master` `c185d551`. Branch: `kevin/K477`.

## Cap arithmetic

The prior full-check evidence under `tests/evidence/release-host/*/host-check/make-check.log`
records 173.72 s, 176.87 s, 192.11 s and 245.16 s. The slowest prior run is 245.16 s; a cold
target-clippy pass is the variable cost. The cap is **900 s = 3.67 × 245.16 s**, recorded before
the first run. Both runs stayed inside it.

## Run 1 — before the two kernel fixes

```
timeout 900 cargo xtask check
```

Exit 1, wall 311.10 s.

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

## Run 2 — after the two K477 fixes

```
timeout 900 cargo xtask check
```

Exit 1, wall 418.49 s (1.71× the slowest prior run, inside the 900 s cap).

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

The two kernel reds are gone. The only remaining red is the K479 file:

```text
error: unused import: `core::cell::RefCell`
  --> crates/raven-tcpip/src/mptcp_hooks.rs:23:5
   = note: `-D unused-imports` implied by `-D warnings`
```

## Fixes made

### `kernel/src/sched/context.rs:625` — safety comment

Added the invariant for the specific block:

```rust
// SAFETY: `frame` is the live trap frame the syscall stub passed in `rdi`; the
// enclosing `!frame.is_null()` check proves it is non-null, and the stub keeps the
// frame alive for the duration of this call.
let pc = unsafe { (*frame).rip };
```

Mutation: removing the three comment lines and rerunning the kernel x86_64 clippy restores
`unsafe block missing a safety comment` at `context.rs:625:18` (log
`/tmp/opencode/k477-mut-safety.log`).

### `kernel/src/syscall/process_ctl.rs:303` — redundant cast

Changed `cpu_runqueue as u64,` to `cpu_runqueue,`.

Mutation: restoring `as u64` and rerunning the kernel x86_64 clippy restores
`casting to the same type is unnecessary (u64 -> u64)` at `process_ctl.rs:303:13` (log
`/tmp/opencode/k477-mut-cast.log`).

Both fixes were restored and the kernel x86_64 and aarch64 clippy steps are green.

## Remaining red and K479

`crates/raven-tcpip/src/mptcp_hooks.rs:23` is Phil's K479 and was not edited here. The fetched
`origin/phil/K479` head is `101434fd9` (`K479: add 25 more direct libc math tests`); it does not
touch `mptcp_hooks.rs`, so it does not clear this red. The full gate stays red until the actual
K479 change for that import lands. Do not add `#[allow(unused_imports)]`.

## Reviewer/owner action

1. Phil/K479: remove or use `core::cell::RefCell` at
   `crates/raven-tcpip/src/mptcp_hooks.rs:23`.
2. After K479 lands, rerun `timeout 900 cargo xtask check`; K477 is DONE only on exit 0.
3. `make check-fast` remains green but skips the target-clippy steps; it is not the release gate.

No behaviour change was made by K477.
