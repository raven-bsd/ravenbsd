/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Donor reference for S160. Type definitions are copied from:
 * FreeBSD 133344405f532bd54ff3629a77d4c4e062f01fdb
 *   sys/x86/include/sigframe.h, sys/x86/include/ucontext.h,
 *   sys/sys/_ucontext.h, sys/sys/_sigset.h, sys/sys/_sigaltstack.h,
 *   sys/sys/signal.h
 * NetBSD 36eddebcc36cb0775aa2962005b9c05419a6ff01
 *   sys/arch/amd64/include/mcontext.h, sys/arch/amd64/include/frame_regs.h
 */

#include <stddef.h>
#include <stdio.h>

typedef unsigned long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef int i32;

typedef struct { u32 __bits[4]; } fbsd_sigset_t;

typedef struct {
    u64 mc_onstack;
    u64 mc_rdi;
    u64 mc_rsi;
    u64 mc_rdx;
    u64 mc_rcx;
    u64 mc_r8;
    u64 mc_r9;
    u64 mc_rax;
    u64 mc_rbx;
    u64 mc_rbp;
    u64 mc_r10;
    u64 mc_r11;
    u64 mc_r12;
    u64 mc_r13;
    u64 mc_r14;
    u64 mc_r15;
    u32 mc_trapno;
    u16 mc_fs;
    u16 mc_gs;
    u64 mc_addr;
    u32 mc_flags;
    u16 mc_es;
    u16 mc_ds;
    u64 mc_err;
    u64 mc_rip;
    u64 mc_cs;
    u64 mc_rflags;
    u64 mc_rsp;
    u64 mc_ss;
    long mc_len;
    long mc_fpformat;
    long mc_ownedfp;
    long mc_fpstate[64] __attribute__((aligned(16)));
    u64 mc_fsbase;
    u64 mc_gsbase;
    u64 mc_xfpustate;
    u64 mc_xfpustate_len;
    u64 mc_tlsbase;
    long mc_spare[3];
} fbsd_mcontext_t;

typedef struct {
    void *ss_sp;
    u64 ss_size;
    i32 ss_flags;
} fbsd_stack_t;

typedef struct __ucontext {
    fbsd_sigset_t uc_sigmask;
    fbsd_mcontext_t uc_mcontext;
    struct __ucontext *uc_link;
    fbsd_stack_t uc_stack;
    i32 uc_flags;
    i32 __spare__[4];
} fbsd_ucontext_t;

typedef union { i32 sival_int; void *sival_ptr; } fbsd_sigval_t;

typedef struct __siginfo {
    i32 si_signo;
    i32 si_errno;
    i32 si_code;
    i32 si_pid;
    u32 si_uid;
    i32 si_status;
    void *si_addr;
    fbsd_sigval_t si_value;
    union {
        struct { i32 _trapno; } _fault;
        struct { i32 _timerid; i32 _overrun; } _timer;
        struct { i32 _mqd; } _mesgq;
        struct { long _band; } _poll;
        struct { i32 _syscall; } _capsicum;
        struct { long __spare1__; i32 __spare2__[7]; } __spare__;
    } _reason;
} fbsd_siginfo_t;

typedef struct {
    union { void *sf_action; void *sf_handler; } sf_ahu;
    fbsd_ucontext_t sf_uc;
    fbsd_siginfo_t sf_si;
} fbsd_sigframe_t;

typedef u64 nb_greg_t;
typedef nb_greg_t nb_gregset_t[26];

typedef union {
    char __fxsave[512] __attribute__((aligned(8)));
    struct {
        char __fxsave[416];
        char __rsvd[48];
        nb_greg_t __xsaveptr;
        nb_greg_t __xsavelen;
        char __pad[32];
    } __xsave;
} nb_fpregset_t;

typedef struct {
    nb_gregset_t __gregs;
    nb_greg_t _mc_tlsbase;
    nb_fpregset_t __fpregs;
} nb_mcontext_t;

/*
 * aarch64 half (S182/S160): NetBSD sys/arch/arm/include/mcontext.h (`__gregs[35]`,
 * `__fregs`, `__spare[8]`) inside the C19/S160 ucontext field order, with the S160
 * siginfo_t. The 32 qregs are 16-byte aligned and the struct sizes follow the C ABI.
 */
