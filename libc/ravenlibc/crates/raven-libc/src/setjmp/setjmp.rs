// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
.text
.globl setjmp
.type setjmp,@function
setjmp: jmp __rl_setjmp
.globl _setjmp
.type _setjmp,@function
_setjmp: jmp __rl_setjmp
"#,
    options(att_syntax)
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
.text
.globl setjmp
.type setjmp,%function
setjmp: b __rl_setjmp
.globl _setjmp
.type _setjmp,%function
_setjmp: b __rl_setjmp
"#
);
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
.text
.globl setjmp
.type setjmp,@function
setjmp: tail __rl_setjmp
.globl _setjmp
.type _setjmp,@function
_setjmp: tail __rl_setjmp
"#
);
