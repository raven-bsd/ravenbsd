<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# strace fixtures

Synthetic samples (hand-written, from `tickets/bob/assets/B05/fixtures/`):

* `hello-musl.strace` - static musl hello, loader/signal setup.
* `threads-excerpt.strace` - interleaved `clone`/`getpid`/`futex`/`mmap`.
* `summary-excerpt.strace` - `strace -c` summary table.
* `tricky.strace` - escaped path, nested `ioctl`/`fcntl`/`prctl`, signal and pid prefixes.

Real captures (OBSERVED on the development host, Linux 5.19, strace 5.16, each
under 200 KiB):

* `real-static.strace` - `busybox true`.
* `real-dynamic.strace` - `ls -la /etc`.
* `real-service.strace` - `openssl version`.
* `real-language.strace` - `python3 -c 'print(1)'`.
* `real-toolchain.strace` - `rustc --version`.
* `real-shell.strace` - `dash -c 'echo hi'`.
* `real-net.strace` - the Python loopback epoll example.

The parser treats these as untrusted input; no fixture is used at run time.
