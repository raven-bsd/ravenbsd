/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD native scheme extension API. A POSIX function never parses a scheme:
 * `open("foo:bar")` is the file `foo:bar` in the cwd. A program that wants
 * `tty:`, `net:` or another native scheme calls raven_open_scheme with the
 * complete scheme path.
 */

#ifndef _RAVEN_SCHEME_H
#define _RAVEN_SCHEME_H

#include <sys/cdefs.h>

__BEGIN_DECLS

/*
 * Open a complete scheme path (`file:/etc/passwd`, `tty:console`, `net:tcp`)
 * with the Linux-valued O_* flags of <fcntl.h>. Returns a descriptor or -1
 * with errno set. The `file:` prefix is the caller's responsibility here; the
 * POSIX open family adds it itself after path resolution.
 */
int raven_open_scheme(const char *scheme_path, int flags, ...);

/*
 * Return a duplicate of the kernel handle behind a descriptor, or 0 with
 * errno = EBADF. The duplicate is owned by the caller and must be closed with
 * the native handle API, not close(2).
 */
unsigned raven_fd_to_handle(int fd);

/*
 * Install a kernel handle as a libc descriptor with the given kind
 * (0 file, 1 directory, 2 tty, 3 socket, 4 pipe, 255 unknown). Returns the
 * descriptor or -1 with errno set. The OFD takes ownership of the handle.
 */
int raven_fd_from_handle(unsigned handle, int kind);

__END_DECLS

#endif /* _RAVEN_SCHEME_H */
