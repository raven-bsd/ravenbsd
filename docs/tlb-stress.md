<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TLB stress matrix runner (S120)

Status: **test harness for S113/S118**. It runs the deterministic concurrent-unmap cases and
records evidence; it does not change the kernel or relax the case expectations.

## Why

S118 added the x86_64 four-way concurrent unmap/TLB-shootdown case and S106 added the aarch64
counterpart. The close-out needs a matrix that is reproducible, stops on the first failure, keeps
the transcript and JSON for that exact iteration, records the host state, and reports latency
percentiles instead of ad-hoc shell loops. A failed sample is a result, not a reason to retry.

## Usage

```sh
scripts/tlb-stress.sh [--runs N] [--case NAME] [--quick] [--no-preflight] [--out DIR]
```

| Option | Meaning |
|---|---|
| `--runs N` | iterations per case (default 50); `--quick` defaults to 1 unless `--runs` is given |
| `--case NAME` | case to run; repeatable or comma-separated; default is all four cases below |
| `--quick` | one iteration per requested case (short smoke) |
| `--no-preflight` | skip `tests/smp/boot/host-idle.sh`; use only for a short smoke |
| `--max-load L` | pass `--max-load L` to the preflight (default: host-idle's `max(2, nproc/4)`) |
| `--wait S` | wait up to S seconds for an idle host before starting (default 0: refuse) |
| `--out DIR` | output directory (default `build/tlb-stress`); refuses to mix into an existing `results.tsv` |

Default matrix:

| Case | Arch | Accelerator / CPUs |
|---|---|---|
| `smp-tlb-x86-concurrent` | x86_64 | TCG, `-smp 4` |
| `smp-tlb-x86-concurrent-kvm2` | x86_64 | KVM, `-smp 2` |
| `smp-tlb-x86-concurrent-kvm4` | x86_64 | KVM, `-smp 4` |
| `smp-tlb-aarch64-concurrent` | aarch64 | TCG, `-smp 4` |

Examples:

```sh
# Pre-fix smoke: one x86 TCG iteration, preserves the expected first failure.
sh scripts/tlb-stress.sh --quick --no-preflight --case smp-tlb-x86-concurrent

# Full close-out after S113: 50 iterations per case on an idle host.
sh scripts/tlb-stress.sh --runs 50 --out build/tlb-stress-s113
```

## Host preflight

The default path runs `tests/smp/boot/host-idle.sh --context tlb-stress`. It refuses when another
QEMU, a cargo/xtask/rustc job, or a load average above the limit is present. The limit is
host-idle's default `max(2, nproc/4)`; pass `--max-load L` on a shared machine and `--wait S` to
poll for an idle window. **A refused host is not a sample**: the runner exits 2 before the first
qtest.

The runner records `host-idle.txt` and `host-state-before.txt`/`host-state-after.txt` with
`nproc`, the 1-minute load, `/dev/kvm`, both QEMU versions, running `qemu-system-*` processes and
running `xtask qtest` processes. `--no-preflight` skips the refusal but still writes the state;
it is for a smoke, not for the 50/50 close-out.

KVM cases are refused (exit 2) if `/dev/kvm` is unreadable or the QEMU binary does not offer
`kvm`. They are never converted into a TCG pass.

## Output and failure rule

Under `DIR`:

```
host-idle.txt              preflight state/decision
host-state-before.txt      host state before the first sample
host-state-after.txt       host state after the last sample
results.tsv                case arch iteration rc duration_ms json log
summary.txt                case arch runs fail min_ms p50_ms p90_ms max_ms
<case>/iter-N.log          full qtest transcript
<case>/iter-N.json         the qtest JSON (copied before the next run overwrites build/qtest/)
```

The runner stops at the **first** non-zero qtest, keeps that iteration's log and JSON, prints the
case/arch/iteration and the forbidden-hit section (or the log tail), and exits 1. It never retries
and never continues with the remaining cases. Latency stats are nearest-rank over the samples that
actually ran: `p50 = ceil(0.5n)`, `p90 = ceil(0.9n)`, plus min/max. A one-sample failed run prints
that sample and `fail=1`.

## Expected results

* **Pre-S113 (current):** the x86 case is expected to fail with
  `smp: tlb shootdown timeout on cpu N ... last=0 flushes=0` (S118 observed 6 lines on TCG `-smp 4`,
  3 on KVM `-smp 2`, 4 on KVM `-smp 4`). The user data still reports `tlb_stale=0`; the timeout is
  the failure. `--quick` on current master preserves this first failure.
* **Post-S113:** the x86 TCG/KVM 2/4 cases must complete with no timeout; the full
  `--runs 50` matrix is the close-out. The aarch64 case remains gated by S103/S106.
* **KVM unavailable:** the runner refuses the KVM cases (exit 2), never a fake pass.

## Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file + per-file licence | Linux finding | Decision |
|---|---|---|---|---|
| TLB stress pass/fail contract | `hat_tlb_inval_range` uses the HAT CPU set, `xc_call` SYNC, no timeout, and delayed invalidation for idle CPUs (`docs/illumos-smp-study.md` §5; illumos-gate `586678ac`) | NetBSD `sys/arch/x86/x86/x86_tlb.c` rev 1.22 (BSD-2): per-CPU packet with a completion count; `ipi.c` rev 1.30 (BSD-2): reason-coded IPIs. FreeBSD `sys/amd64/amd64/mp_machdep.c` @ `1333444…` (BSD-2): per-initiator scoreboard and `KASSERT` that the initiator has IRQs enabled | Linux `arch/x86/mm/tlb.c:995-1035` and `kernel/smp.c:901-909` @ `adc2186…` (GPL, read-only): synchronous `SCF_WAIT` completion and "must not be called with disabled interrupts" | **Original shell runner; no donor code copied.** The donors agree that a concurrent unmap must complete and a lost ack is a real failure; the runner treats it as such and never retries. |
| Host isolation and statistics | No donor mechanism; the requirement is that a noisy host is not a sample | No BSD harness to port; the existing `tests/smp/boot/host-idle.sh` (S107, BSD-3) is used as-is | Linux CI practice is not a code donor here | Reuse the in-tree preflight; nearest-rank p50/p90 from the observed samples; record the raw transcripts. |

Donor revisions: Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`; NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`; FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`; Linux
`adc218676eef25575469234709c2d87185ca223a`.

## Evidence tags

* OBSERVED: S118 pre-fix serials (6/3/4 timeout lines) and the S106 aarch64 case.
* OBSERVED: `tests/smp/boot/host-idle.sh` options and state fields.
* PROPOSED: the post-S113 50/50 close-out; it is not claimed until S113 merges and the matrix runs.
