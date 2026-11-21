<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Reproducible load-matched TLB stress gate (K376)

Status: **test harness for S159**. It drives the existing `posix-fs-smp` TCG `-smp 4` case under a
deterministic bounded CPU load, records every iteration, and runs the S113 TLB reproducer 10/10
separately. It does not edit the kernel, the case, or any fix path.

## Why a load-matched gate

S159's only failing evidence was 20/20 `posix-fs-smp` TCG4 iterations under host load 88-126, with
two real `smp: tlb shootdown timeout` lines (`FSTAT` and `EXIT`) and the same case green at low
load. A report table is not a gate anyone can rerun. This harness fixes the pressure class, the
iteration count, the per-iteration cap and the serial capture.

## Invocation

```sh
# 20 load-matched iterations (default; load 100 workers)
scripts/tlb-stress.sh --load 100 --iterations 20 --out build/tlb-stress/load20

# low-load control: no added workers; refuses if the shared host is still loaded
scripts/tlb-stress.sh --load 0 --iterations 3 --out build/tlb-stress/control

# batches of five when 20 iterations do not fit one cap
scripts/tlb-stress.sh --load 100 --iterations 5 --out build/tlb-stress/batch-N
```

Options: `--load N`, `--iterations N`, `--out DIR`, `--case NAME` (default `posix-fs-smp`),
`--no-prebuild`, `--s113`/`--no-s113`, `--max-host-load L` (low-load guard, default 20).

The load generator is `tests/smp/tlb/load-generator.sh`: `--load N` starts exactly N `yes(1)`
workers with one fixed work unit each, so the pressure class is reproducible; the measured
`nproc` and `/proc/loadavg` 1/5/15 are recorded, not the requested count. The workers are stopped
on exit, including on signal.

## Output

Under `DIR`:

```text
host-start.txt          nproc/load/commit/kvm/qemu
host-load-start.txt     after workers start
results.tsv             iteration, start/end, load 1/5/15 at both ends, rc, result, duration, log
serial-lines.txt        every tlb/timeout/stale/FAIL/pass line, per iteration
iter-N.log              full serial for that iteration
iter-N.json             qtest JSON
iter-N.stdout           foreground qtest output
s113.log                separate S113 10/10 run
prebuild*.log           image build and one prebuild sample
```

A row is `PASS` only when the qtest exits 0. A case timeout is `FAIL`, keeps its log and is never
retried. `tlb shootdown timeout`, `tlb_stale`, worker failure and `init exited (1)` are all read
from `iter-N.log`; the harness does not filter them into a separate result.

## Measured runtime

* Per `posix-fs-smp` iteration under the observed load class: **309-323 s** in the K376 batch
  (`build/tlb-stress/k376-load5`), so a five-iteration batch needs a **1800 s** cap and a
  20-iteration run must be split into four recorded batches.
* S113 10/10 under the same shared load: worst **21.3 s**, average 10.9 s; total ~110 s.
* Low-load guard: if `--load 0` sees host load1 above 20, it exits 2 with zero samples. A loaded
  shared host is not a control.

## How to read a PASS/FAIL

* `results.tsv` `rc=0 result=PASS` for every row and `s113 rc=0` -> gate green.
* Any `result=FAIL` / `s113 rc!=0` -> gate red; open the row's `iter-N.log`, find the first
  `tlb|timeout|stale|FAIL|pass` line in `serial-lines.txt`, and hand it to the S159 owner.
* `--load 0` refusing with `BLOCKED low-load control` means the host was too loaded for a control;
  it is not a pass and not a product failure.

## Current K376 evidence

* Harness built and syntax-checked; prebuild of `posix-fs-smp` on `978899da`.
* Load batch `--load 20 --iterations 5` under host load 102-159: **5/5 FAIL** (309-323 s each;
  iteration 2 worker timeout, the others case timeout). This proves the harness measures the
  load-class failure.
* Low-load control: **BLOCKED** on the shared host (`load1=102 > 20`, zero samples after a
  900 s idle wait; load never fell below 31).
* S113 `tests/smp/memory/run.sh tcg --iterations 10`: **10/10 PASS**, `tlb_stale=0`, worst
  21.3 s, separate from the SMP gate.
* Full 20-row load run was not taken because the shared host could not provide a low-load control
  and the measured 5-row batch is already a FAIL result; the harness is batch-ready.

## Donor reconciliation

| Question | Answer |
|---|---|
| How does Illumos do it? | n/a — harness |
| Does BSD have a permissive implementation to take? | n/a |
| How did Linux do it? | n/a |
| Chosen approach | first-party shell harness around the existing qtest; no donor code |

The gate treats a missing TLB ACK/timeout as a real failure, matching the donor consensus
(per-CPU completion; a lost ack is not retried away), but no donor code is copied.

## Gaps

* The low-load control cannot be claimed on this host; it is BLOCKED until a lane gets an idle
  window (load1 <= 20).
* Only the first load batch of five rows was measured; the remaining three batches are the same
  command and are required for the full S159 20-row reproduction.
* The load generator controls CPU pressure, not memory/IO pressure; the S159 evidence is CPU
  starvation on a 20-CPU host, so this is the matching class.
