// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target-only weak fallbacks for the three BSD diagnostics that the current ravenlibc C ABI
// does not export. The test harness links this object into every target program; a future strong
// libc definition wins over these weak ones, so this never masks an implemented function.

#include <errno.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static void rl_write(const char *text, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t written = write(2, text + done, length - done);
        if (written > 0) {
            done += (size_t)written;
            continue;
        }
        // The guest may not hand the test a stderr descriptor; the serial console is fd 1.
        if (done == 0) {
            (void)!write(1, text, length);
        }
        return;
    }
}

static void rl_diag(int with_errno, const char *fmt, va_list ap) {
    char message[480];
    int written = fmt != NULL ? vsnprintf(message, sizeof(message), fmt, ap) : 0;
    size_t length = written > 0 ? (size_t)written : 0;
    if (length >= sizeof(message)) {
        length = sizeof(message) - 1;
    }
    if (with_errno && length + 3 < sizeof(message)) {
        const char *text = strerror(errno);
        int extra = snprintf(message + length, sizeof(message) - length, ": %s",
                             text != NULL ? text : "unknown error");
        if (extra > 0) {
            size_t add = (size_t)extra;
            length += add < sizeof(message) - length ? add : sizeof(message) - length - 1;
        }
    }
    if (length < sizeof(message) - 1) {
        message[length++] = '\n';
    }
    rl_write(message, length);
}

__attribute__((weak)) void warn(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    rl_diag(1, fmt, ap);
    va_end(ap);
}

__attribute__((weak)) void warnx(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    rl_diag(0, fmt, ap);
    va_end(ap);
}

__attribute__((weak)) void err(int eval, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    rl_diag(1, fmt, ap);
    va_end(ap);
    exit(eval);
    __builtin_unreachable();
}

__attribute__((weak, noreturn)) void errx(int eval, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    rl_diag(0, fmt, ap);
    va_end(ap);
    exit(eval);
    __builtin_unreachable();
}

static const char rl_suffix[] =
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

__attribute__((weak)) char *mkdtemp(char *template) {
    static unsigned long counter;
    size_t length = strlen(template);
    if (length < 6 || strcmp(template + length - 6, "XXXXXX") != 0) {
        errno = EINVAL;
        return NULL;
    }
    char *suffix = template + length - 6;
    for (unsigned attempt = 0; attempt < 100; attempt++) {
        unsigned long value = (counter++ * 1103515245u) ^ ((unsigned long)getpid() << 16);
        for (size_t index = 0; index < 6; index++) {
            suffix[index] = rl_suffix[value % 62];
            value /= 62;
        }
        if (mkdir(template, 0700) == 0) {
            return template;
        }
        if (errno != EEXIST) {
            return NULL;
        }
    }
    errno = EEXIST;
    return NULL;
}

__attribute__((weak, noreturn)) void __assert_fail(const char *expr, const char *file, int line,
                                                   const char *func) {
    char message[480];
    int written = snprintf(message, sizeof(message), "%s:%d: %s: Assertion `%s' failed.\n",
                           file != NULL ? file : "?", line, func != NULL ? func : "?",
                           expr != NULL ? expr : "?");
    size_t length = written > 0 ? (size_t)written : 0;
    if (length >= sizeof(message)) {
        length = sizeof(message) - 1;
    }
    rl_write(message, length);
    _exit(134);
    __builtin_unreachable();
}
