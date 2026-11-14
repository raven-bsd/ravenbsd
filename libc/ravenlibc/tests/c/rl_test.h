/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * The common C73 test header. It intentionally has no stdio dependency: the first process
 * starts with an empty fd table, so `rl_emit` probes fd 1 and opens the console with
 * `raven_open_scheme("tty:console", O_WRONLY)` when the write fails with EBADF. Every marker is
 * printed after its check; a skipped check prints `SKIP`, never a pass marker.
 */

#ifndef RL_TEST_H
#define RL_TEST_H

#include <unistd.h>
#include <fcntl.h>
#include <errno.h>

#if defined(__has_include)
#if __has_include(<raven/scheme.h>)
#include <raven/scheme.h>
#define RL_HAVE_SCHEME 1
/* C64a may still be adding the C37/C38 export; a weak reference keeps the test linkable
 * before the shell binds it and resolves to the real function once it lands. */
#if defined(__GNUC__)
extern int raven_open_scheme(const char *, int, ...) __attribute__((weak));
#define RL_WEAK_SCHEME 1
#endif
#endif
#endif

#ifndef RL_CASE
#error "define RL_CASE before including rl_test.h"
#endif

#define RL_LINE_MAX 255

static int rl_failures;
static int rl_console_ready;

static inline int rl_finish(void);

static inline unsigned long rl_strlen(const char *text)
{
    unsigned long n = 0;
    while (text[n] != '\0') {
        n++;
    }
    return n;
}

static inline void rl_copy(char *to, const char *from)
{
    while (*from != '\0') {
        *to++ = *from++;
    }
    *to = '\0';
}

static inline void rl_open_console(void)
{
#ifdef RL_HAVE_SCHEME
    int fd;

    if (rl_console_ready) {
        return;
    }
#ifdef RL_WEAK_SCHEME
    if (raven_open_scheme == NULL) {
        return;
    }
#endif
    fd = raven_open_scheme("tty:console", O_WRONLY);
    if (fd < 0) {
        return;
    }
    if (fd != 1 && dup2(fd, 1) < 0) {
        close(fd);
        return;
    }
    if (fd != 2) {
        (void)dup2(fd, 2);
    }
    if (fd > 2) {
        close(fd);
    }
    rl_console_ready = 1;
#endif
}

static inline void rl_emit(const char *text, unsigned long length)
{
    unsigned long done = 0;
    int attempt;

    if (length > RL_LINE_MAX) {
        length = RL_LINE_MAX;
    }
    for (attempt = 0; attempt < 2; attempt++) {
        while (done < length) {
            ssize_t written = write(1, text + done, length - done);
            if (written > 0) {
                done += (unsigned long)written;
                continue;
            }
            if (written < 0 && errno == EBADF && attempt == 0) {
                rl_open_console();
                if (!rl_console_ready) {
                    (void)write(2, text, length);
                    return;
                }
                break;
            }
            return;
        }
        if (done == length) {
            return;
        }
    }
}

static inline void rl_out(const char *text)
{
    char line[RL_LINE_MAX + 1];
    unsigned long length = rl_strlen(text);
    unsigned long index = 0;

    if (length > RL_LINE_MAX - 1) {
        length = RL_LINE_MAX - 1;
    }
    while (index < length) {
        line[index] = text[index];
        index++;
    }
    line[index++] = '\n';
    rl_emit(line, index);
}

static inline void rl_append(char **cursor, const char *text)
{
    while (*text != '\0') {
        *(*cursor)++ = *text++;
    }
    **cursor = '\0';
}

static inline void rl_append_uint(char **cursor, long value)
{
    char digits[24];
    int count = 0;
    unsigned long number;

    if (value < 0) {
        *(*cursor)++ = '-';
        number = (unsigned long)(-(value + 1)) + 1;
    } else {
        number = (unsigned long)value;
    }
    do {
        digits[count++] = (char)('0' + (number % 10));
        number /= 10;
    } while (number != 0);
    while (count > 0) {
        *(*cursor)++ = digits[--count];
    }
    **cursor = '\0';
}

static inline void rl_pass(const char *name)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": ");
    rl_append(&cursor, name);
    rl_append(&cursor, " ok");
    rl_out(line);
}

static inline void rl_fail(const char *name, const char *file, int line_number)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_failures++;
    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": FAIL ");
    rl_append(&cursor, name);
    rl_append(&cursor, " (");
    rl_append(&cursor, file);
    rl_append(&cursor, ":");
    rl_append_uint(&cursor, line_number);
    rl_append(&cursor, ")");
    rl_out(line);
}

static inline void __attribute__((unused)) rl_skip(const char *name, const char *needs)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": SKIP ");
    rl_append(&cursor, name);
    rl_append(&cursor, " needs ");
    rl_append(&cursor, needs);
    rl_out(line);
}

#define RL_CHECK(name, condition)                                           \
    do {                                                                    \
        if (condition) {                                                    \
            rl_pass(name);                                                  \
        } else {                                                            \
            rl_fail(name, __FILE__, __LINE__);                              \
        }                                                                   \
    } while (0)

#define RL_REQUIRE(condition)                                               \
    do {                                                                    \
        if (!(condition)) {                                                 \
            rl_fail(#condition, __FILE__, __LINE__);                        \
            (void)rl_finish();                                              \
            _exit(1);                                                       \
        }                                                                   \
    } while (0)

static inline int rl_finish(void)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    if (rl_failures == 0) {
        rl_append(&cursor, ": done");
        rl_out(line);
        return 0;
    }
    rl_append(&cursor, ": FAILED ");
    rl_append_uint(&cursor, rl_failures);
    rl_out(line);
    return 1;
}

#endif /* RL_TEST_H */