typedef struct {
    u32 __bits[4];
} aarch64_sigset_t;

typedef union {
    unsigned char __b8[16];
    unsigned __int128 __q128;
} aarch64_qreg_t;

typedef struct {
    aarch64_qreg_t __qregs[32] __attribute__((aligned(16)));
    u32 __fpcr;
    u32 __fpsr;
} aarch64_fregset_t;

typedef struct {
    u64 __gregs[35];
    aarch64_fregset_t __fregs;
    u64 __spare[8];
} aarch64_mcontext_t;

typedef struct {
    void *ss_sp;
    u64 ss_size;
    int ss_flags;
} aarch64_stack_t;

typedef struct __aarch64_ucontext {
    aarch64_sigset_t uc_sigmask;
    aarch64_mcontext_t uc_mcontext;
    struct __aarch64_ucontext *uc_link;
    aarch64_stack_t uc_stack;
    int uc_flags;
    int __spare__[4];
} aarch64_ucontext_t;

typedef union {
    int sival_int;
    void *sival_ptr;
} aarch64_sigval_t;

typedef struct {
    int si_signo;
    int si_errno;
    int si_code;
    int si_pid;
    u32 si_uid;
    int si_status;
    void *si_addr;
    aarch64_sigval_t si_value;
    union {
        struct { int _trapno; } _fault;
        struct { int _timerid; int _overrun; } _timer;
        struct { int _mqd; } _mesgq;
        struct { long _band; } _poll;
        struct { int _syscall; } _capsicum;
        struct { long __spare1__; int __spare2__[7]; } __spare__;
    } _reason;
} aarch64_siginfo_t;

#define P(type, field) printf("%s.%s=0x%zx\n", #type, #field, offsetof(type, field))
#define S(type) printf("sizeof(%s)=0x%zx align=%zu\n", #type, sizeof(type), _Alignof(type))

