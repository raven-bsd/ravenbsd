<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Syscall robustness (K456, K462 round 2)

Status: **OBSERVED on base `abd594b8`, x86-64, TCG `smp=4` and KVM `smp=2`.** The guest fixture
`tests/smp/syscall-fuzz` walks every `raven-abi` syscall variant **and** every raw pre-enum dispatch
site with deterministic garbage vectors, and prints the raw return, errno and survival for each
call. It fixes nothing: the deviations below are findings for the owning lane (K461 owns the one
root cause).

## Scope and count

* `crates/raven-abi/src/syscall.rs` defines **47** `Syscall` variants (`Syscall::ALL.len() == 47`,
  printed by the fixture). The ticket text says 48; the measured enum table has 47 rows.
* Round 2 adds the **65 pre-enum dispatch sites** in `kernel/src/syscall/mod.rs` before
  `match Syscall::from_usize(number)`: 58 direct `if number == ..` checks, six DTrace control
  numbers, and the Linux-personality trap. `tests/smp/syscall-fuzz/raw-dispatch.list` is the
  checked list (expression, number, definition file:line, disposition); `raw-check.sh` parses the
  kernel source and fails on any drift, missing fixture reference, or a fuzzed excluded path. The
  current check prints `entries=65 fuzz=63 child=1 excluded=1`.
* Total measured rows: **432** (240 enum vectors + 192 raw vector rows; `raw-audit fuzzed=184`
  counts raw calls, including the multi-vector sites).

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
* `EXIT`, `THREAD_EXIT`, `PROC_TERMINATE(self)`, `PROC_SETSID`, `THREAD_EXIT_EX` and the
  Linux-personality trap run in forked children; the parent records the exit code and its survival.
* Every case line is `syscall-fuzz: case nr=.. name=.. vector=.. ret=.. errno=.. ok=..`; the summary
  prints the stable FNV-1a `table-hash` over `(number, vector, return)`.

## Commands and result (OBSERVED)

```text
$ timeout 300 sh tests/smp/syscall-fuzz/run.sh tcg --iterations 1
rows=432 deviations=10 seed=0x5a17c0de12349876 table-hash=0xef23e662891758f3
real 13.79 s

$ timeout 300 sh tests/smp/syscall-fuzz/run.sh kvm2 --iterations 1
rows=432 deviations=10 seed=0x5a17c0de12349876 table-hash=0xd8b6c53a23b7a8db
real 10.86 s
```

One iteration is a determinism pair: two boots of the same image. Both boots in each mode printed the
same hash. TCG and KVM now differ at exactly two rows, both mode-dependent by design:
`THREAD_SELF` returns the current tid (5 vs 3) and `DTRACE_STATS` returns the online CPU count
(4 vs 2). The K456 hash was `0xb6b4b85a00ea5d77` for the 240 enum rows; the new hash covers the 192
raw rows as well, so it necessarily changes. `syscall-fuzz: alive` and `init exited (0)` were present
in all four boots. Full logs: `build/syscall-fuzz/<mode>/run-1/a/syscall-fuzz*.log`.

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

## Raw pre-enum table (round 2)

Order and names are the checked list; `ok` is a non-negative return. `exit(-5)` is a child exit
code, not an errno.

