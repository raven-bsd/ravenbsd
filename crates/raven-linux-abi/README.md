# raven-linux-abi

Linux userspace ABI data for the RavenBSD Linux personality: syscall numbers, errno values,
UAPI flags and `#[repr(C)]` struct layouts for x86-64, aarch64 and riscv64.

The crate is `no_std`, forbids `unsafe`, and has no runtime dependencies. It contains plain
data only.

## Layout

| Module | Contents |
|---|---|
| `sysnums` | Generated syscall tables (`X86_64`, `AARCH64`, `RISCV64`, `ALL_NAMES`) and `number` / `name` lookups |
| `errno` | Linux errno names and values as `u16`, plus `ERRNOS` and `name(value)` |
| `flags` | `O_*`, `AT_*`, `PROT_*`, `MAP_*`, `CLONE_*`, `SIG*`, `F_*`, `FUTEX_*`, `EPOLL*`, `CLOCK_*`, `S_IF*`, `POLL*`, socket and TCP constants |
| `structs` | `x86_64` and `generic` (asm-generic, shared by aarch64 and riscv64) UAPI structs with compile-time size, alignment and field-offset assertions |

Architecture-specific flag values live in `flags::open::{x86_64,aarch64,riscv64}`,
`flags::mmap::{x86_64,aarch64}`, `flags::signal::{x86_64,aarch64,riscv64}` and
`flags::auxv::{x86_64,riscv64}`. Everything else is common.

## Source

All values and layouts come from `linux-raw-sys` 0.12.1 (Apache-2.0 OR MIT), which generates
its bindings from the Linux UAPI headers. No value is copied from the Linux kernel sources,
glibc, `strace` or memory.

## Regenerating `src/sysnums.rs`

`src/sysnums.rs` is generated; do not edit it by hand.

```text
cargo run -q -p raven-linux-abi --example gen_tables
cargo run -q -p raven-linux-abi --example gen_tables -- --check
```

The generator locates the `linux-raw-sys` package through `cargo metadata --offline`, reads
`src/{x86_64,aarch64,riscv64}/general.rs`, extracts every `pub const __NR_name: u32 = N;`
(it drops the `__NR_` prefix), and writes the three name-sorted tables, number-sorted
lookup indexes and `ALL_NAMES`. The file header records the generator command, the
`linux-raw-sys` version and the generation date. `--check` compares the generated tables
with the committed file and exits non-zero on any difference.

The committed `sysnums.rs` is licensed MIT to match the provenance entry; the values are
facts taken from the MIT/Apache-2.0 dual-licensed donor data.

## Verification

`cargo test -p raven-linux-abi` checks:

* every syscall of all three tables against the `linux-raw-sys` sources;
* every struct against `linux-raw-sys` on x86-64 (size, alignment and every field offset);
* the asm-generic layouts that are identical to x86-64 against the same host types, and the
  two layouts that differ (`Stat`, `EpollEvent`) against literal expected offsets;
* every common flag against the host `linux-raw-sys` types, and aarch64/riscv64-specific
  values against the corresponding donor source files;
* every `raven_abi::errno::Errno` constant against the same-named constant in `errno`.

See `TICKET` B06 in `ravenbsd-prjmgmt/tickets/bob/` for the acceptance commands.
