<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell child wait (K171)

RavenBSD's `PROC_WAIT_EX` reports exited children as well as stopped ones and has no separate
`WEXITED` bit. The shell's stop pre-poll must therefore not call `waitid(WUNTRACED|WNOHANG)` on
the run path: a fast static child (`uname`, `true`, a usage-only `svcctl`) can exit before the
poll, the poll consumes/reaps the exit status, and the later `try_wait` returns `ECHILD`
(`No child process (os error 10)`).

`brush-core` now makes `poll_for_stopped_child` and `poll_for_stopped_children` non-reaping
no-ops on RavenBSD:

```rust
#[cfg(target_os = "ravenbsd")]
pub(crate) fn poll_for_stopped_children(_pid: ProcessId) -> Result<bool, error::Error> {
    Ok(false)
}
```

The exec future remains the single wait/reap path, so the real exit status is returned and a
failed command keeps its status. Job-control stop polling remains on the other platforms.

The bounded `shell-wait` qtest runs `uname -a`, `true`, `hostname`, `svcctl` and a five-iteration
loop, checks their output and forbids `No child process`, `i/o error`, `ECHILD` and
`process: child exited` noise.
