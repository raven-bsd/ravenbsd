// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
.text
.globl longjmp
.type longjmp,@function
longjmp: jmp __rl_longjmp
.globl _longjmp
.type _longjmp,@function
_longjmp: jmp __rl_longjmp
.globl siglongjmp
.type siglongjmp,@function
siglongjmp: jmp __rl_longjmp
"#,
    options(att_syntax)
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
.text
.globl longjmp
.type longjmp,%function
longjmp: b __rl_longjmp
.globl _longjmp
.type _longjmp,%function
_longjmp: b __rl_longjmp
.globl siglongjmp
.type siglongjmp,%function
siglongjmp: b __rl_longjmp
"#
);
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
.text
.globl longjmp
.type longjmp,@function
longjmp: tail __rl_longjmp
.globl _longjmp
.type _longjmp,@function
_longjmp: tail __rl_longjmp
.globl siglongjmp
.type siglongjmp,@function
siglongjmp: tail __rl_longjmp
"#
);
