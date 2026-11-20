// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Force-included portability header for the OpenSSH add-on. It repairs libc header gaps without
// modifying upstream OpenSSH or the libc source. No SSH protocol or crypto code lives here.

#ifndef RAVEN_SSH_COMPAT_H
#define RAVEN_SSH_COMPAT_H

#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

/* ravenlibc fcntl.h undefines the SEEK_* constants after including bits/fcntl.h. Restore the
 * standard values so openbsd-compat/bsd-flock.c and the rest of OpenSSH compile. */
#ifndef SEEK_SET
#define SEEK_SET 0
#endif
#ifndef SEEK_CUR
#define SEEK_CUR 1
#endif
#ifndef SEEK_END
#define SEEK_END 2
#endif

/* packet.c/sftp-common.c use the OpenBSD util.h constant; ravenlibc exports fmt_scaled/scan_scaled
 * but not this macro. */
#ifndef FMT_SCALED_STRSIZE
#define FMT_SCALED_STRSIZE 7
#endif

/* ravenlibc paths.h omits the mail/default-PATH constants that session.c (linked only into
 * sshd-session) uses. Keep the values in this portability header; no upstream source changes. */
#ifndef _PATH_MAILDIR
#define _PATH_MAILDIR "/var/mail"
#endif
#ifndef _PATH_STDPATH
#define _PATH_STDPATH "/usr/bin:/bin:/usr/sbin:/sbin"
#endif

/* ravenlibc exports these symbols but some public headers do not declare them. */
void freezero(void *, size_t);
void arc4random_buf(void *, size_t);

#endif /* RAVEN_SSH_COMPAT_H */
