<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Syscall robustness (K456)

Status: **OBSERVED on base `a666aab4`, x86-64, TCG `smp=4` and KVM `smp=2`.** The guest fixture
`tests/smp/syscall-fuzz` walks every `raven-abi` syscall variant with deterministic garbage vectors
and prints the raw return, errno and survival for each call. It fixes nothing: the deviations below
are findings for the owning lane.

## Scope and count

`crates/raven-abi/src/syscall.rs` defines **47** `Syscall` variants (`Syscall::ALL.len() == 47`,
printed by the fixture). The ticket text says 48; the measured table has 47 rows. Raw numbers that
are dispatched before the enum (`PROC_FORK`, `PIPE`, `TRACE_SET`, `RECV_LOCK`, `MSYNC`,
`PROC_REBOOT`, the Linux trap, ...) are outside this table; they are exercised only where the
fixture uses them as setup (`PIPE`, `PROC_FORK`) or where the enum names the same operation.

## Method

* Seed: `0x5a17c0de12349876` (`SEED` in `tests/smp/syscall-fuzz/src/main.rs`).
* Vector classes: invalid handle (`fdneg` `-1`, `fdhuge` `0x7fffffff`, `fdclosed`), invalid pointer
  (`ptr0`, `ptr1`, `ptrneg`), a valid buffer, a buffer straddling a page boundary into a
  `PROT_NONE` guard page, lengths (`0`, `1`, `SIZE_MAX`, `SIZE_MAX-7`, a length larger than the
  mapping), flags/opcode (`0`, `-1`, a fixed seeded 64-bit value), and a `PROT_NONE` buffer.
* Each vector poisons exactly one argument; every other argument is valid, so the errno is
  attributable.
* A valid baseline is measured per call. Where the initfs has no server or device, the baseline is
  recorded as context (`any`) and the coverage gap is listed below.
* `EXIT`, `THREAD_EXIT` and `PROC_TERMINATE(self)` run in forked children; the parent records the
  exit code and its own survival.
* Every case line is `syscall-fuzz: case nr=.. name=.. vector=.. ret=.. errno=.. ok=..`; the summary
  prints the stable FNV-1a `table-hash` over `(number, vector, return)`.

## Commands and result (OBSERVED)

```text
$ timeout 120 sh tests/smp/syscall-fuzz/run.sh tcg --iterations 1
rows=240 deviations=9 seed=0x5a17c0de12349876 table-hash=0xb6b4b85a00ea5d77
real 26.42 s (image rebuild included)

$ timeout 120 sh tests/smp/syscall-fuzz/run.sh kvm2 --iterations 1
rows=240 deviations=9 seed=0x5a17c0de12349876 table-hash=0xb6b4b85a00ea5d77
real 13.50 s
```

One iteration is a determinism pair: two boots of the same image. Both boots in each mode printed the
same hash, and TCG and KVM produced the same hash. `syscall-fuzz: alive` and `init exited (0)` were
present in all four boots. Full logs: `build/syscall-fuzz/<mode>/run-1/a/syscall-fuzz*.log`.

## Measured table

`ok` is a non-negative return; every other cell is the errno. `2 boots` are identical.

