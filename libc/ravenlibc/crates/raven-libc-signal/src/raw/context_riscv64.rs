// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RISC-V `getcontext`/`setcontext`/`swapcontext`/`makecontext` asm (CLEANROOM).
//!
//! The Linux RISC-V `mcontext_t` starts with `__gregs[32]` at bytes 0..255 (`0` pc, `1` ra,
//! `2` sp, `8..27` s0..s11 and a0..a7 at 10..17), then a 528-byte FP union. This crate keeps
//! `fs0-fs11` at 256..351 and `fcsr` at 352 (hard-float profile; the slots stay zero in the
//! soft-float profile but the size does not change). `s0` holds the `ucontext` pointer in a
//! `makecontext` function, `s1` the function and `s2` `uc_link`.

#[cfg(all(not(raven_va_softfloat), target_feature = "d"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_getcontext
    .hidden __rl_getcontext
    .type __rl_getcontext,%function
__rl_getcontext:
    mv a1, a0
    sd s0, 64(a1)
    sd s1, 72(a1)
    sd s2, 144(a1)
    sd s3, 152(a1)
    sd s4, 160(a1)
    sd s5, 168(a1)
    sd s6, 176(a1)
    sd s7, 184(a1)
    sd s8, 192(a1)
    sd s9, 200(a1)
    sd s10, 208(a1)
    sd s11, 216(a1)
    sd ra, 0(a1)
    sd ra, 8(a1)
    sd sp, 16(a1)
    sd zero, 80(a1)
    sd zero, 88(a1)
    sd zero, 96(a1)
    sd zero, 104(a1)
    sd zero, 112(a1)
    sd zero, 120(a1)
    sd zero, 128(a1)
    sd zero, 136(a1)
    fsd fs0, 256(a1)
    fsd fs1, 264(a1)
    fsd fs2, 272(a1)
    fsd fs3, 280(a1)
    fsd fs4, 288(a1)
    fsd fs5, 296(a1)
    fsd fs6, 304(a1)
    fsd fs7, 312(a1)
    fsd fs8, 320(a1)
    fsd fs9, 328(a1)
    fsd fs10, 336(a1)
    fsd fs11, 344(a1)
    frcsr t0
    sw t0, 352(a1)
    li a0, 0
    ret

    .global __rl_setcontext
    .hidden __rl_setcontext
    .type __rl_setcontext,%function
__rl_setcontext:
    fld fs0, 256(a0)
    fld fs1, 264(a0)
    fld fs2, 272(a0)
    fld fs3, 280(a0)
    fld fs4, 288(a0)
    fld fs5, 296(a0)
    fld fs6, 304(a0)
    fld fs7, 312(a0)
    fld fs8, 320(a0)
    fld fs9, 328(a0)
    fld fs10, 336(a0)
    fld fs11, 344(a0)
    lw t0, 352(a0)
    fscsr t0
    mv t6, a0
    ld ra, 8(t6)
    ld sp, 16(t6)
    ld a0, 80(t6)
    ld a1, 88(t6)
    ld a2, 96(t6)
    ld a3, 104(t6)
    ld a4, 112(t6)
    ld a5, 120(t6)
    ld a6, 128(t6)
    ld a7, 136(t6)
    ld s0, 64(t6)
    ld s1, 72(t6)
    ld s2, 144(t6)
    ld s3, 152(t6)
    ld s4, 160(t6)
    ld s5, 168(t6)
    ld s6, 176(t6)
    ld s7, 184(t6)
    ld s8, 192(t6)
    ld s9, 200(t6)
    ld s10, 208(t6)
    ld s11, 216(t6)
    ld t0, 0(t6)
    jr t0

    .global __rl_swapcontext
    .hidden __rl_swapcontext
    .type __rl_swapcontext,%function
__rl_swapcontext:
    addi sp, sp, -24
    sd s0, 0(sp)
    sd s1, 8(sp)
    sd s2, 16(sp)
    mv s2, a0
    mv s0, a1
    mv s1, ra
    call __rl_getcontext
    la t0, 2f
    sd t0, 0(s2)
    sd s1, 8(s2)
    mv a0, s0
    call __rl_setcontext
2:
    ld s0, 0(sp)
    ld s1, 8(sp)
    ld s2, 16(sp)
    addi sp, sp, 24
    li a0, 0
    ret

"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "d")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_getcontext
    .hidden __rl_getcontext
    .type __rl_getcontext,%function
__rl_getcontext:
    mv a1, a0
    sd s0, 64(a1)
    sd s1, 72(a1)
    sd s2, 144(a1)
    sd s3, 152(a1)
    sd s4, 160(a1)
    sd s5, 168(a1)
    sd s6, 176(a1)
    sd s7, 184(a1)
    sd s8, 192(a1)
    sd s9, 200(a1)
    sd s10, 208(a1)
    sd s11, 216(a1)
    sd ra, 0(a1)
    sd ra, 8(a1)
    sd sp, 16(a1)
    sd zero, 80(a1)
    sd zero, 88(a1)
    sd zero, 96(a1)
    sd zero, 104(a1)
    sd zero, 112(a1)
    sd zero, 120(a1)
    sd zero, 128(a1)
    sd zero, 136(a1)
    li a0, 0
    ret

    .global __rl_setcontext
    .hidden __rl_setcontext
    .type __rl_setcontext,%function
__rl_setcontext:
    mv t6, a0
    ld ra, 8(t6)
    ld sp, 16(t6)
    ld a0, 80(t6)
    ld a1, 88(t6)
    ld a2, 96(t6)
    ld a3, 104(t6)
    ld a4, 112(t6)
    ld a5, 120(t6)
    ld a6, 128(t6)
    ld a7, 136(t6)
    ld s0, 64(t6)
    ld s1, 72(t6)
    ld s2, 144(t6)
    ld s3, 152(t6)
    ld s4, 160(t6)
    ld s5, 168(t6)
    ld s6, 176(t6)
    ld s7, 184(t6)
    ld s8, 192(t6)
    ld s9, 200(t6)
    ld s10, 208(t6)
    ld s11, 216(t6)
    ld t0, 0(t6)
    jr t0

    .global __rl_swapcontext
    .hidden __rl_swapcontext
    .type __rl_swapcontext,%function
__rl_swapcontext:
    addi sp, sp, -24
    sd s0, 0(sp)
    sd s1, 8(sp)
    sd s2, 16(sp)
    mv s2, a0
    mv s0, a1
    mv s1, ra
    call __rl_getcontext
    la t0, 2f
    sd t0, 0(s2)
    sd s1, 8(s2)
    mv a0, s0
    call __rl_setcontext
2:
    ld s0, 0(sp)
    ld s1, 8(sp)
    ld s2, 16(sp)
    addi sp, sp, 24
    li a0, 0
    ret

"#
);
