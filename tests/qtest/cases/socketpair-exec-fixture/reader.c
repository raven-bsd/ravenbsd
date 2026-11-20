/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K195: static reader for the socketpair-exec fixture. Reads fd 4 and proves 563 'C' bytes,
 * then 191 'K' bytes, then EOF. */

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define CONFIG_BYTES 563
#define KEY_BYTES 191

static long raven_debug_write(const char *bytes, size_t len) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(3L), "D"(bytes), "S"(len)
                     : "rcx", "r11", "memory");
    return result;
}

static void say(const char *text) {
    (void)raven_debug_write(text, strlen(text));
}

static void say_line(const char *format, long value) {
    char line[128];
    int length = snprintf(line, sizeof(line), format, value);
    if (length > 0)
        (void)raven_debug_write(line, (size_t)length);
}

static int fail(const char *what, int value) {
    char line[128];
    int length = snprintf(line, sizeof(line), "socketpair-exec: fail %s=%d errno=%d\n", what,
        value, errno);
    if (length > 0)
        (void)raven_debug_write(line, (size_t)length);
    return 1;
}

int main(void) {
    static char buffer[4096];
    size_t total = 0;
    size_t position = 0;
    int saw_eof = 0;

    say("socketpair-exec: reader start\n");
    for (;;) {
        ssize_t count = read(4, buffer, sizeof(buffer));
        if (count < 0) {
            if (errno == EINTR)
                continue;
            return fail("read", errno);
        }
        if (count == 0) {
            saw_eof = 1;
            break;
        }
        for (ssize_t index = 0; index < count; index++) {
            char expected = position < CONFIG_BYTES ? 'C' : 'K';
            if (buffer[index] != expected)
                return fail("byte", (int)(position + (size_t)index));
            position++;
        }
        total += (size_t)count;
    }
    say_line("socketpair-exec: bytes=%ld\n", (long)total);
    if (total != CONFIG_BYTES + KEY_BYTES)
        return fail("total", (int)total);
    say("socketpair-exec: exact ok\n");
    if (!saw_eof)
        return fail("eof", 0);
    say("socketpair-exec: eof ok\n");
    say("socketpair-exec: ok\n");
    return 0;
}
