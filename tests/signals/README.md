# S185 signal acceptance fixture

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

This is the independent C19 acceptance fixture for S44. It is test-only and must be red on the
current pre-S44 image and green after S44 with no row changed.

## Rows

- `signal: handler ok sig=10` - `sigaction` installed a SIGUSR1 handler; the handler ran with the
  right signal number and `siginfo.si_signo`.
- `signal: ucontext ok rip=... rsp=...` - the handler's `ucontext_t` machine context has a non-zero
  RIP within the raise path and an RSP within the interrupted stack window.
- `signal: return ok` - execution continues after `raise()` returned; the post-raise flag is set.
- `signal: nested ok sig=12` - a second signal (SIGUSR2) raised inside the SIGUSR1 handler runs its
  own handler.
- `signal: exec-default ok signal=10` - a forked child installs no handler after `exec`, raises
  SIGUSR1, and dies by the default SIGUSR1 action instead of inheriting the old image's handler.
- `signal: ok` and `init exited (0)`.

## Run

```sh
timeout 180 cargo xtask qtest --case tests/qtest/cases/signal-acceptance.toml --arch x86_64
```

The current image uses the retired `SIG_SETUP` model and is expected to fail these rows; the qtest
records the red serial. After S44 the same case must pass with no expectation or code change.

## Donor map

- FreeBSD `sys/kern/kern_sig.c` (`postsig`, `execsigs`), `sys/amd64/amd64/machdep.c` (`sendsig`),
  `sys/x86/include/sigframe.h`, `sys/x86/include/ucontext.h`; BSD-2-Clause.
- NetBSD `sys/arch/amd64/amd64/machdep.c` (`sendsig_siginfo`, `sp = tf_rsp - 128`),
  `sys/arch/amd64/include/mcontext.h`; BSD-2/3-Clause.
- Illumos `uts/common/os/sig.c`: behaviour only.
- Linux `arch/x86/kernel/signal.c`: outcome only, GPL; read-only.
- Contract: C19 signal frame, trampoline and return ABI.