| Syscall | Vectors -> observed |
|---|---|
| YIELD | noargs=ok |
| NANOSLEEP | zero=ok |
| RECV | cap0=EINVAL noserver=EINVAL |
| CLOCK_GET | baseline=ok clockneg=EINVAL ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT protnone=EFAULT |
| UNAME | baseline=ok ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT protnone=EFAULT |
| DEBUG_WRITE | baseline=ok len0=ok ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT **straddle=ok** **protnone=ok** lenbig=EINVAL lenmax=EINVAL |
| DEBUG_READ | baseline=ok len0=ok ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT straddle=ok protnone=ok lenmax=EFAULT |
| FUTEX | wake=ok wait-other=EAGAIN ptr0=EFAULT ptr1=EINVAL ptrneg=EFAULT wake-protnone=ok **wait-protnone=EAGAIN** opneg=EINVAL oprand=EINVAL |
| OPEN | ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT **protnone=EINVAL** len0=EINVAL lenmax=ENAMETOOLONG noscheme=EINVAL flagsneg=EINVAL baseline=ENOENT |
| SCHEME_REGISTER | ptr0=EFAULT ptr1=EFAULT **protnone=EINVAL** len0=EINVAL lenmax=ENAMETOOLONG fdneg=EBADF fdnotproc=EACCES baseline=ok |
| THREAD_CREATE | baseline=ok fdneg=EBADF fdhuge=EBADF ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT stack0=EFAULT stack1=EFAULT flagsneg=EINVAL flags2=EINVAL exnull=EINVAL |
| PROC_CREATE | baseline=ok flagsneg=EINVAL flagsrand=EINVAL |
| PROC_TERMINATE | fdneg=EBADF fdhuge=EBADF created=ok terminal=ESRCH childself9=ok |
| PROC_NAMESPACE | fdneg=EBADF opneg=EINVAL ptr0=EFAULT ptr1=EFAULT **protnone=EINVAL** len0=EINVAL lenmax=ENAMETOOLONG add=ok clear=ok |
| PROTECT | baseline=ok protneg=EINVAL protflag=EINVAL fdneg=EBADF addr1=EINVAL len0=EINVAL lenmax=EINVAL |
| UNMAP | baseline=ok fdneg=EBADF addr1=EINVAL len0=EINVAL lenmax=EINVAL |
| MAP | baseline=ok fdneg=EBADF memobjhuge=EBADF len0=EINVAL lenbig=EINVAL lenmax=EINVAL flagsneg=EINVAL addr1=EINVAL offset1=EINVAL |
| MEMOBJ_CREATE | baseline=ok len0=EINVAL lenbig=EINVAL lenmax=EINVAL flagsneg=EINVAL |
| PAGER_SUPPLY | baseline=ok fdneg=EBADF notpager=EPERM offset1=EINVAL offsetpast=EINVAL len0=EINVAL lenbig=EINVAL ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT **protnone=ok** |
| HANDLE_CLOSE | baseline=ok fdneg=EBADF fdhuge=EBADF fdclosed=EBADF |
| HANDLE_DUP | baseline=ok fdneg=EBADF fdhuge=EBADF fdclosed=EBADF mask0=ok |
| HANDLE_TRANSFER | baseline=ok fdneg=EBADF destnotproc=EINVAL srcneg=EBADF mask0=EINVAL |
| HANDLE_REVOKE | baseline=ok fdneg=EBADF fdclosed=EBADF |
| EVQ_CREATE | baseline=ok flagsneg=EINVAL flags1=EINVAL |
| EVQ_WAIT | empty=ok fdneg=EBADF ptr0=EFAULT ptr1=EFAULT max0=ok maxmax=EINVAL ready=ok |
| EVQ_CTL | add=ok fdneg=EBADF opneg=EINVAL srcneg=EBADF eventsneg=EINVAL |
| EVENT_SIGNAL | baseline=ok fdneg=EBADF fdclosed=EBADF valuemax=ok |
| TIMER_CREATE | baseline=ok clockbad=EINVAL flagsneg=EINVAL |
| EVENT_CREATE | baseline=ok flagsneg=EINVAL |
| TIMER_SET | disarm=ok fdneg=EBADF fdclosed=EBADF |
| READ | baseline=ok fdneg=EBADF fdhuge=EBADF fdclosed=EBADF fdwrongdir=EBADF ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT straddle=EFAULT protnone=EFAULT len0=ok lenmax=EFAULT lenmax7=EFAULT |
| WRITE | baseline=ok fdneg=EBADF fdhuge=EBADF fdclosed=EBADF fdwrongdir=EBADF ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT **straddle=ok** **protnone=ok** len0=ok lenmax=EFAULT |
| CLOSE | baseline=ok fdneg=EBADF fdclosed=EBADF |
| DUP | baseline=EBADF fdneg=EBADF fdclosed=EBADF |
| FSTAT | fdneg=EBADF pipe=EBADF |
| CALL | fdneg=EBADF |
| SEND | fdneg=EBADF |
| MMIO_CREATE | baseline=ok fdneg=EBADF len0=EINVAL lenbig=EINVAL attrneg=EINVAL |
| BOOT_BLOB | baseline=ok fdneg=EBADF kind0=EINVAL kindmax=EINVAL |
| DMA_ALLOC | baseline=ok fdneg=EBADF len0=EINVAL lenbig=EINVAL alignbad=EINVAL masklow=EINVAL flagsneg=EINVAL |
| DMA_INFO | fdneg=EBADF ptr0=EFAULT baseline=ok |
| IRQ_CREATE | baseline=ok fdneg=EBADF kind0=EINVAL kindmax=EINVAL numberbig=EINVAL flags1=EINVAL |
| IRQ_WAIT | timeout=ok fdneg=EBADF |
| IRQ_ACK | baseline=ok fdneg=EBADF |
| PROC_WAIT | fdneg=EBADF fdhuge=EBADF self=EINVAL ptr0=EFAULT ptr1=EFAULT child-code=ok |
| EXIT | child7=ok childmax=ok |
| THREAD_EXIT | child7=ok |

## Deviations

Nine rows returned something other than the contract errno. All nine have one root cause: on
x86-64, `copy_from_user` reads a `PROT_NONE` leaf instead of faulting.

