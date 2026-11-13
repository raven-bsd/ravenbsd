<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-sys

The only ravenlibc crate that executes a Raven system call: the per-architecture
`syscall0..6` stubs, one safe typed wrapper per `raven_abi::syscall::Syscall`, the errno
cell and TLS-provider seam, the thread pointer, the C8 auxv reader, cancellation hooks, the
`PageProvider` raw implementation and the deterministic `host-fake` kernel.

The crate is `no_std`; `std` is compiled only for tests or the `host-fake` feature. `unsafe` is
class U and every block has a `// SAFETY:` note. The stubs are copied once from the project's own
`userland/rt/src/sys.rs` (BSD-3-Clause, same project); no donor code is copied.

## Features

- `host-fake`: compile the in-memory deterministic kernel and route the typed wrappers through
  it. Used by C55/C56/C57/C58/C60 host tests.
- `cancel`: compile the cancellation-point wrappers (`cancellable`, `restartable`, `raw`).

## S-1 mechanism

The weak-hook spike showed that a strong override does not survive fat LTO with the pinned
toolchain. The shipped `weak_hook!` therefore uses the documented static function-pointer cell
installed by `__raven_libc_init`; `unimplemented_symbol!` emits its own `.raven_unimpl` record in
its own section, so the linker can drop it when unreferenced and `-u` can keep it.

## Pending PG calls

`pending.rs` exposes `thread_set_tls` (PG-1), `thread_kick` (PG-7), `getrandom` (PG-52),
`poll_once` (PG-3), `proc_fork` (PG-45), `clock_set` (PG-17), `sysinfo` (PG-21) and `madvise`
(PG-15), all returning `ENOSYS` and carrying `PENDING = true`.

## Checks

```
cargo test -p raven-libc-sys
cargo test -p raven-libc-sys --features host-fake
cargo clippy -p raven-libc-sys --all-targets --features host-fake -- -D warnings
cargo build -p raven-libc-sys --target x86_64-unknown-none
cargo build -p raven-libc-sys --target aarch64-unknown-none-softfloat
cargo build -p raven-libc-sys --target riscv64gc-unknown-none-elf
cargo doc -p raven-libc-sys --no-deps
cargo xtask libc-lint
```
