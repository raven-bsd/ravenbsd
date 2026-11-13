<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `raven-libc/src/va`: variadic trampolines and `va_list` readers

Stable Rust cannot define a C-variadic function, so every exported variadic symbol of
ravenlibc is a `global_asm!` stub. One macro, `raven_variadic!` (`trampoline.rs`), emits one
stub per row of the data table `for_each_variadic!` (`table.rs`); the stub spills the argument
registers, builds the platform's `va_list` in its own frame and calls a normal Rust `v*`
target with that list. The readers implement `raven_libc_fmt::VaReader` and
`VaReaderExt`, and `CEnv` (`env.rs`) is the only code of the module that dereferences a C
pointer.

## Layouts (the contract: `donor-map/04` 3.9.1-3.9.3, ticket C59 section 4.4)

| Arch | Save area | Frame | `va_list` | Reader |
|---|---|---|---|---|
| x86_64 SysV | `VaSaveSysV` 176 B, align 16: `gp[6]` at 0, `xmm[8][16]` at 48 | 208 B; entry `rsp % 16 == 8`, after `push` 0, after `sub $208` 0 | `VaListSysV` 24 B at frame+176: `gp_offset = 8*N`, `fp_offset = 48`, `overflow_arg_area = rbp+16`, `reg_save_area = rsp` | `SysvReader` |
| aarch64 AAPCS64 | `VaSaveAapcs64` 208 B, align 16: `x29 x30` at 0, `x0..x7` at 16, `q0..q7` at 80 | 240 B | `VaListAapcs64` 32 B at frame+208: `stack = sp+240`, `gr_top = sp+80`, `vr_top = sp+208`, `gr_offs = -(64-8*N)`, `vr_offs = -128` | `Aapcs64Reader` |
| riscv64 psABI | `VaSaveRiscv64` 80 B, align 16: `ra`+pad at 0, `a0..a7` at 16 (ends at the entry `sp`) | 80 B | `*mut u8` cursor = `sp+16+8*N`, passed by value | `RiscvReader` |

The readers copy the list on construction and never write back into the caller's `va_list`
(RD-new 4): C says the caller's list is indeterminate after a `v*` call, and glibc's
advancing behaviour is deliberately not reproduced.

`x86_64` integer and pointer arguments come from the six GP slots and then the overflow
area; a `double` comes from the low 8 bytes of a 16-byte XMM slot and then the overflow area;
a `long double` is memory class, 16-byte aligned in the overflow area, 10 meaningful bytes
(x87) with bits 80..128 returned as zero. aarch64 walks the two negative offsets of
AAPCS64 B.5; riscv64 walks a contiguous cursor where `double` uses the integer convention
and `long double` is 16-byte aligned.

## The OBSERVED soft-float rule (RD-new 1)

`clang --target=x86_64-unknown-none-elf -msoft-float -mno-sse -O1 -S` passes the doubles of a
variadic call on the STACK (`pushq` of the bit patterns, `%al = 0`) while the integers stay in
`edi esi edx ecx r8d r9d`; a `long double` is still an x87 memory argument. The area document
`donor-map/04` 3.9.1 said a soft-float `double` was read "like an integer from the next GP
slot"; that is wrong and this module implements the observed rule: under
`cfg(raven_va_softfloat)` the stub does not touch `xmm`, stores `fp_offset = 176`, does not
test `%al`, and `next_f64_bits` reads the overflow area only. See `tests/libc/va/softfloat.rs`.

Until PG-12 (eager FP/SIMD state save) the soft-float profile is the only valid x86_64 libc
build (H-20/CR-21).

## The documented struct-by-value subset (RD-new 5)

Integer-class aggregates only, sizes 8, 12, 16, 24 and 32 bytes, 8-byte alignment; fetched by
`VaReaderExt::next_aggregate_int`. Anything else returns `false` and consumes nothing. The
x86_64 rows are verified against clang; the aarch64 and riscv64 rows are model-tested only.

## Flipping an architecture to "verified" (RD-new 8)

aarch64 and riscv64 are specifications, not tested. Until a `qemu-user` lane passes, the
macro emits a LINK-TIME FAILING stub (`b`/`tail __raven_variadic_unsupported_<name>`) for
every non-`v` form, so a program that calls `printf` does not link, and only the `v*` forms
work. The reviewer flips an architecture by adding the build-profile flag:

```
RUSTFLAGS='--cfg raven_va_verified_aarch64'    # only after the qemu lane passed
RUSTFLAGS='--cfg raven_va_verified_riscv64'    # same
```

The flags are a build-profile decision, not a source edit; nobody else sets them.

## Provenance

First-party BSD-3-Clause. `sysv_x86_64.rs` and `trampoline.rs` DERIVE from the project's own
`libc/spike/va` (K03, same author) under the `internal-copy` rule of `17-crate-layout.md`
section 12; the spike's `snprintf.rs` is not used (it has `core::fmt` and `f64`). The
aarch64 and riscv64 files are CLEANROOM from the AAPCS64 B.5 and riscv64 psABI texts: no
donor file was read for code. glibc and musl were read for behaviour only; no GPL/LGPL text
is copied here.