| Site | Vectors -> observed |
|---|---|
| GETRANDOM | baseline=ok len0=ok ptr0=EFAULT protnone=EFAULT flagsneg=EINVAL lenmax=EINVAL |
| CLOCK_SET | clockbad=EINVAL ptr0=EFAULT negts=EINVAL |
| TRACE_SET | zero=ok max=ok |
| RECORD_LOCK | cmdneg=EINVAL cmd0=EINVAL ptr0=EFAULT handlebad=EBADF pipe=EBADF |
| PROC_CHROOT | ptr0=EFAULT ptr1=EFAULT len0=EINVAL lenmax=ENAMETOOLONG |
| THREAD_SET_TLS | fdneg=EBADF zero=ok tp1=EFAULT tpmax=EFAULT |
| THREAD_SELF | noargs=ok (mode-dependent tid) |
| ROBUST_LIST_SET | clear=ok head1=EINVAL lenmax=EINVAL |
| PROC_SET_PERSONALITY | persbad=EINVAL handlerbad=EBADF baseline=ok |
| PROC_SELF_INFO | baseline=ok ptr0=EFAULT count0=EINVAL |
| PROC_SETPGID | pidneg=ESRCH pgidneg=EPERM baseline=EPERM (caller is a session leader) |
| PROC_SETSID | child=ok (child is not a group leader) |
| PROC_SETCRED | whichneg=EINVAL valuechange=EPERM baseline=ok |
| PROC_FORK | fdneg=EBADF flags1=EINVAL |
| PROC_PID_INFO | baseline=ok ptr0=EFAULT count0=EINVAL pidneg=ESRCH |
| PROC_SETCRED_EX | which0=EINVAL groupsptr0=EFAULT baseline=ok |
| PROC_CRED_INFO | baseline=ok ptr0=EFAULT count5=EINVAL |
| PROC_GETGROUPS | size0=ok ptr0=EFAULT size1=ok |
| PROC_HANDLE_INFO | baseline=ok fdneg=EBADF |
| PROC_EXEC_CRED | flags2=EINVAL self=EPERM filebad=EBADF |
| MSYNC | fdneg=EBADF flags0=EINVAL flagsneg=EINVAL addr1=EINVAL len0=EINVAL lenmax=EINVAL |
| PROC_SET_COMMAND | baseline=ok ptr0=EFAULT len0=ok lenmax=ENAMETOOLONG fdneg=EBADF |
| PROC_WAIT_EX | optneg=EINVAL ptr0=EFAULT idtypebad=EINVAL baseline=ok |
| PROC_GETPGID | self=ok pidneg=ESRCH |
| PROC_SIGNAL | signo0=EINVAL targethuge=ESRCH |
| PROC_SIGRECV | ptr0=EFAULT flagsneg=EINVAL nohang=EAGAIN (pending SIGCHLD drained first) |
| PROC_TIMES | baseline=ok ptr0=EFAULT count3=EINVAL |
| PROC_GETRUSAGE | baseline=ok who1=EINVAL ptr0=EFAULT count7=EINVAL |
| PROC_GETRLIMIT | baseline=ok res16=EINVAL ptr0=EFAULT |
| PROC_SETRLIMIT | res16=EINVAL invert=EINVAL |
| HANDLE_COPY | baseline=ok fdneg=EBADF mask0=EINVAL |
| HANDLE_TRANSFER_FILE | mask0=EINVAL filebad=EBADF pipe=EBADF |
| PROC_EXEC | ptr0=EFAULT ptr1=EFAULT ptrneg=EFAULT **protnone=EINVAL** image0=EINVAL flags1=EINVAL pathmax=ENAMETOOLONG imagmax=EFBIG |
| PREAD | fdneg=EBADF pipe=EBADF |
| PWRITE | fdneg=EBADF pipe=EBADF |
| SEEK | fdneg=EBADF pipe=EBADF |
| PIPE | baseline=ok ptr0=EFAULT ptr1=EFAULT protnone=EFAULT |
| PIPE2 | baseline=ok ptr0=EFAULT flagsneg=EINVAL |
| GETPRIORITY | baseline=ok ptr0=EFAULT whichneg=EINVAL whohuge=ESRCH |
| SETPRIORITY | baseline=ok whichneg=EINVAL whohuge=ESRCH |
| NICE | baseline=ok ptr0=EFAULT |
| SCHED_GET_PRIORITY_MAX | other=ok fifo=EOPNOTSUPP policyneg=EINVAL |
| SCHED_GET_PRIORITY_MIN | other=ok fifo=EOPNOTSUPP policyneg=EINVAL |
| SCHED_SETSCHEDULER | baseline=ok policyfifo=EOPNOTSUPP policyneg=EINVAL priobad=EINVAL pidhuge=ESRCH |
| SCHED_GETSCHEDULER | self=ok pidneg=ESRCH |
| PROC_CTTY_SET | fdneg=EBADF pipe=ENOTTY |
| PROC_CTTY_GET | fdneg=EBADF pipe=ENOTTY |
| PROC_CTTY_SET_FG | fdneg=EBADF pipe=ENOTTY |
| SIG_SETUP | ptr0=EFAULT offset1=EINVAL flagsneg=EINVAL baseline=ok |
| THREAD_KICK | fdneg=EBADF selfzero=ok |
| SIGRESEND | noregistration=EINVAL |
| DTRACE_PROBE_LIST | count=ok ptr0=EFAULT maxmax=ok (clamped) |
| DTRACE_ENABLE | flags1=EINVAL handlebad=EBADF |
| DTRACE_DISABLE | probe0=EINVAL (no probe 0) probemax=EINVAL |
| DTRACE_READ | empty=ok ptr0=EFAULT maxmax=EINVAL |
| DTRACE_STATS | count=ok (mode-dependent CPU count) ptr0=EFAULT |
| DTRACE_CLEAR | noargs=ok |
| DUPLEX_PAIR | baseline=ok ptr0=EFAULT ptr1=EFAULT protnone=EFAULT flagsneg=EINVAL |
| DUPLEX_SHUTDOWN | baseline=ok howbad=EINVAL fdneg=EBADF |
| DUPLEX_SENDMSG | baseline=ok fdneg=EBADF msgptr0=EFAULT flagsbad=EINVAL databad=EFAULT |
| DUPLEX_RECVMSG | baseline=ok fdneg=EBADF msgptr0=EFAULT |
| DUPLEX_PEERCRED | baseline=ok ptr0=EFAULT |
| THREAD_EXIT_EX | child7=ok garbage=ok (child exit 9) |
| LINUX_TRAP | write=exit(-5) read=exit(-5) exit=exit(-5) (no handler) |

