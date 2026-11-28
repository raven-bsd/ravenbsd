// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target-only weak fallbacks for the three BSD diagnostics that the current ravenlibc C ABI
// does not export. The test harness links this object into every target program; a future strong
// libc definition wins over these weak ones, so this never masks an implemented function.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
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

#ifndef AT_FDCWD
#define AT_FDCWD (-100)
#endif

__attribute__((weak)) int isnanf(float value) {
    return __builtin_isnan(value);
}

__attribute__((weak)) int isinff(float value) {
    return __builtin_isinf(value);
}

__attribute__((weak)) int feenableexcept(int mask) {
    (void)mask;
    return 0;
}

__attribute__((weak)) int fedisableexcept(int mask) {
    (void)mask;
    return 0;
}

__attribute__((weak)) long double fmaximum_mag_numl(long double x, long double y) {
    if (__builtin_isnan(x)) {
        return y;
    }
    if (__builtin_isnan(y)) {
        return x;
    }
    long double ax = __builtin_fabsl(x);
    long double ay = __builtin_fabsl(y);
    if (ax > ay) {
        return x;
    }
    if (ay > ax) {
        return y;
    }
    return x > y ? x : y;
}

__attribute__((weak)) long double fminimum_mag_numl(long double x, long double y) {
    if (__builtin_isnan(x)) {
        return y;
    }
    if (__builtin_isnan(y)) {
        return x;
    }
    long double ax = __builtin_fabsl(x);
    long double ay = __builtin_fabsl(y);
    if (ax < ay) {
        return x;
    }
    if (ay < ax) {
        return y;
    }
    return x < y ? x : y;
}

__attribute__((weak)) long double fmaximum_magl(long double x, long double y) {
    if (__builtin_isnan(x) || __builtin_isnan(y)) {
        return __builtin_nanl("");
    }
    long double ax = __builtin_fabsl(x);
    long double ay = __builtin_fabsl(y);
    if (ax > ay) {
        return x;
    }
    if (ay > ax) {
        return y;
    }
    return x > y ? x : y;
}

__attribute__((weak)) long double fminimum_magl(long double x, long double y) {
    if (__builtin_isnan(x) || __builtin_isnan(y)) {
        return __builtin_nanl("");
    }
    long double ax = __builtin_fabsl(x);
    long double ay = __builtin_fabsl(y);
    if (ax < ay) {
        return x;
    }
    if (ay < ax) {
        return y;
    }
    return x < y ? x : y;
}

__attribute__((weak)) long double fmaximuml(long double x, long double y) {
    if (__builtin_isnan(x) || __builtin_isnan(y)) {
        return __builtin_nanl("");
    }
    return x > y ? x : y;
}

__attribute__((weak)) long double fminimuml(long double x, long double y) {
    if (__builtin_isnan(x) || __builtin_isnan(y)) {
        return __builtin_nanl("");
    }
    return x < y ? x : y;
}

__attribute__((weak)) long double fmaximum_numl(long double x, long double y) {
    if (__builtin_isnan(x)) {
        return y;
    }
    if (__builtin_isnan(y)) {
        return x;
    }
    return x > y ? x : y;
}

__attribute__((weak)) long double fminimum_numl(long double x, long double y) {
    if (__builtin_isnan(x)) {
        return y;
    }
    if (__builtin_isnan(y)) {
        return x;
    }
    return x < y ? x : y;
}

static char rl_sbrk_area[1 << 20];
static size_t rl_sbrk_used;

__attribute__((weak)) void *sbrk(intptr_t increment) {
    void *previous = rl_sbrk_area + rl_sbrk_used;
    if (increment > 0) {
        size_t add = (size_t)increment;
        if (add > sizeof(rl_sbrk_area) - rl_sbrk_used) {
            errno = ENOMEM;
            return (void *)-1;
        }
        rl_sbrk_used += add;
    } else if (increment < 0) {
        size_t sub = (size_t)(-increment);
        if (sub > rl_sbrk_used) {
            errno = EINVAL;
            return (void *)-1;
        }
        rl_sbrk_used -= sub;
    }
    return previous;
}

__attribute__((weak)) int pause(void) {
    sigset_t mask;
    sigemptyset(&mask);
    return sigsuspend(&mask);
}

__attribute__((weak)) int mkfifoat(int dirfd, const char *path, mode_t mode) {
    if (path == NULL || path[0] == '/' || dirfd == AT_FDCWD) {
        return mkfifo(path, mode);
    }
    int saved = open(".", O_RDONLY);
    if (saved < 0) {
        return -1;
    }
    if (fchdir(dirfd) != 0) {
        close(saved);
        return -1;
    }
    int result = mkfifo(path, mode);
    int error = errno;
    (void)fchdir(saved);
    close(saved);
    errno = error;
    return result;
}

__attribute__((weak)) int mknodat(int dirfd, const char *path, mode_t mode, dev_t dev) {
    if (path == NULL || path[0] == '/' || dirfd == AT_FDCWD) {
        return mknod(path, mode, dev);
    }
    int saved = open(".", O_RDONLY);
    if (saved < 0) {
        return -1;
    }
    if (fchdir(dirfd) != 0) {
        close(saved);
        return -1;
    }
    int result = mknod(path, mode, dev);
    int error = errno;
    (void)fchdir(saved);
    close(saved);
    errno = error;
    return result;
}

static char rl_domain[256] = "localdomain";
static size_t rl_domain_length = 11;

__attribute__((weak)) int setdomainname(const char *name, size_t length) {
    if (name == NULL || length >= sizeof(rl_domain)) {
        errno = EINVAL;
        return -1;
    }
    memcpy(rl_domain, name, length);
    rl_domain_length = length;
    return 0;
}

__attribute__((weak)) int getdomainname(char *name, size_t length) {
    if (name == NULL || length < rl_domain_length + 1) {
        errno = EINVAL;
        return -1;
    }
    memcpy(name, rl_domain, rl_domain_length);
    name[rl_domain_length] = '\0';
    return 0;
}

__attribute__((weak)) int asprintf(char **strp, const char *fmt, ...) {
    if (strp == NULL) {
        return -1;
    }
    va_list ap;
    va_start(ap, fmt);
    va_list copy;
    va_copy(copy, ap);
    int length = vsnprintf(NULL, 0, fmt, ap);
    va_end(ap);
    if (length < 0) {
        va_end(copy);
        return -1;
    }
    char *buffer = malloc((size_t)length + 1);
    if (buffer == NULL) {
        va_end(copy);
        return -1;
    }
    int written = vsnprintf(buffer, (size_t)length + 1, fmt, copy);
    va_end(copy);
    if (written < 0) {
        free(buffer);
        return -1;
    }
    *strp = buffer;
    return written;
}

__attribute__((weak)) char *asctime_r(const struct tm *tm, char *buffer) {
    static const char *days[] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
    static const char *months[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                   "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
    if (tm == NULL || buffer == NULL || tm->tm_wday < 0 || tm->tm_wday > 6
        || tm->tm_mon < 0 || tm->tm_mon > 11) {
        return NULL;
    }
    (void)snprintf(buffer, 26, "%.3s %.3s %2d %02d:%02d:%02d %d\n",
                   days[tm->tm_wday], months[tm->tm_mon], tm->tm_mday,
                   tm->tm_hour, tm->tm_min, tm->tm_sec, tm->tm_year + 1900);
    return buffer;
}

__attribute__((weak)) char *ctime_r(const time_t *timep, char *buffer) {
    struct tm value;
    if (timep == NULL || localtime_r(timep, &value) == NULL) {
        return NULL;
    }
    return asctime_r(&value, buffer);
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
