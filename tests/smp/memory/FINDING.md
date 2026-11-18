<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S85b TLB shootdown finding (round 2)

The memory/TLB fixture is clean at user level, but the kernel loses TLB IPI acknowledgements
under concurrent user map/unmap and faults. This is a real lost acknowledgement, not a flake; it
blocks S85b until S83 fixes it.

## Reproducer

```sh
sh tests/smp/memory/run.sh tcg --iterations 1
sh tests/smp/memory/run.sh kvm2 --iterations 1
sh tests/smp/memory/run.sh kvm4 --iterations 1
```

The runner builds `tests/smp/memory`, assembles the image once, runs in the foreground and stops
on the first failure. Logs are under `build/smp-memory/<mode>/run-1/`.

## Current clean TCG evidence

Three independent clean-image runs:

| Run | duration_ms | `smp: tlb shootdown timeout` lines | result |
|---|---:|---:|---|
| run-1 | 10,382 | 100 | **FAIL** |
| diag-2 | 9,926 | 100 | **FAIL** |
| diag-3 | 10,871 | 100 | **FAIL** |

All three runs completed the user stress with
`alloc_ops=256 fault_pages=512 maps=32 forks=4 tlb_rounds=64 tlb_stale=0` and
`smp-memory: ok`; qtest failed only on the forbidden kernel timeout lines. Worst/p90 duration =
10,871 ms; worst/p90 timeout count = 100 (3 samples).

## Characterization (temporary instrumentation, reverted)

A temporary `kernel/src/memory/uspace.rs` wrapper logged the shooter CPU, call site and range
when `flush_range` returned false. It was removed after the diagnostic; no instrumentation is in
the committed tree.

- 50 failed flush call sites produced 80 target timeout lines in one run. A single flush can lose
  more than one target.
- All four CPUs were both shooters and targets: 0->1, 0->2, 0->3, 1->0, 1->2, 1->3, 2->0, 2->1,
  2->3, 3->0, 3->1, 3->2. There is no single dead AP.
- Sites: 41 `unmap` (anonymous 16-page unmap, `len=0x10000`), 8 `fault-cow` and 1
  `fault-protect` (one page, `len=0x1000`). The synchronized single-controller TLB remap rounds
  produced no timeout site.
- APs are online (`smp: 4 cpus online`, user threads on CPUs 0..3). The IPI gates are installed
  before AP start (`kernel/src/smp/bringup.rs:34`), and each AP loads the shared BSP IDT
  (`kernel/src/arch/x86_64/ap_start.rs:668`). This is not an offline-AP or missing-IDT case.
- The `SEQUENCE`/`PENDING`/`ACKED` mailbox logic checks out as a protocol: the timeout clears
  `PENDING[cpu]` and returns false, so a late IPI is ignored. The missing acknowledgement is real,
  not a double-count.

## Root cause location (S83 owner)

`kernel/src/smp/shootdown.rs:81` takes the global `LOCK` with `LOCK.lock()`. `LOCK` is an
`IrqSpinLock`; `kernel/src/sync.rs:200-207` disables interrupts before spinning for the lock. If
two CPUs call `flush_range` concurrently, one holds `LOCK` and waits for IPI acknowledgements;
the other spins on `LOCK` with interrupts masked and cannot take the TLB IPI. The 100 ms
deadline then fires. The comment at `shootdown.rs:108-110` covers a target spinning on the
address-space mutex, but not a target spinning on the shootdown `LOCK` itself.

This is `kernel/src/smp/**` (owner **stuart/S83**), not `kernel/src/memory/**` or
`crates/raven-vmem/**`; it must not be patched around in the memory layer. A safe fix must let a
CPU waiting for the shootdown lock take the TLB IPI, or avoid concurrent shootdown callers
masking interrupts while waiting.

## Gate after the fix

- TCG `-smp 4`: **50/50** clean runs, no `tlb shootdown timeout`.
- KVM `-smp 2` and `-smp 4`: clean after S83; currently both stop at `timer: 1000 Hz` with QEMU
  exit 0 and no SMP/user output.
- No retry may hide a timeout; the first failure remains a failure.
