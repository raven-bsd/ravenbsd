<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Full check on the post-gate tree (K477)

Command on merged `origin/master` `c185d551`:

```
timeout 900 cargo xtask check
```

No `--fast`, no `--changed`. The run took **311.10 s** (exit 1) and executed every step.

## Cap arithmetic

The prior full-check evidence under `tests/evidence/release-host/*/host-check/make-check.log`
records 173.72 s, 176.87 s, 192.11 s and 245.16 s. The slowest prior run is 245.16 s; a cold
target-clippy pass is the variable cost. `900 s = 3.67 × 245.16 s`, recorded before the run. The
actual 311.10 s is 1.27× the slowest prior run and well inside the cap.

## Step table

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

`check: FAILED`; wall time 311.10 s.

## Reds found

1. **`kernel/src/sched/context.rs:625:18`** — `unsafe block missing a safety comment`
   (`clippy::undocumented_unsafe_blocks`, kernel x86_64). The block is
   `let pc = unsafe { (*frame).rip };`. Owning lane: **stuart** (x86-64 kernel/scheduler).
2. **`kernel/src/syscall/process_ctl.rs:303:13`** — `casting to the same type is unnecessary
   (u64 -> u64)` (`clippy::unnecessary_cast`, kernel x86_64 and aarch64). The expression is
   `cpu_runqueue as u64,`; clippy suggests `cpu_runqueue`. Owning lane: **stuart**
   (kernel syscall/process control).
3. **`crates/raven-tcpip/src/mptcp_hooks.rs:23:5`** — `unused import: core::cell::RefCell`
   (`unused_imports` via `-D warnings`, user programs x86_64). Owning lane: **carl**
   (`raven-tcpip` Illumos/Solaris satellite).

All three are outside K477 Owns. None was silenced, allowed or edited. The host clippy pass,
host tests, doc tests, profiles, provenance, loader clippy and docs are green; the userland
`userland/bin/**` crates were checked in the failing user-program pass and produced no error
(the only user-program error is `raven-tcpip`).

## Fixes made

None. K477 Owns (`xtask/src/**`, `kernel/src/log.rs`, `kernel/src/smp/selftest.rs`,
`userland/bin/**`, docs) had no red in this run, so there was no code fix and no mutation to
revert. The three reds above are the complete escalation set from this run.

## Reviewer/owner action

- stuart: add the missing `// SAFETY:` comment at `kernel/src/sched/context.rs:625` (the invariant
  is the live trap frame) and remove the redundant cast at
  `kernel/src/syscall/process_ctl.rs:303`.
- carl: remove the unused `use core::cell::RefCell;` at
  `crates/raven-tcpip/src/mptcp_hooks.rs:23`, or use it if the hook needs interior mutability
  (the current file does not).
- After both lanes land, rerun `timeout 900 cargo xtask check`; the full gate is the release
  evidence, not `make check-fast`.

No behaviour change is proposed by this ticket.
