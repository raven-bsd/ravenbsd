/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/pwd.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _PWD_H
#define _PWD_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t
#define __NEED_uid_t
#define __NEED_gid_t
#define __NEED_time_t

#if __BSD_VISIBLE || __XSI_VISIBLE
#define __NEED_FILE
#endif

#include <bits/alltypes.h>

/* S746: the public record is the FreeBSD 133344405 include/pwd.h struct passwd:
 * eleven fields including the change/class/expire words and pw_fields. */
struct passwd {
	char *pw_name;
	char *pw_passwd;
	uid_t pw_uid;
	gid_t pw_gid;
	time_t pw_change;
	char *pw_class;
	char *pw_gecos;
	char *pw_dir;
	char *pw_shell;
	time_t pw_expire;
	int pw_fields;
};

/* S746: the FreeBSD pw_fields bit map. */
#define _PWF(x)		(1 << (x))
#define _PWF_NAME	_PWF(0)
#define _PWF_PASSWD	_PWF(1)
#define _PWF_UID	_PWF(2)
#define _PWF_GID	_PWF(3)
#define _PWF_CHANGE	_PWF(4)
#define _PWF_CLASS	_PWF(5)
#define _PWF_GECOS	_PWF(6)
#define _PWF_DIR	_PWF(7)
#define _PWF_SHELL	_PWF(8)
#define _PWF_EXPIRE	_PWF(9)

#if __XSI_VISIBLE || __BSD_VISIBLE
void setpwent (void);
void endpwent (void);
struct passwd *getpwent (void);
#endif

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
struct passwd *getpwuid (uid_t);
struct passwd *getpwnam (const char *);
int getpwuid_r (uid_t, struct passwd *, char *, size_t, struct passwd **);
int getpwnam_r (const char *, struct passwd *, char *, size_t, struct passwd **);

#if __BSD_VISIBLE || __XSI_VISIBLE
struct passwd *fgetpwent(FILE *);
int putpwent(const struct passwd *, FILE *);
#endif

#if __BSD_VISIBLE || __XSI_VISIBLE
int setpassent(int);
#endif

#endif
__END_DECLS

#endif
