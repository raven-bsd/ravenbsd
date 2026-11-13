/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the FreeBSD-only __realpathat fast path is disabled; realpath1 runs.
 */
#ifndef _ORACLE_LIBSYS_H
#define _ORACLE_LIBSYS_H
static inline int __sys___realpathat(int dirfd, const char *path, char *buf,
    unsigned long size, int flags)
{
	(void)dirfd;
	(void)path;
	(void)buf;
	(void)size;
	(void)flags;
	return -1;
}
#endif
