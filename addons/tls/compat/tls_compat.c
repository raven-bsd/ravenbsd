// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Portability shim for the OpenSSL add-on. The RavenBSD libc does not yet export the full POSIX
// surface that OpenSSL's optional CLI paths reference; these helpers keep the upstream source
// unmodified. None of them implements cryptography. `chmod` uses fchmod(2); memory locking and
// msync are no-ops because the add-on runs as a normal process; the syslog family writes to
// stderr; the path-creation/metadata calls report ENOSYS where no libc wrapper exists.

#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

int chmod(const char *path, mode_t mode) {
    int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return -1;
    }
    int result = fchmod(fd, mode);
    int saved = errno;
    close(fd);
    errno = saved;
    return result;
}

int chown(const char *path, uid_t uid, gid_t gid) {
    (void)path;
    (void)uid;
    (void)gid;
    errno = ENOSYS;
    return -1;
}

int fchown(int fd, uid_t uid, gid_t gid) {
    (void)fd;
    (void)uid;
    (void)gid;
    errno = ENOSYS;
    return -1;
}

int link(const char *oldpath, const char *newpath) {
    (void)oldpath;
    (void)newpath;
    errno = ENOSYS;
    return -1;
}

int symlink(const char *target, const char *linkpath) {
    (void)target;
    (void)linkpath;
    errno = ENOSYS;
    return -1;
}

int utimes(const char *path, const struct timeval times[2]) {
    (void)path;
    (void)times;
    errno = ENOSYS;
    return -1;
}

int futimes(int fd, const struct timeval times[2]) {
    (void)fd;
    (void)times;
    errno = ENOSYS;
    return -1;
}

int mlock(const void *addr, size_t len) {
    (void)addr;
    (void)len;
    return 0;
}

int munlock(const void *addr, size_t len) {
    (void)addr;
    (void)len;
    return 0;
}

int madvise(void *addr, size_t len, int advice) {
    (void)addr;
    (void)len;
    (void)advice;
    return 0;
}

int msync(void *addr, size_t len, int flags) {
    (void)addr;
    (void)len;
    (void)flags;
    return 0;
}

unsigned int alarm(unsigned int seconds) {
    (void)seconds;
    return 0;
}

double atof(const char *text) {
    (void)text;
    return 0.0;
}

int pipe(int fds[2]) {
    (void)fds;
    errno = ENOSYS;
    return -1;
}

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
           struct timeval *timeout) {
    (void)nfds;
    (void)readfds;
    (void)writefds;
    (void)exceptfds;
    (void)timeout;
    errno = ENOSYS;
    return -1;
}

void setbuf(FILE *stream, char *buffer) {
    (void)stream;
    (void)buffer;
}

static const char *log_ident = "openssl";

void openlog(const char *ident, int option, int facility) {
    (void)option;
    (void)facility;
    if (ident != NULL) {
        log_ident = ident;
    }
}

void closelog(void) {
}

void syslog(int priority, const char *format, ...) {
    (void)priority;
    va_list arguments;
    va_start(arguments, format);
    fprintf(stderr, "%s: ", log_ident);
    vfprintf(stderr, format, arguments);
    fputc('\n', stderr);
    va_end(arguments);
}
