/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * aarch64 donor reference for the S182/S160 signal frame. The NetBSD aarch64
 * `mcontext_t` definitions are from
 * sys/arch/arm/include/mcontext.h (NetBSD 36eddebcc36cb0775aa2962005b9c05419a6ff01);
 * the `ucontext_t` field order and `siginfo_t` size are the C19/S160 product target
 * (FreeBSD sys/x86/include/ucontext.h, sys/sys/_ucontext.h, sys/sys/_sigset.h,
 * sys/sys/_sigaltstack.h) with the aarch64 machine context.
 */

#include <stddef.h>
#include <stdio.h>

typedef unsigned long long u64;
typedef unsigned int u32;

typedef struct { u32 __bits[4]; } aarch64_sigset_t;

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

typedef struct __ucontext {
    aarch64_sigset_t uc_sigmask;
    aarch64_mcontext_t uc_mcontext;
    struct __ucontext *uc_link;
    aarch64_stack_t uc_stack;
    int uc_flags;
    int __spare__[4];
} aarch64_ucontext_t;

typedef union { int sival_int; void *sival_ptr; } aarch64_sigval_t;

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

#define S(type) printf("sizeof(%s)=0x%zx align=%zu\n", #type, sizeof(type), _Alignof(type))
#define P(type, field) printf("%s.%s=0x%zx\n", #type, #field, offsetof(type, field))

int main(void) {
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
    return 0;
}
