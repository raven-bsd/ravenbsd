<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `raven-rt` formatting rule

[OBSERVED] Freestanding user programs use static linking and the x86-64 formatter
contains GOT vtable calls.  The user linker scripts therefore place `.got*` (and
`.data.rel.ro*`) in the loaded writable data segment; they must never discard those sections.
[OBSERVED] x86-64 syscall asm declares the full C caller-clobbered register set because the
kernel return path does not preserve scratch registers across a syscall.
[OBSERVED] `print!` and `println!` are supported for argument formatting in debug and release
builds. Runtime users may use `core::fmt::Write`, `dyn Trait`, and closures normally.
[PROPOSED] Keep the matching rule in every user architecture linker script when adding targets.