| Syscall | Vector | Expected | Actual | Kernel path (OBSERVED) |
|---|---|---|---|---|
| DEBUG_WRITE | straddle (8 B valid + 8 B `PROT_NONE`) | EFAULT | 16 (success) | `kernel/src/syscall/basic.rs:60` `debug_write` -> `:71` `copy_from_user` |
| DEBUG_WRITE | protnone (1 B) | EFAULT | 1 (success) | same |
| WRITE | straddle | EFAULT | 16 (success) | `kernel/src/syscall/ipc.rs:145` `write` -> `kernel/src/process/pipe.rs:351` `copy_in` |
| WRITE | protnone | EFAULT | 1 (success) | same |
| OPEN | protnone (7 B path) | EFAULT | EINVAL (reads NUL bytes, then no scheme) | `kernel/src/syscall/ipc.rs:73` `open` -> `kernel/src/ipc/router.rs:1230` `copy_from_user` |
| SCHEME_REGISTER | protnone (4 B name) | EFAULT | EINVAL (reads NUL bytes) | `kernel/src/syscall/ipc.rs:48` `register` -> `router.rs:1230` |
| PROC_NAMESPACE | protnone (4 B prefix) | EFAULT | EINVAL (reads NUL bytes) | `kernel/src/syscall/process_ctl.rs:946` `user_string` -> `router.rs:1230` |
| PAGER_SUPPLY | protnone (1 B data) | EFAULT | 0 (success; the zero page is supplied as data) | `kernel/src/syscall/memory_ctl.rs:128` `copy_from_user` |
| FUTEX | wait-protnone | EFAULT | EAGAIN (reads the zero word, values differ) | `kernel/src/syscall/futex.rs:104` `copy_from_user` |

### Root cause

* `kernel/src/memory/uspace.rs:1508` `none_leaf_flags()` encodes `PROT_NONE` as
  `PRESENT | NO_EXECUTE`: a present, supervisor-only, **read-only** leaf. The comment says clearing
  the user bit makes ring-3 access fault, and it does.
* `kernel/src/usercopy.rs:42-54` implements x86-64 `copy_from_user` with `rep movsb` under a fault
  fixup. At CPL 0 an absent user bit does not stop a read, so the kernel reads the protected page.
  Writes do fault because the leaf is read-only: `copy_to_user` into the same page correctly returns
  EFAULT (see `UNAME`, `CLOCK_GET`, `EVQ_WAIT`, `READ` in the table).
* `crates/raven-usercopy/src/lib.rs:93-106` `validate_range` checks the numeric address range and
  the `isize::MAX` bound only; it cannot see the mapping protection.
* aarch64 does not have the defect: `kernel/src/usercopy.rs:92-110` uses unprivileged `ldtrb`/
  `sttrb`, which enforce EL0 permissions (`PROT_NONE` faults regardless of PAN).

### Proposed follow-up (PROPOSED, not fixed here)

One kernel ticket, owner stuart: make x86-64 `copy_from_user` reject a user range whose leaf lacks
the user bit (walk the leaf, or set AC for SMAP only after the leaf check), with a unit test that a
`PROT_NONE` source returns EFAULT for `DEBUG_WRITE`/pipe `WRITE`/`user_string`/`PAGER_SUPPLY`/
`FUTEX`; keep the aarch64 `ldtrb` path as the reference. The ticket that fixes it must also keep the
write-side EFAULT behavior covered.

## Coverage gaps (OBSERVED)

* `DEBUG_READ` copies only after console input arrives; the fixture injects none, so the
  straddle/`PROT_NONE` **destination** copy is not reached. A future harness can feed the serial
  console.
* `FSTAT`, `CALL`, `SEND` and `DUP` need a scheme file/server; the initfs has no vfsd, so only the
  invalid-handle rows are measured for `FSTAT`/`CALL`/`SEND`, and `DUP`'s baseline is `EBADF`.
* `RECV` with a full packet buffer cannot be measured without a registered server (it would block);
  only the capacity-error and no-server rows are measured.
* Device baselines (`MMIO_CREATE`, `BOOT_BLOB`, `DMA_ALLOC`, `IRQ_CREATE`) are recorded as context;
  their success depends on the QEMU profile. They are not part of the deviation list.
* Raw dispatched numbers outside `Syscall::ALL` (fork/exec/pipe/signal/pread/trace/reboot/...) are
  not in this table; `PROC_FORK` and `PIPE` are used only as setup.

## K451 cross-reference

`docs/kernel/audit/` is not present on base `a666aab4` (K451 not merged). This file is the measured
input that the K451 panic-surface section should cite by row; `docs/kernel/audit/**` was not edited
here (Stuart owns it).
