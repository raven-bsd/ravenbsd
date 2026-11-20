// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Portability shim for the OpenSSL add-on. The current ravenlibc exports the POSIX calls that
// OpenSSL's optional CLI paths need (chmod, chown, fchown, link, symlink, utimes, mlock, munlock,
// madvise, msync, alarm, atof, pipe, select, setbuf, openlog, closelog, syslog), so the old stubs
// are removed to avoid duplicate symbols in the static link. Only futimes is still missing.
// Nothing here implements cryptography.

#include <errno.h>
#include <sys/time.h>

int futimes(int fd, const struct timeval times[2]) {
    (void)fd;
    (void)times;
    errno = ENOSYS;
    return -1;
}
