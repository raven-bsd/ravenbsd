<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S177 memory-mapping conformance

Application-level POSIX memory-mapping contract rows on the release image. The probe runs as the
unprivileged `raven` account over SSH and prints `mmap: <row> key=value ... ok=<0|1>` lines; the
runner asserts them.

| Row | Contract | Expected |
|---|---|---|
| anon-zero | anonymous private mapping | zero-filled |
| cow-private | MAP_PRIVATE across fork | child does not see the parent's post-fork write |
| cow-shared | MAP_SHARED across fork | child sees the parent's post-fork write |
| mprotect-segv | write through PROT_READ | child dies with SIGSEGV (11) |
| fixed-replace | MAP_FIXED over an occupied range | POSIX replacement; the Raven syscall-level K4 EEXIST is a recorded libc divergence (PG-13) |
| sigbus-eof | fault past EOF of a file-backed mapping | child dies with SIGBUS (7), not SIGSEGV |
| munmap-partial | munmap of the middle page | both edges stay mapped, middle faults SIGSEGV |
| bad-len | mmap length 0 | MAP_FAILED, errno EINVAL (22) |

`mremap` is Partial (PG-13) in the libc registry and the sysroot has no exported `mremap` symbol;
the contract row is recorded as a gap rather than gated. The gate rows are the eight application
rows above.

The negative check is a probe-only mutation: `--mutation=private-shared` maps the private row as
shared, the child sees the parent's `0x11`, and the runner requires the row to turn red. The
product tree is not changed.

## Build and run

```text
sh tests/conformance/memory-map/build.sh
python3 tests/conformance/memory-map/run.py --image build/conformance/memory-map/memory-map.img
python3 tests/conformance/memory-map/run.py --image build/conformance/memory-map/memory-map.img --iterations 100
python3 tests/conformance/memory-map/run.py --image build/conformance/memory-map/memory-map.img --mutation
```

`build.sh` uses the release initfs and stages only `/bin/mmap-probe`; no product file changes.

### Kernel heap churn (S198)

`--iterations N` runs every row N times inside one probe process, so repeated map/unmap and
fork/exit cycles exercise the kernel heap. The image command line also enables `k449-metrics=1`;
the runner records the pre-loop and post-loop `K449-GRAPH`/`K449-METRICS` samples. The 1440-byte
allocation in the S195 panic is served from the kernel 1536-byte size class, so the gate requires
`class1536_in_use` (plus `heap_used`, `procs` and `threads`) to return to the pre-loop baseline
within a small tolerance for `--iterations` of 10 or more. A leaking per-mapping or per-child
object grows the class until its 512-object slab table is exhausted and the next 1440-byte
allocation panics.

## Donor map

| Topic | Donor |
|---|---|
| mmap/mprotect/munmap/mremap semantics, MAP_FIXED collision | POSIX `mmap`, FreeBSD `sys/vm/vm_mmap.c` (`vm_mmap`), NetBSD `sys/uvm/uvm_mmap.c` |
| fault, SIGSEGV/SIGBUS distinction, COW | FreeBSD `sys/vm/vm_fault.c` (`vm_fault`), NetBSD `sys/uvm/uvm_fault.c` |
| MAP_FIXED occupied errno | Raven `docs/mm-contract.md` K4 (EEXIST), NetBSD `uvm_mmap.c` |
| mremap (Partial, PG-13) | musl `src/mman/mremap.c` via the libc registry; private anonymous growth only |

The test is original C; no donor code is copied.
