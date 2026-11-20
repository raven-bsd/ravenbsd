// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K225 static writer: the exec target receives the accepted socket on fd 4 and
// writes its banner through the inherited descriptor.

#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static void debug_write(const char *bytes, size_t length) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t size asm("rsi") = length;
    __asm__ volatile("syscall" : "+r"(number) : "r"(pointer), "r"(size)
                     : "rcx", "r11", "memory");
}

static void say(const char *format, ...) {
    char buffer[256];
    va_list arguments;
    va_start(arguments, format);
    int length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(buffer)) {
            size = sizeof(buffer) - 1;
        }
        debug_write(buffer, size);
    }
}

int main(int argc, char **argv) {
    if (argc < 2) {
        say("tcp-accept-writer: missing banner\n");
        return 2;
    }
    const char *banner = argv[1];
    size_t length = strlen(banner);
    ssize_t written = write(4, banner, length);
    if (written != (ssize_t)length) {
        say("tcp-accept-writer: wrote %zd of %zu\n", written, length);
        return 1;
    }
    say("tcp-accept-writer: wrote fd=4 bytes=%zu\n", length);
    close(4);
    return 0;
}