## Excluded paths

| Path | Number | Reason | Proof |
|---|---|---|---|
| PROC_REBOOT | 0x46 | Exits the machine (poweroff, halt or reset). A successful call is not survivable, so no bounded non-destructive variant exists. | `process_ctl::reboot` calls `crate::arch::poweroff/halt/reset`; `raw-check.sh` verifies the handler still has a machine-ending path and that the fixture has **no** `RAW_PROC_REBOOT` call and lists the name in `RAW_EXCLUDED`. |

No other whole path is excluded. The baseline of `PROC_EXEC` (a successful image replacement),
`CLOCK_SET` (a real time change) and `PROC_SETCRED_EX` (a real credential change) are not run; the
garbage-argument rows for those calls are measured.

## Deviations

Ten rows returned something other than the contract errno. All ten have one root cause: on
x86-64, `copy_from_user` reads a `PROT_NONE` leaf instead of faulting. Round 2 adds the `PROC_EXEC`
request path; `PROC_EXEC` with `ptr0`/`ptr1`/`ptrneg` correctly returns EFAULT because the range
validator rejects those before the copy.

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
| PROC_EXEC | protnone (88 B request) | EFAULT | EINVAL (reads a zero request, `image_len=0`) | `kernel/src/syscall/exec.rs:57` `Request::parse` -> `copy_from_user` |

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

K461 (Mark) owns the root cause: make x86-64 `copy_from_user` reject a user range whose leaf lacks
the user bit (walk the leaf, or set AC for SMAP only after the leaf check), with a unit test that a
`PROT_NONE` source returns EFAULT for `DEBUG_WRITE`, pipe `WRITE`, `user_string`, `PAGER_SUPPLY`,
`FUTEX` and the `PROC_EXEC` request; keep the aarch64 `ldtrb` path as the reference. The fix must
also keep the write-side EFAULT behavior covered.

## Coverage gaps (OBSERVED)

* `DEBUG_READ` copies only after console input arrives; the fixture injects none, so the
  straddle/`PROT_NONE` **destination** copy is not reached. A future harness can feed the serial
  console.
* `FSTAT`, `CALL`, `SEND` and `DUP` need a scheme file/server; the initfs has no vfsd, so only the
  invalid-handle rows are measured for `FSTAT`/`CALL`/`SEND`, and `DUP`'s baseline is `EBADF`.
  `PREAD`/`PWRITE`/`SEEK`/`HANDLE_TRANSFER_FILE` and the TTY controls have the same setup limit.
* `RECV` with a full packet buffer cannot be measured without a registered server (it would block);
  only the capacity-error and no-server rows are measured.
* The Linux-personality trap is measured **without a handler**, which is the documented `-EIO`
  termination path (child exit `-5`); a handler-backed trap needs a userspace Linux server and is
  not covered.
* `PROC_EXEC` is measured with invalid requests only; a successful exec replaces the image. The
  `CLOCK_SET` and `PROC_SETCRED_EX` successful-effect baselines are likewise not run.
* `PROC_SIGRECV` drains the pending `SIGCHLD` from the earlier child tests before its WNOHANG
  vector, so that vector deterministically measures the no-pending path; the drained signal number
  is not a table row.
* Device baselines (`MMIO_CREATE`, `BOOT_BLOB`, `DMA_ALLOC`, `IRQ_CREATE`) are recorded as context;
  their success depends on the QEMU profile. They are not part of the deviation list.
* Cross-mode hash equality is not claimed: `THREAD_SELF` (tid) and `DTRACE_STATS` (online CPUs)
  legitimately differ between TCG `smp=4` and KVM `smp=2`. The determinism pair is per mode.

## K451 cross-reference

`docs/kernel/audit/` is not present on base `abd594b8` (K451 not merged). This file is the measured
input that the K451 panic-surface section should cite by row; `docs/kernel/audit/**` was not edited
here (Stuart owns it).