int main(void) {
    S(fbsd_mcontext_t);
    P(fbsd_mcontext_t, mc_onstack);
    P(fbsd_mcontext_t, mc_rdi);
    P(fbsd_mcontext_t, mc_rsi);
    P(fbsd_mcontext_t, mc_rdx);
    P(fbsd_mcontext_t, mc_rcx);
    P(fbsd_mcontext_t, mc_r8);
    P(fbsd_mcontext_t, mc_rip);
    P(fbsd_mcontext_t, mc_rsp);
    P(fbsd_mcontext_t, mc_rflags);
    P(fbsd_mcontext_t, mc_cs);
    P(fbsd_mcontext_t, mc_ss);
    P(fbsd_mcontext_t, mc_fpstate);
    P(fbsd_mcontext_t, mc_fsbase);
    P(fbsd_mcontext_t, mc_tlsbase);
    P(fbsd_mcontext_t, mc_spare);
    S(fbsd_sigset_t);
    S(fbsd_stack_t);
    P(fbsd_stack_t, ss_sp);
    P(fbsd_stack_t, ss_size);
    P(fbsd_stack_t, ss_flags);
    S(fbsd_siginfo_t);
    P(fbsd_siginfo_t, si_signo);
    P(fbsd_siginfo_t, si_errno);
    P(fbsd_siginfo_t, si_code);
    P(fbsd_siginfo_t, si_pid);
    P(fbsd_siginfo_t, si_uid);
    P(fbsd_siginfo_t, si_status);
    P(fbsd_siginfo_t, si_addr);
    P(fbsd_siginfo_t, si_value);
    P(fbsd_siginfo_t, _reason);
    S(fbsd_ucontext_t);
    P(fbsd_ucontext_t, uc_sigmask);
    P(fbsd_ucontext_t, uc_mcontext);
    P(fbsd_ucontext_t, uc_link);
    P(fbsd_ucontext_t, uc_stack);
    P(fbsd_ucontext_t, uc_flags);
    P(fbsd_ucontext_t, __spare__);
    S(fbsd_sigframe_t);
    P(fbsd_sigframe_t, sf_ahu);
    P(fbsd_sigframe_t, sf_uc);
    P(fbsd_sigframe_t, sf_si);
    S(nb_fpregset_t);
    S(nb_mcontext_t);
    P(nb_mcontext_t, __gregs);
    P(nb_mcontext_t, _mc_tlsbase);
    P(nb_mcontext_t, __fpregs);
#define NB_GREG(name, idx) printf("nb_greg_%s=%d\n", #name, idx)
    NB_GREG(RDI, 0);
    NB_GREG(RSI, 1);
    NB_GREG(RDX, 2);
    NB_GREG(RCX, 3);
    NB_GREG(R8, 4);
    NB_GREG(R9, 5);
    NB_GREG(R10, 6);
    NB_GREG(R11, 7);
    NB_GREG(R12, 8);
    NB_GREG(R13, 9);
    NB_GREG(R14, 10);
    NB_GREG(R15, 11);
    NB_GREG(RBP, 12);
    NB_GREG(RBX, 13);
    NB_GREG(RAX, 14);
    NB_GREG(GS, 15);
    NB_GREG(FS, 16);
    NB_GREG(ES, 17);
    NB_GREG(DS, 18);
    NB_GREG(TRAPNO, 19);
    NB_GREG(ERR, 20);
    NB_GREG(RIP, 21);
    NB_GREG(CS, 22);
    NB_GREG(RFLAGS, 23);
    NB_GREG(RSP, 24);
    NB_GREG(SS, 25);
#undef NB_GREG
    S(aarch64_sigset_t);
    S(aarch64_fregset_t);
    P(aarch64_fregset_t, __qregs);
    P(aarch64_fregset_t, __fpcr);
    P(aarch64_fregset_t, __fpsr);
    S(aarch64_mcontext_t);
    P(aarch64_mcontext_t, __gregs);
    P(aarch64_mcontext_t, __fregs);
    P(aarch64_mcontext_t, __spare);
    S(aarch64_stack_t);
    P(aarch64_stack_t, ss_sp);
    P(aarch64_stack_t, ss_size);
    P(aarch64_stack_t, ss_flags);
    S(aarch64_siginfo_t);
    S(aarch64_ucontext_t);
    P(aarch64_ucontext_t, uc_sigmask);
    P(aarch64_ucontext_t, uc_mcontext);
    P(aarch64_ucontext_t, uc_link);
    P(aarch64_ucontext_t, uc_stack);
    P(aarch64_ucontext_t, uc_flags);
    P(aarch64_ucontext_t, __spare__);
#define AARCH64_REG(name, idx) printf("aarch64_reg_%s=%d\n", #name, idx)
    AARCH64_REG(X0, 0);
    AARCH64_REG(X1, 1);
    AARCH64_REG(X2, 2);
    AARCH64_REG(X3, 3);
    AARCH64_REG(X4, 4);
    AARCH64_REG(X5, 5);
    AARCH64_REG(X6, 6);
    AARCH64_REG(X7, 7);
    AARCH64_REG(X8, 8);
    AARCH64_REG(X9, 9);
    AARCH64_REG(X10, 10);
    AARCH64_REG(X11, 11);
    AARCH64_REG(X12, 12);
    AARCH64_REG(X13, 13);
    AARCH64_REG(X14, 14);
    AARCH64_REG(X15, 15);
    AARCH64_REG(X16, 16);
    AARCH64_REG(X17, 17);
    AARCH64_REG(X18, 18);
    AARCH64_REG(X19, 19);
    AARCH64_REG(X20, 20);
    AARCH64_REG(X21, 21);
    AARCH64_REG(X22, 22);
    AARCH64_REG(X23, 23);
    AARCH64_REG(X24, 24);
    AARCH64_REG(X25, 25);
    AARCH64_REG(X26, 26);
    AARCH64_REG(X27, 27);
    AARCH64_REG(X28, 28);
    AARCH64_REG(X29, 29);
    AARCH64_REG(X30, 30);
    AARCH64_REG(SP, 31);
    AARCH64_REG(ELR, 32);
    AARCH64_REG(SPSR, 33);
    AARCH64_REG(TPIDR, 34);
#undef AARCH64_REG
    return 0;
}
