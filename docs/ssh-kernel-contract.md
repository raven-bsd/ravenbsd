# SSH kernel contract: SIGCHLD and session close

Status: K443 OBSERVED on x86-64. The guest fixture is
`tests/smp/ssh-contract/sigchld.c`; the registered qtest case is
`tests/qtest/cases/ssh-contract.toml`.

## Contract rows

| Row | Assertion | Result |
|---|---|---|
| K11 | A child exits 7 while the parent sleeps in `ppoll` on an idle socket and a self-pipe. `ppoll` is interrupted (`EINTR`), the handler writes one byte, the blocking self-pipe read returns 1, and `waitpid` returns the child with `WEXITSTATUS(status) == 7`. `SA_RESTART` clear. | PASS |
| K12 | The same row with `SA_RESTART` set. `ppoll` is not restarted; the handler still writes the self-pipe byte. | PASS |
| K13 | A blocking `read` on the empty self-pipe sleeps until the handler writes; it returns the byte and never `EAGAIN`. `SA_RESTART` set. | PASS |
| K14 | Two children exit 7 and 8 while the parent sleeps. Two `SIGCHLD` records are delivered and `waitpid(-1, &status, 0)` reaps both exact statuses. | PASS |

## POSIX

* `sigaction(2)`: a signal handler runs asynchronously; `SA_RESTART` does not
  make `poll`/`ppoll` restart.
* `ppoll(2)`: returns `EINTR` when a caught signal arrives while it is blocked.
* `waitpid(2)`: `pid == -1` waits for any child; the caller reaps each exit
  status exactly once.

## Donors (behaviour, exact lines)

FreeBSD (BSD-2-Clause), `sys/kern`:

* `kern_exit.c` `exit1` line 269; the SIGCHLD queue is
  `pksignal(q->p_reaper, SIGCHLD, ksi1)` at line 571.
* `kern_sig.c` `kern_psignal` line 2234; `kern_sigaction` line 791;
  `kern_sigprocmask` line 1122.
* `kern_exit.c` `kern_wait4` line 907 and `kern_wait6` line 1463.
* `sys_generic.c` `kern_poll` line 1675; poll is not restarted after a signal
  (lines 1358-1360).

NetBSD (BSD-2-Clause), `sys/kern`:

* `kern_exit.c` `exit1` line 201; `p_exitsig` line 544.
* `kern_sig.c` `psignal` line 1025; `sendsig` line 2142.
* `sys_sig.c` `sys___sigaction_sigtramp` line 93; `sigaction1` line 382.
* `kern_exit.c` `sys_wait6` line 764.
* `sys_select.c` `pollcommon` line 626; `EINTR` at line 348.

Illumos and Linux were read for outcome only; no CDDL or GPL code is present
in the implementation.

## Raven path

1. A child exit calls `process::notify_parent`, which queues `SIGCHLD` on the
   parent and calls `Process::queue_signal`.
2. `queue_signal` keeps the per-process `PROC_SIGRECV` queue, wakes its signal
   waiter, and sets `THREAD_KICK` bit 0 on every live thread that has a
   registered `SIG_SETUP` entry. A process that has not installed a handler is
   left alone: its signal stays in the queue for `PROC_SIGRECV` instead of
   producing a handlerless `EINTR`. An interruptible `EVQ_WAIT`, pipe read or
   `PROC_WAIT_EX` returns `EINTR`.
3. On the syscall return path the kernel resolves the registered `SIG_SETUP`
   entry, saves the interrupted `mcontext_t` and resumes libc's
   `__raven_sigentry`.
4. The libc entry pops the queued signal with `PROC_SIGRECV(WNOHANG)`, runs the
   `sigaction` handler on a stack below the live entry frame, and issues
   `SIGRETURN`.
5. `waitpid(-1, ...)` selects the kernel `P_ALL=3` id type; `P_PID=1` and
   `P_PGID=2` agree with the public header.

The x86-64 context window is the unused tail of the one-page builtin TLS block,
registered as an offset from the thread pointer. `Pthread.sig_private` holds
the libc signal state and is never overwritten.

## Verification

```text
timeout 600 cargo xtask qtest --case ssh-contract --arch x86_64
timeout 900 make check-fast
```

The qtest expects `CONTRACT K11..K14 PASS`, `CONTRACT SUMMARY PASS 4/4` and
`init exited (0)`.

## Limits

* Secondary threads created with `pthread_create` map an exact-size TLS block;
  this slice registers the context window only for the builtin main-thread TLS
  image and returns `ENOMEM` for a layout that fills the page.
* `PROC_SIGRECV` has no peek operation; a process signal masked while the entry
  pops it is not put back. K443 exercises the unmasked SIGCHLD path used by the
  SSH server loop.

## Integration status

The K434 gate matrix first ran against a release image at commit `a8059977`.
`netd` reached `netd: ready` and then restarted into `netd: net: already
exists` / `svcd: netd crash loop, giving up`. The registered-entry gate
(`096f97e0`) removed that restart; a second run with the `096f97e0` release
kernel copied into the same image reached sshd and passed S1/S2/S3/S4/S6.
S5-session still exits -9 and S7 still returns 0 command bytes (the
`/tmp/ssh-marker` fixture is not staged on this branch). The matrix therefore
remains an open K434 integration item after K440/K441/K444 are merged.
