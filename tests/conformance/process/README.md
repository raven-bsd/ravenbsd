<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# Process and exec conformance (S181)

`process-conformance.rs` asserts the POSIX/FreeBSD process contract on the release image. The
shared runner in `../harness/runner.py` builds the probe, overlays it as a one-shot service,
boots the image and maps the rows to an exit status. It is test-only: no product behaviour is
changed here, and a red row is a fix ticket for the owning lane (S168/S169).

## Rows

| Row | Contract asserted |
|---|---|
| `wait-echild` | `waitpid(-1, ..., WNOHANG)` with no children is `-1`/`ECHILD` |
| `exit-status` | `_exit(0)`, `_exit(1)` and `_exit(42)` decode through `WIFEXITED`/`WEXITSTATUS` |
| `external-status` | `/bin/brush -c 'exit 134'` reports the exact status 134 the SSH gate quotes |
| `signal-status` | a `SIGKILL` child reports `WIFSIGNALED`/`WTERMSIG`, never a normal exit |
| `wait-nohang` | `WNOHANG` returns 0 while a live child has no event; the blocking wait then returns 42 |
| `zombie-until-waited` | the status is retained until the parent waits, then a second wait is `ECHILD` and `kill(pid, 0)` is `ESRCH` |
| `fork-signal-inheritance` | a forked child inherits the caught handler and the blocked mask |
| `exec-sigreset` | `exec` resets caught handlers to `SIG_DFL` and preserves the blocked mask |
| `fd-inheritance` | `fork` copies plain and `O_CLOEXEC` descriptors; `exec` closes the `O_CLOEXEC` one and keeps the plain one |
| `exec-start-layout` | `argv` and `envp` reach the image byte-exactly, `AT_EXECFN` names the image, the entry stack is 16-byte aligned and `argv[argc]`/`envp` are terminated |
| `at-random-policy` | `AT_RANDOM` is present and, on the seeded release image, nonzero and 16 readable nonzero bytes |
| `reparent-reap` | an orphan is reparented to init (ppid 1), is no longer the probe's child, and is reaped |

## Controlled mutations

* `signal-as-exit` -- the probe expects a `SIGKILL` child to report a normal exit. The correct
  product reports the signal, so `signal-status` turns red.
* `reparent-stays-child` -- the probe expects the orphan to keep its dead parent. The correct
  product reparents it to init, so `reparent-reap` turns red.

Both run in the same boot after the normal rows and are delimited by `CONFORM-MUTATION` lines.

## Donor map

| Topic | Illumos finding | BSD file + licence | Linux finding | Decision |
|---|---|---|---|---|
| exit status, wait, zombie, reparent | `proc_exit`/`waitid` state machine: a zombie stays on the reaplist until `waitid`; children reparent to init | FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` `sys/kern/kern_exit.c` `exit1`, `proc_reap`, `proc_reparent`, `proc_realparent`; `sys/sys/wait.h` `W_EXITCODE`/`WIFEXITED`/`WEXITSTATUS` (BSD-2-Clause) | `kernel/exit.c` wait outcome only | test only: exact status bytes, `WNOHANG`, `ECHILD`, signal status, reparent |
| start layout, argv/envp/auxv | `exec_common`, `stk_copyout`; no `AT_RANDOM` on Illumos | FreeBSD `sys/kern/kern_exec.c` `exec_copyout_strings`, `exec_args_adjust_args`; `sys/kern/imgact_elf.c` auxargs (BSD-2-Clause) | glibc `sysdeps/unix/sysv/linux/dl-parse_auxv.h` and `elf/elf.h` (LGPL, read-only) plus the Linux `AT_RANDOM` stack contract | test only: exact bytes, alignment, `AT_EXECFN`, `AT_RANDOM` |
| handler reset, mask across fork/exec | `exec_common` resets dispositions, keeps the mask | FreeBSD `sys/kern/kern_exec.c` `execsigs`; `sys/kern/kern_fork.c` `fork1` sigacts copy (BSD-2-Clause) | `fs/exec.c` outcome only | test only |
| descriptor inheritance, `O_CLOEXEC` | file objects are shared by `fhold` | FreeBSD `sys/kern/kern_descrip.c` `fdcopy`, `fdcloseexec`; `sys/kern/kern_exec.c` exec cleanup (BSD-2-Clause) | `fs/file.c` read-only | test only: fork copies both, exec closes the `O_CLOEXEC` one |
| external command status | shell status plumbing only | FreeBSD `sys/kern/kern_exit.c` `W_EXITCODE` encoding (BSD-2-Clause); brush (MIT) returns the command status | outcome only | test only: `/bin/brush -c 'exit 134'` |

No donor expression is copied; the probe asserts observable bytes, status words and errno values.
`AT_RANDOM` is not a FreeBSD native auxv key; the Raven policy is the local libc start contract
(`libc/ravenlibc/crates/raven-libc/src/start.rs`, `kernel/src/process/mod.rs` `build_stack`) with
the Linux/glibc consumer contract as the reference.

## Run

```sh
python3 tests/conformance/harness/runner.py --topic tests/conformance/process \
    --service process-conformance
python3 tests/conformance/harness/runner.py --topic tests/conformance/process \
    --service process-conformance --phase mutation --mutation signal-as-exit
```
