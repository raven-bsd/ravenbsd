/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * One thread-local byte for the dynamic case. The C test program itself has no TLS, but
 * `pthread_create` needs at least one recorded TLS module; the plug exercises the real TLS path.
 */
.section .tdata,"awT",@progbits
.globl __rl_dynamic_tls_stub
.type __rl_dynamic_tls_stub,@object
.size __rl_dynamic_tls_stub,1
__rl_dynamic_tls_stub:
    .byte 0
.section .note.GNU-stack,"",@progbits
