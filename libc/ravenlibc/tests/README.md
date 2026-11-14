# libc/ravenlibc/tests

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

First-party C programs for the C73 on-target cases. They are built by
`tests/libc/target/build.sh` against the ravenlibc sysroot and run on RavenBSD under QEMU; they
are not linked into a shipped artifact.

* `c/rl_test.h` is the shared header. It writes one complete line per marker with `write(2)`
  and, on `EBADF`, opens `tty:console` through `raven_open_scheme` (a weak reference while C64a
  is still adding the export). It has no stdio dependency, so `libc-hello` can use it before
  stdio is trusted.
* `c/libc-hello.c`, `libc-stdio.c`, `libc-time.c`, `libc-threads.c`, `libc-signals.c`,
  `libc-dynamic.c`, `libc-spawn.c` are the seven cases. `c/libc-child.c` is the spawned initfs
  child and `c/libplug.c` the shared object loaded by the dynamic case.
* `vectors/stdio-expected.txt` and `vectors/time-expected.txt` are hand-written expected
  strings from the C standard and POSIX. They are installed at `/etc/stdio-expected.txt` and
  `/etc/time-expected.txt` by the `libc-test` profile and compared by the programs; they are
  never captured from ravenlibc.
* A marker is printed only after its check. A check whose kernel or libc primitive is missing
  prints `<case>: SKIP <name> needs PG-n` and is not matched by the case's `[[expect]]` list.
* The seven cases are blocked today (the probe reports the missing HARD PGs); `build.sh`
  therefore exits 77 unless `RL_FORCE=1` is set. A forced build proves compilation and linking
  only and never turns a case on.
