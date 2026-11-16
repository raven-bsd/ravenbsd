// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target entry for the openssl-version qtest. It calls the upstream apps/version.c entry point
// and routes printf output to the RavenBSD debug console. The installed bin/openssl remains the
// unmodified upstream application; this wrapper exists because a freshly spawned C process has
// no POSIX stdout fd yet.

#include <openssl/bio.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>

int version_main(int argc, char **argv);

// Normally defined by apps/openssl.c; the version command only uses them on its error path.
BIO *bio_err;
char *default_config_file;

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
static long raven_debug_write(const char *bytes, size_t len) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t length asm("rsi") = len;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(length)
                     : "rcx", "r11", "memory");
    return number;
}

int printf(const char *format, ...) {
    char buffer[4096];
    va_list arguments;
    va_start(arguments, format);
    int length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(buffer)) {
            size = sizeof(buffer) - 1;
        }
        (void)raven_debug_write(buffer, size);
    }
    return length;
}

int main(void) {
    char program[] = "openssl";
    char *argv[] = {program, NULL};
    return version_main(1, argv);
}
