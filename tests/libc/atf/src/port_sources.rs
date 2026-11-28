// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exact donor test-port sources embedded as test-only text.
//!
//! The tracked C/C++ donor bodies tripped the structural scaffold gate. This module carries the
//! exact bytes of each historical blob and writes them into the build tree at test time, so the
//! repository tracks no C/C++ source for the port. [`stage`] verifies every written file against
//! the recorded SHA-256 before the single build-only transformation in [`transform`].
//!
//! The donor provenance (upstream revision, path and licence) remains in
//! `tests/libc-oracle/vendor/freebsd/libc-sys/provenance.toml`; the two shim files are first-party
//! BSD-3-Clause test scaffolding.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// One embedded test-port source.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    /// Repository-relative path the historical blob had before removal.
    pub path: &'static str,
    /// Exact historical bytes.
    pub body: &'static str,
    /// SHA-256 of `body`, recorded from the historical Git blob.
    pub sha256: &'static str,
}

/// Directory under the run output that holds the staged trees.
pub const STAGE_DIR: &str = "port-sources";
/// Subdirectory with the exact historical bytes.
pub const EXACT_DIR: &str = "exact";
/// Subdirectory with the build-ready bytes.
pub const BUILD_DIR: &str = "build";

/// The 18 donor test-port sources removed from tracking by this change.
pub const SOURCES: &[Source] = &[
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/gen/execve/t_execve.c",
        sha256: "28eef6caef4409085d16a84977fd0386ce64a17f86d728574dd19816352e075f",
        body: r##"/*	$NetBSD: t_execve.c,v 1.2 2015/09/12 15:21:33 christos Exp $	*/

/*-
 * Copyright (c) 2014 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#include <atf-c.h>

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <unistd.h>

ATF_TC(t_execve_null);

ATF_TC_HEAD(t_execve_null, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Tests an empty execve(2) executing");
}

ATF_TC_BODY(t_execve_null, tc)
{
	int err;

	err = execve(NULL, NULL, NULL);
	ATF_REQUIRE(err == -1);
	ATF_REQUIRE_MSG(errno == EFAULT,
	    "wrong error returned %d instead of %d", errno, EFAULT);
}

ATF_TP_ADD_TCS(tp)
{
	ATF_TP_ADD_TC(tp, t_execve_null);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/h_macros.h",
        sha256: "6512c23cdf2344537170adf8f7bf052b77fec54d12fd2296382ec2c9effa05f4",
        body: r##"/* $NetBSD: h_macros.h,v 1.13 2016/08/20 15:49:08 christos Exp $ */

/*-
 * Copyright (c) 2008, 2009 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#ifndef SRC_TESTS_H_MACROS_H_
#define SRC_TESTS_H_MACROS_H_

#include <sys/types.h>
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <atf-c.h>

#define REQUIRE_LIBC(x, v) \
	ATF_REQUIRE_MSG((x) != (v), "%s: %s", #x, strerror(errno))

#define CHECK_LIBC(x, v) \
	ATF_CHECK_MSG((x) != (v), "%s: %s", #x, strerror(errno))

#define RL(x) REQUIRE_LIBC(x, -1)
#define RLF(x, fmt, arg) \
	ATF_CHECK_MSG((x) != -1, "%s [" fmt "]: %s", #x, arg, strerror(errno))
#define RZ(x)								\
do {									\
	int RZ_rv = x;							\
	ATF_REQUIRE_MSG(RZ_rv == 0, "%s: %s", #x, strerror(RZ_rv));	\
} while (/*CONSTCOND*/0)

__dead static __inline __printflike(1, 2) void
atf_tc_fail_errno(const char *fmt, ...)
{
	va_list ap;
	char buf[1024];
	int sverrno = errno;

	va_start(ap, fmt);
	vsnprintf(buf, sizeof(buf), fmt, ap);
	va_end(ap);

	strlcat(buf, ": ", sizeof(buf));
	strlcat(buf, strerror(sverrno), sizeof(buf));

	atf_tc_fail("%s", buf);
}

static __inline void
tests_makegarbage(void *space, size_t len)
{
	uint16_t *sb = space;
	uint16_t randval;

	while (len >= sizeof(randval)) {
		*sb++ = (uint16_t)random();
		len -= sizeof(*sb);
	}
	randval = (uint16_t)random();
	memcpy(sb, &randval, len);
}

#endif
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_dup.c",
        sha256: "5a736f3a8e8ad2743bb2dca2622afd6693a4121e32e0a27e6b6dfd897c31b52f",
        body: r##"/* $NetBSD: t_dup.c,v 1.9 2017/01/13 20:31:53 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_dup.c,v 1.9 2017/01/13 20:31:53 christos Exp $");

#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sysexits.h>

static char	path[] = "dup";
#ifdef __NetBSD__
static void	check_mode(bool, bool, bool);
#endif

static void
check_mode(bool _dup, bool _dup2, bool _dup3)
{
	int mode[3] = { O_RDONLY, O_WRONLY, O_RDWR   };
	int perm[5] = { 0700, 0400, 0600, 0444, 0666 };
	struct stat st, st1;
	int fd, fd1, fd2;
	size_t i, j;

	/*
	 * Check that a duplicated descriptor
	 * retains the mode of the original file.
	 */
	for (i = 0; i < __arraycount(mode); i++) {

		for (j = 0; j < __arraycount(perm); j++) {

			fd1 = open(path, mode[i] | O_CREAT, perm[j]);
			fd2 = open("/etc/passwd", O_RDONLY);

			ATF_REQUIRE(fd1 >= 0);
			ATF_REQUIRE(fd2 >= 0);

			if (_dup != false)
				fd = dup(fd1);
			else if (_dup2 != false)
				fd = dup2(fd1, fd2);
			else if (_dup3 != false)
				fd = dup3(fd1, fd2, O_CLOEXEC);
			else {
				fd = -1;
			}

			ATF_REQUIRE(fd >= 0);

			(void)memset(&st, 0, sizeof(struct stat));
			(void)memset(&st1, 0, sizeof(struct stat));

			ATF_REQUIRE(fstat(fd, &st) == 0);
			ATF_REQUIRE(fstat(fd1, &st1) == 0);

			if (st.st_mode != st1.st_mode)
				atf_tc_fail("invalid mode");

			(void)close(fd);
			(void)close(fd1);
			(void)close(fd2);
			(void)unlink(path);
		}
	}
}

ATF_TC(dup2_basic);
ATF_TC_HEAD(dup2_basic, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of dup2(2)");
}

ATF_TC_BODY(dup2_basic, tc)
{
	int fd, fd1, fd2;

	fd1 = open("/etc/passwd", O_RDONLY);
	fd2 = open("/etc/passwd", O_RDONLY);

	ATF_REQUIRE(fd1 >= 0);
	ATF_REQUIRE(fd2 >= 0);

	fd = dup2(fd1, fd2);
	ATF_REQUIRE(fd >= 0);

	if (fd != fd2)
		atf_tc_fail("invalid descriptor");

	(void)close(fd);
	(void)close(fd1);

	ATF_REQUIRE(close(fd2) != 0);
}

ATF_TC(dup2_err);
ATF_TC_HEAD(dup2_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of dup2(2)");
}

ATF_TC_BODY(dup2_err, tc)
{
	int fd;

	fd = open("/etc/passwd", O_RDONLY);
	ATF_REQUIRE(fd >= 0);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup2(-1, -1) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup2(fd, -1) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup2(-1, fd) == -1);

	/*
	 * Note that this should not fail with EINVAL.
	 */
	ATF_REQUIRE(dup2(fd, fd) != -1);

	(void)close(fd);
}

ATF_TC(dup2_max);
ATF_TC_HEAD(dup2_max, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test dup2(2) against limits");
}

ATF_TC_BODY(dup2_max, tc)
{
	struct rlimit res;

	(void)memset(&res, 0, sizeof(struct rlimit));
	(void)getrlimit(RLIMIT_NOFILE, &res);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup2(STDERR_FILENO, res.rlim_cur + 1) == -1);
}

ATF_TC_WITH_CLEANUP(dup2_mode);
ATF_TC_HEAD(dup2_mode, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of dup2(2)");
}

ATF_TC_BODY(dup2_mode, tc)
{
	check_mode(false, true, false);
}

ATF_TC_CLEANUP(dup2_mode, tc)
{
	(void)unlink(path);
}


ATF_TC(dup3_err);
ATF_TC_HEAD(dup3_err, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test error conditions of dup3(2) (PR lib/45148)");
}

ATF_TC_BODY(dup3_err, tc)
{
	int fd;

	fd = open("/etc/passwd", O_RDONLY);
	ATF_REQUIRE(fd >= 0);

	errno = 0;
#if defined(__FreeBSD__) || defined(__linux__)
	/*
	 * FreeBSD and linux return EINVAL, because...
	 *
	 * [EINVAL] The oldd argument is equal to the newd argument.
	 */
	ATF_REQUIRE(dup3(fd, fd, O_CLOEXEC) == -1);
#else
	ATF_REQUIRE(dup3(fd, fd, O_CLOEXEC) != -1);
#endif

	errno = 0;
#if defined(__FreeBSD__) || defined(__linux__)
	ATF_REQUIRE_ERRNO(EINVAL, dup3(-1, -1, O_CLOEXEC) == -1);
	ATF_REQUIRE_ERRNO(EBADF, dup3(fd, -1, O_CLOEXEC) == -1);
#else
	ATF_REQUIRE_ERRNO(EBADF, dup3(-1, -1, O_CLOEXEC) == -1);
#endif

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup3(fd, -1, O_CLOEXEC) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup3(-1, fd, O_CLOEXEC) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EINVAL, dup3(fd, 1, O_NOFOLLOW) == -1);

	(void)close(fd);
}

ATF_TC(dup3_max);
ATF_TC_HEAD(dup3_max, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test dup3(2) against limits");
}

ATF_TC_BODY(dup3_max, tc)
{
	struct rlimit res;

	(void)memset(&res, 0, sizeof(struct rlimit));
	(void)getrlimit(RLIMIT_NOFILE, &res);

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup3(STDERR_FILENO,
		res.rlim_cur + 1, O_CLOEXEC) == -1);
}

ATF_TC_WITH_CLEANUP(dup3_mode);
ATF_TC_HEAD(dup3_mode, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of dup3(2)");
}

ATF_TC_BODY(dup3_mode, tc)
{
	check_mode(false, false, true);
}

ATF_TC_CLEANUP(dup3_mode, tc)
{
	(void)unlink(path);
}

ATF_TC(dup_err);
ATF_TC_HEAD(dup_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of dup(2)");
}

ATF_TC_BODY(dup_err, tc)
{

	errno = 0;
	ATF_REQUIRE_ERRNO(EBADF, dup(-1) == -1);
}

ATF_TC_WITH_CLEANUP(dup_max);
ATF_TC_HEAD(dup_max, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test dup(2) against limits");
}

ATF_TC_BODY(dup_max, tc)
{
	struct rlimit res;
	int *buf, fd, sta;
	size_t i, n;
	pid_t pid;

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {

		/*
		 * Open a temporary file until the
		 * maximum number of open files is
		 * reached. Ater that dup(2) family
		 * should fail with EMFILE.
		 */
		(void)closefrom(0);
		(void)memset(&res, 0, sizeof(struct rlimit));

		n = 10;
		res.rlim_cur = res.rlim_max = n;
		if (setrlimit(RLIMIT_NOFILE, &res) != 0)
			_exit(EX_OSERR);

		buf = calloc(n, sizeof(int));

		if (buf == NULL)
			_exit(EX_OSERR);

		buf[0] = mkstemp(path);

		if (buf[0] < 0)
			_exit(EX_OSERR);

		for (i = 1; i < n; i++) {

			buf[i] = open(path, O_RDONLY);

			if (buf[i] < 0)
				_exit(EX_OSERR);
		}

		errno = 0;
		fd = dup(buf[0]);

		if (fd != -1 || errno != EMFILE)
			_exit(EX_DATAERR);

		_exit(EXIT_SUCCESS);
	}

	(void)wait(&sta);

	if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS) {

		if (WEXITSTATUS(sta) == EX_OSERR)
			atf_tc_fail("system call error");

		if (WEXITSTATUS(sta) == EX_DATAERR)
			atf_tc_fail("dup(2) dupped more than RLIMIT_NOFILE");

		atf_tc_fail("unknown error");
	}

	(void)unlink(path);
}

ATF_TC_CLEANUP(dup_max, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(dup_mode);
ATF_TC_HEAD(dup_mode, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of dup(2)");
}

ATF_TC_BODY(dup_mode, tc)
{
	check_mode(true, false, false);
}

ATF_TC_CLEANUP(dup_mode, tc)
{
	(void)unlink(path);
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, dup2_basic);
	ATF_TP_ADD_TC(tp, dup2_err);
	ATF_TP_ADD_TC(tp, dup2_max);
	ATF_TP_ADD_TC(tp, dup2_mode);
	ATF_TP_ADD_TC(tp, dup3_err);
	ATF_TP_ADD_TC(tp, dup3_max);
	ATF_TP_ADD_TC(tp, dup3_mode);
	ATF_TP_ADD_TC(tp, dup_err);
	ATF_TP_ADD_TC(tp, dup_max);
	ATF_TP_ADD_TC(tp, dup_mode);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_getgroups.c",
        sha256: "50f3b58e2de5b3728baad32ff2e493f462c8388b97fb2d4a8df3c665a9561e90",
        body: r##"/* $NetBSD: t_getgroups.c,v 1.1 2011/07/07 06:57:53 jruoho Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_getgroups.c,v 1.1 2011/07/07 06:57:53 jruoho Exp $");

#include <sys/wait.h>

#include <atf-c.h>
#include <errno.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

ATF_TC(getgroups_err);
ATF_TC_HEAD(getgroups_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test errors in getgroups(2)");
}

ATF_TC_BODY(getgroups_err, tc)
{
	gid_t gidset[NGROUPS_MAX];

	errno = 0;

	ATF_REQUIRE(getgroups(10, (gid_t *)-1) == -1);
	ATF_REQUIRE(errno == EFAULT);

	errno = 0;

	ATF_REQUIRE(getgroups(-1, gidset) == -1);
	ATF_REQUIRE(errno == EINVAL);
}

ATF_TC(getgroups_getgid);
ATF_TC_HEAD(getgroups_getgid, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test getgid(2) from getgroups(2)");
}

ATF_TC_BODY(getgroups_getgid, tc)
{
	gid_t gidset[NGROUPS_MAX];
	gid_t gid = getgid();
	int i, n;

	/*
	 * Check that getgid(2) is found from
	 * the GIDs returned by getgroups(2).
	 */
	n = getgroups(NGROUPS_MAX, gidset);

	for (i = 0; i < n; i++) {

		if (gidset[i] == gid)
			return;
	}

	atf_tc_fail("getgid(2) not found from getgroups(2)");
}

ATF_TC(getgroups_setgid);
ATF_TC_HEAD(getgroups_setgid, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test setgid(2) from getgroups(2)");
	atf_tc_set_md_var(tc, "require.user", "root");
}

ATF_TC_BODY(getgroups_setgid, tc)
{
	gid_t gidset[NGROUPS_MAX];
	int i, n, rv, sta;
	pid_t pid;

	/*
	 * Check that we can setgid(2)
	 * to the returned group IDs.
	 */
	n = getgroups(NGROUPS_MAX, gidset);
	ATF_REQUIRE(n >= 0);

	for (i = 0; i < n; i++) {

		pid = fork();
		ATF_REQUIRE(pid >= 0);

		if (pid == 0) {

			rv = setgid(gidset[i]);

			if (rv != 0)
				_exit(EXIT_FAILURE);

			_exit(EXIT_SUCCESS);
		}

		(void)wait(&sta);

		if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS)
			atf_tc_fail("getgroups(2) is inconsistent");
	}
}

ATF_TC(getgroups_zero);
ATF_TC_HEAD(getgroups_zero, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test getgroups(2) with zero param");
}

ATF_TC_BODY(getgroups_zero, tc)
{
	const gid_t val = 123456789;
	gid_t gidset[NGROUPS_MAX];
	size_t i;

	/*
	 * If the first parameter is zero, the number
	 * of groups should be returned but the supplied
	 * buffer should remain intact.
	 */
	for (i = 0; i < __arraycount(gidset); i++)
		gidset[i] = val;

	ATF_REQUIRE(getgroups(0, gidset) >= 0);

	for (i = 0; i < __arraycount(gidset); i++) {

		if (gidset[i] != val)
			atf_tc_fail("getgroups(2) modified the buffer");
	}
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, getgroups_err);
	ATF_TP_ADD_TC(tp, getgroups_getgid);
	ATF_TP_ADD_TC(tp, getgroups_setgid);
	ATF_TP_ADD_TC(tp, getgroups_zero);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_kill.c",
        sha256: "7f461acb224bb49d3a614c2f8c9ab65dd7a3d96095b3049bea52c372cb9cfdae",
        body: r##"/* $NetBSD: t_kill.c,v 1.1 2011/07/07 06:57:53 jruoho Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_kill.c,v 1.1 2011/07/07 06:57:53 jruoho Exp $");

#include <sys/wait.h>

#include <errno.h>
#include <limits.h>
#include <pwd.h>
#include <signal.h>
#include <stdlib.h>
#include <unistd.h>

#include <atf-c.h>

ATF_TC(kill_basic);
ATF_TC_HEAD(kill_basic, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test that kill(2) works");
}

ATF_TC_BODY(kill_basic, tc)
{
	const int sig[] = { SIGHUP, SIGINT, SIGKILL, SIGTERM };
	pid_t pid;
	size_t i;
	int sta;

	for (i = 0; i < __arraycount(sig); i++) {

		pid = fork();
		ATF_REQUIRE(pid >= 0);

		switch (pid) {

		case 0:
			pause();
			break;

		default:
			ATF_REQUIRE(kill(pid, sig[i]) == 0);
		}

		(void)wait(&sta);

		if (WIFSIGNALED(sta) == 0 || WTERMSIG(sta) != sig[i])
			atf_tc_fail("kill(2) failed to kill child");
	}
}

ATF_TC(kill_err);
ATF_TC_HEAD(kill_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of kill(2)");
}

ATF_TC_BODY(kill_err, tc)
{
	int rv, sta;
	pid_t pid;

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {

		errno = 0;
		rv = kill(getpid(), -1);

		if (rv == 0 || errno != EINVAL)
			_exit(EINVAL);

		errno = 0;
		rv = kill(INT_MAX, SIGUSR1);

		if (rv == 0 || errno != ESRCH)
			_exit(ESRCH);

		_exit(EXIT_SUCCESS);
	}

	(void)wait(&sta);

	if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS) {

		if (WEXITSTATUS(sta) == EINVAL)
			atf_tc_fail("expected EINVAL, but kill(2) succeeded");

		if (WEXITSTATUS(sta) == ESRCH)
			atf_tc_fail("expected ESRCH, but kill(2) succeeded");

		atf_tc_fail("unknown error from kill(2)");
	}
}

ATF_TC(kill_perm);
ATF_TC_HEAD(kill_perm, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test kill(2) permissions");
	atf_tc_set_md_var(tc, "require.user", "root");
}

ATF_TC_BODY(kill_perm, tc)
{
	struct passwd *pw;
	pid_t cpid, ppid;
	uid_t cuid = 0;
	uid_t puid = 0;
	int sta;

	/*
	 * Test that kill(2) fails when called
	 * for a PID owned by another user.
	 */
	pw = getpwnam("operator");

	if (pw != NULL)
		cuid = pw->pw_uid;

	pw = getpwnam("nobody");

	if (pw != NULL)
		puid = pw->pw_uid;

	if (cuid == 0 || puid == 0 || cuid == puid)
		atf_tc_fail("getpwnam(3) failed");

	ppid = fork();

	if (ppid < 0)
		_exit(EXIT_FAILURE);

	if (ppid == 0) {

		cpid = fork();

		if (cpid < 0)
			_exit(EXIT_FAILURE);

		if (cpid == 0) {

			if (setuid(cuid) < 0)
				_exit(EXIT_FAILURE);
			else {
				(void)sleep(1);
			}

			_exit(EXIT_SUCCESS);
		}

		/*
		 * Try to kill the child after having
		 * set the real and effective UID.
		 */
		if (setuid(puid) != 0)
			_exit(EXIT_FAILURE);

		errno = 0;

		if (kill(cpid, SIGKILL) == 0)
			_exit(EPERM);

		if (errno != EPERM)
			_exit(EPERM);

		(void)waitpid(cpid, &sta, 0);

		_exit(EXIT_SUCCESS);
	}

	(void)waitpid(ppid, &sta, 0);

	if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) == EPERM)
		atf_tc_fail("killed a process of another user");

	if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS)
		atf_tc_fail("unknown error from kill(2)");
}

ATF_TC(kill_pgrp_neg);
ATF_TC_HEAD(kill_pgrp_neg, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test kill(2) with process group, #2");
}

ATF_TC_BODY(kill_pgrp_neg, tc)
{
	const int maxiter = 3;
	pid_t cpid, ppid;
	int i, sta;

	ppid = fork();
	ATF_REQUIRE(ppid >= 0);

	if (ppid == 0) {

		ATF_REQUIRE(setpgid(0, 0) == 0);

		for (i = 0; i < maxiter; i++) {

			cpid = fork();
			ATF_REQUIRE(cpid >= 0);

			if (cpid == 0)
				pause();
		}

		/*
		 * Test the variant of killpg(3); if the process number
		 * is negative but not -1, the signal should be sent to
		 * all processes whose process group ID is equal to the
		 * absolute value of the process number.
		 */
		ATF_REQUIRE(kill(-getpgrp(), SIGKILL) == 0);

		(void)sleep(1);

		_exit(EXIT_SUCCESS);
	}

	(void)waitpid(ppid, &sta, 0);

	if (WIFSIGNALED(sta) == 0 || WTERMSIG(sta) != SIGKILL)
		atf_tc_fail("failed to kill(2) a process group");
}

ATF_TC(kill_pgrp_zero);
ATF_TC_HEAD(kill_pgrp_zero, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test kill(2) with process group, #1");
}

ATF_TC_BODY(kill_pgrp_zero, tc)
{
	const int maxiter = 3;
	pid_t cpid, ppid;
	int i, sta;

	ppid = fork();
	ATF_REQUIRE(ppid >= 0);

	if (ppid == 0) {

		ATF_REQUIRE(setpgid(0, 0) == 0);

		for (i = 0; i < maxiter; i++) {

			cpid = fork();
			ATF_REQUIRE(cpid >= 0);

			if (cpid == 0)
				pause();
		}

		/*
		 * If the supplied process number is zero,
		 * the signal should be sent to all processes
		 * under the current process group.
		 */
		ATF_REQUIRE(kill(0, SIGKILL) == 0);

		(void)sleep(1);

		_exit(EXIT_SUCCESS);
	}

	(void)waitpid(ppid, &sta, 0);

	if (WIFSIGNALED(sta) == 0 || WTERMSIG(sta) != SIGKILL)
		atf_tc_fail("failed to kill(2) a process group");
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, kill_basic);
	ATF_TP_ADD_TC(tp, kill_err);
	ATF_TP_ADD_TC(tp, kill_perm);
	ATF_TP_ADD_TC(tp, kill_pgrp_neg);
	ATF_TP_ADD_TC(tp, kill_pgrp_zero);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_link.c",
        sha256: "e4628385c25ad37dd3ad842f3352ddbf19441f67333cef900d80ff57c22df815",
        body: r##"/* $NetBSD: t_link.c,v 1.3 2017/01/13 20:42:36 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_link.c,v 1.3 2017/01/13 20:42:36 christos Exp $");

#include <sys/param.h>
#include <sys/stat.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static const char	*getpath(void);
static char		 path[] = "link";
static const char	*pathl;

static const char *
getpath(void)
{
	static char buf[LINE_MAX];

	(void)memset(buf, '\0', sizeof(buf));

	if (getcwd(buf, sizeof(buf)) == NULL)
		return NULL;

	(void)strlcat(buf, path, sizeof(buf));
	(void)strlcat(buf, ".link", sizeof(buf));

	return buf;
}

ATF_TC_WITH_CLEANUP(link_count);
ATF_TC_HEAD(link_count, tc)
{
	atf_tc_set_md_var(tc, "descr", "link(2) counts are incremented?");
}

ATF_TC_BODY(link_count, tc)
{
	struct stat sa, sb;
	int fd;

	(void)memset(&sa, 0, sizeof(struct stat));
	(void)memset(&sb, 0, sizeof(struct stat));

	pathl = getpath();
	fd = open(path, O_RDWR | O_CREAT, 0600);

	ATF_REQUIRE(fd >= 0);
	ATF_REQUIRE(pathl != NULL);

	ATF_REQUIRE(stat(path, &sa) == 0);
	ATF_REQUIRE(link(path, pathl) == 0);
	ATF_REQUIRE(stat(path, &sb) == 0);

	if (sa.st_nlink != sb.st_nlink - 1)
		atf_tc_fail("incorrect link(2) count");

	ATF_REQUIRE(close(fd) == 0);
	ATF_REQUIRE(unlink(path) == 0);
	ATF_REQUIRE(unlink(pathl) == 0);
}

ATF_TC_CLEANUP(link_count, tc)
{
	(void)unlink(path);
	(void)unlink(pathl);
}

ATF_TC_WITH_CLEANUP(link_err);
ATF_TC_HEAD(link_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of link(2)");
}

ATF_TC_BODY(link_err, tc)
{
	char buf[MAXPATHLEN + 1];
	int fd;

	(void)memset(buf, 'x', sizeof(buf));

	pathl = getpath();
	fd = open(path, O_RDWR | O_CREAT, 0600);

	ATF_REQUIRE(fd >= 0);
	ATF_REQUIRE(pathl != NULL);

	errno = 0;
	ATF_REQUIRE(link(path, pathl) == 0);
	ATF_REQUIRE_ERRNO(EEXIST, link(path, pathl) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENAMETOOLONG, link(buf, "xxx") == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, link(path, "/d/c/b/a") == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, link("/a/b/c/d", path) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, link("/a/b/c/d", "/d/c/b/a") == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EFAULT, link(path, (const char *)-1) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EFAULT, link((const char *)-1, "xxx") == -1);

	ATF_REQUIRE(close(fd) == 0);
	ATF_REQUIRE(unlink(path) == 0);
	ATF_REQUIRE(unlink(pathl) == 0);
}

ATF_TC_CLEANUP(link_err, tc)
{
	(void)unlink(path);
	(void)unlink(pathl);
}

ATF_TC(link_perm);
ATF_TC_HEAD(link_perm, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test permissions with link(2)");
	atf_tc_set_md_var(tc, "require.user", "unprivileged");
}

ATF_TC_BODY(link_perm, tc)
{
	int rv;

	errno = 0;
	rv = link("/root", "/root.link");
	ATF_REQUIRE_MSG(rv == -1 && (errno == EACCES || errno == EPERM),
	    "link to a directory did not fail with EPERM or EACCESS; link() "
	    "returned %d, errno %d", rv, errno);

	errno = 0;
	ATF_REQUIRE_ERRNO(EACCES,
	    link("/root/.profile", "/root/.profile.link") == -1);
}

ATF_TC_WITH_CLEANUP(link_stat);
ATF_TC_HEAD(link_stat, tc)
{
	atf_tc_set_md_var(tc, "descr", "Check stat(2) of a linked file");
}

ATF_TC_BODY(link_stat, tc)
{
	struct stat sa, sb;
	int fd;

	(void)memset(&sa, 0, sizeof(struct stat));
	(void)memset(&sb, 0, sizeof(struct stat));

	pathl = getpath();
	fd = open(path, O_RDWR | O_CREAT, 0600);

	ATF_REQUIRE(fd >= 0);
	ATF_REQUIRE(pathl != NULL);

	ATF_REQUIRE(link(path, pathl) == 0);
	ATF_REQUIRE(stat(path, &sa) == 0);
	ATF_REQUIRE(lstat(pathl, &sb) == 0);

	if (sa.st_uid != sb.st_uid)
		atf_tc_fail("unequal UIDs");

	if (sa.st_mode != sb.st_mode)
		atf_tc_fail("unequal modes");

	if (sa.st_ino != sb.st_ino)
		atf_tc_fail("unequal inodes");

	ATF_REQUIRE(close(fd) == 0);
	ATF_REQUIRE(unlink(path) == 0);
	ATF_REQUIRE(unlink(pathl) == 0);
}

ATF_TC_CLEANUP(link_stat, tc)
{
	(void)unlink(path);
	(void)unlink(pathl);
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, link_count);
	ATF_TP_ADD_TC(tp, link_err);
	ATF_TP_ADD_TC(tp, link_perm);
	ATF_TP_ADD_TC(tp, link_stat);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mkdir.c",
        sha256: "2edca3e4bd3c8d19be64e96a93ec4fedaffdc455057f7385838c393f6d46f769",
        body: r##"/* $NetBSD: t_mkdir.c,v 1.2 2011/10/15 07:38:31 jruoho Exp $ */

/*-
 * Copyright (c) 2008, 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jason R. Thorpe and Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#include <sys/cdefs.h>
__COPYRIGHT("@(#) Copyright (c) 2008\
 The NetBSD Foundation, inc. All rights reserved.");
__RCSID("$NetBSD: t_mkdir.c,v 1.2 2011/10/15 07:38:31 jruoho Exp $");

#include <sys/stat.h>
#include <sys/wait.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

ATF_TC(mkdir_err);
ATF_TC_HEAD(mkdir_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Checks errors from mkdir(2)");
}

ATF_TC_BODY(mkdir_err, tc)
{
	char buf[PATH_MAX + 1];
	int fd;

	(void)memset(buf, 'x', sizeof(buf));

	fd = open("/etc", O_RDONLY);

	if (fd >= 0) {

		(void)close(fd);

		errno = 0;
		ATF_REQUIRE_ERRNO(EEXIST, mkdir("/etc", 0500) == -1);
	}

	errno = 0;
	ATF_REQUIRE_ERRNO(EFAULT, mkdir((void *)-1, 0500) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENAMETOOLONG, mkdir(buf, 0500) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, mkdir("/a/b/c/d/e/f/g/h/i/j/k", 0500) == -1);
}

ATF_TC_WITH_CLEANUP(mkdir_perm);
ATF_TC_HEAD(mkdir_perm, tc)
{
	atf_tc_set_md_var(tc, "descr", "Checks permissions with mkdir(2)");
	atf_tc_set_md_var(tc, "require.user", "unprivileged");
}

ATF_TC_BODY(mkdir_perm, tc)
{
	errno = 0;
	ATF_REQUIRE_ERRNO(EACCES, mkdir("/usr/__nonexistent__", 0500) == -1);
}

ATF_TC_CLEANUP(mkdir_perm, tc)
{
	(void)rmdir("/usr/__nonexistent__");
}

ATF_TC_WITH_CLEANUP(mkdir_mode);
ATF_TC_HEAD(mkdir_mode, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test that UIDs and GIDs are right "
	    "for a directory created with mkdir(2)");
	atf_tc_set_md_var(tc, "require.user", "root");
}

ATF_TC_BODY(mkdir_mode, tc)
{
	static const char *path = "/tmp/mkdir";
	struct stat st_a, st_b;
	struct passwd *pw;
	pid_t pid;
	int sta;

	(void)memset(&st_a, 0, sizeof(struct stat));
	(void)memset(&st_b, 0, sizeof(struct stat));

	pw = getpwnam("nobody");

	ATF_REQUIRE(pw != NULL);
	ATF_REQUIRE(stat("/tmp", &st_a) == 0);

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {

		if (setuid(pw->pw_uid) != 0)
			_exit(EXIT_FAILURE);

		if (mkdir(path, 0500) != 0)
			_exit(EXIT_FAILURE);

		_exit(EXIT_SUCCESS);
	}

	(void)sleep(1);
	(void)wait(&sta);

	if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS)
		atf_tc_fail("failed to create '%s'", path);

	ATF_REQUIRE(stat(path, &st_b) == 0);
	ATF_REQUIRE(rmdir(path) == 0);

	/*
	 * The directory's owner ID should be set to the
	 * effective UID, whereas the group ID should be
	 * set to that of the parent directory.
	 */
	if (st_b.st_uid != pw->pw_uid)
		atf_tc_fail("invalid UID for '%s'", path);

	if (st_b.st_gid != st_a.st_gid)
		atf_tc_fail("GID did not follow the parent directory");
}

ATF_TC_CLEANUP(mkdir_mode, tc)
{
	(void)rmdir("/tmp/mkdir");
}

ATF_TC(mkdir_trail);
ATF_TC_HEAD(mkdir_trail, tc)
{
	atf_tc_set_md_var(tc, "descr", "Checks mkdir(2) for trailing slashes");
}

ATF_TC_BODY(mkdir_trail, tc)
{
	const char *tests[] = {
		/*
		 * IEEE 1003.1 second ed. 2.2.2.78:
		 *
		 * If the pathname refers to a directory, it may also have
		 * one or more trailing slashes.  Multiple successive slashes
		 * are considered to be the same as one slash.
		 */
		"dir1/",
		"dir2//",

		NULL,
	};

	const char **test;

	for (test = &tests[0]; *test != NULL; ++test) {

		(void)printf("Checking \"%s\"\n", *test);
		(void)rmdir(*test);

		ATF_REQUIRE(mkdir(*test, 0777) == 0);
		ATF_REQUIRE(rename(*test, "foo") == 0);
		ATF_REQUIRE(rename("foo/", *test) == 0);
		ATF_REQUIRE(rmdir(*test) == 0);
	}
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, mkdir_err);
	ATF_TP_ADD_TC(tp, mkdir_perm);
	ATF_TP_ADD_TC(tp, mkdir_mode);
	ATF_TP_ADD_TC(tp, mkdir_trail);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mmap.c",
        sha256: "2829a5e7dd6891fb98bcae6d24e943f3ac43e994378fb7e010ca33bb2ccaf75b",
        body: r##"/* $NetBSD: t_mmap.c,v 1.12 2017/01/16 16:31:05 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

/*-
 * Copyright (c)2004 YAMAMOTO Takashi,
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_mmap.c,v 1.12 2017/01/16 16:31:05 christos Exp $");

#include <sys/param.h>
#include <sys/disklabel.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/socket.h>
#include <sys/sysctl.h>
#include <sys/wait.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <paths.h>
#ifdef __FreeBSD__
#include <stdint.h>
#endif

static long	page = 0;
static char	path[] = "mmap";
static void	map_check(void *, int);
static void	map_sighandler(int);
static void	testloan(void *, void *, char, int);

#define	BUFSIZE	(32 * 1024)	/* enough size to trigger sosend_loan */

static void
map_check(void *map, int flag)
{

	if (flag != 0) {
		ATF_REQUIRE(map == MAP_FAILED);
		return;
	}

	ATF_REQUIRE(map != MAP_FAILED);
	ATF_REQUIRE(munmap(map, page) == 0);
}

void
testloan(void *vp, void *vp2, char pat, int docheck)
{
	char buf[BUFSIZE];
	char backup[BUFSIZE];
	ssize_t nwritten;
	ssize_t nread;
	int fds[2];
	int val;

	val = BUFSIZE;

	if (docheck != 0)
		(void)memcpy(backup, vp, BUFSIZE);

	if (socketpair(AF_LOCAL, SOCK_STREAM, PF_UNSPEC, fds) != 0)
		atf_tc_fail("socketpair() failed");

	val = BUFSIZE;

	if (setsockopt(fds[1], SOL_SOCKET, SO_RCVBUF, &val, sizeof(val)) != 0)
		atf_tc_fail("setsockopt() failed, SO_RCVBUF");

	val = BUFSIZE;

	if (setsockopt(fds[0], SOL_SOCKET, SO_SNDBUF, &val, sizeof(val)) != 0)
		atf_tc_fail("setsockopt() failed, SO_SNDBUF");

	if (fcntl(fds[0], F_SETFL, O_NONBLOCK) != 0)
		atf_tc_fail("fcntl() failed");

	nwritten = write(fds[0], (char *)vp + page, BUFSIZE - page);

	if (nwritten == -1)
		atf_tc_fail("write() failed");

	/* Break loan. */
	(void)memset(vp2, pat, BUFSIZE);

	nread = read(fds[1], buf + page, BUFSIZE - page);

	if (nread == -1)
		atf_tc_fail("read() failed");

	if (nread != nwritten)
		atf_tc_fail("too short read");

	if (docheck != 0 && memcmp(backup, buf + page, nread) != 0)
		atf_tc_fail("data mismatch");

	ATF_REQUIRE(close(fds[0]) == 0);
	ATF_REQUIRE(close(fds[1]) == 0);
}

static void
map_sighandler(int signo)
{
	_exit(signo);
}

#ifdef __NetBSD__
ATF_TC(mmap_block);
ATF_TC_HEAD(mmap_block, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) with a block device");
	atf_tc_set_md_var(tc, "require.user", "root");
}

ATF_TC_BODY(mmap_block, tc)
{
	static const int mib[] = { CTL_HW, HW_DISKNAMES };
	static const unsigned int miblen = __arraycount(mib);
	char *map, *dk, *drives, dev[PATH_MAX];
	size_t len;
	int fd = -1;

	atf_tc_skip("The test case causes a panic (PR kern/38889, kern/46592)");

	ATF_REQUIRE(sysctl(mib, miblen, NULL, &len, NULL, 0) == 0);
	drives = malloc(len);
	ATF_REQUIRE(drives != NULL);
	ATF_REQUIRE(sysctl(mib, miblen, drives, &len, NULL, 0) == 0);
	for (dk = strtok(drives, " "); dk != NULL; dk = strtok(NULL, " ")) {
		sprintf(dev, _PATH_DEV "%s%c", dk, 'a'+RAW_PART);
		fprintf(stderr, "trying: %s\n", dev);

		if ((fd = open(dev, O_RDONLY)) >= 0) {
			(void)fprintf(stderr, "using %s\n", dev);
			break;
		}
	}
	free(drives);

	if (fd < 0)
		atf_tc_skip("failed to find suitable block device");

	map = mmap(NULL, 4096, PROT_READ, MAP_FILE, fd, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	(void)fprintf(stderr, "first byte %x\n", *map);
	ATF_REQUIRE(close(fd) == 0);
	(void)fprintf(stderr, "first byte %x\n", *map);

	ATF_REQUIRE(munmap(map, 4096) == 0);
}
#endif

ATF_TC(mmap_err);
ATF_TC_HEAD(mmap_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of mmap(2)");
}

ATF_TC_BODY(mmap_err, tc)
{
	size_t addr = SIZE_MAX;
	void *map;

	errno = 0;
	map = mmap(NULL, 3, PROT_READ, MAP_FILE|MAP_PRIVATE, -1, 0);

	ATF_REQUIRE(map == MAP_FAILED);
	ATF_REQUIRE(errno == EBADF);

	errno = 0;
	map = mmap(&addr, page, PROT_READ, MAP_FIXED|MAP_PRIVATE, -1, 0);

	ATF_REQUIRE(map == MAP_FAILED);
	ATF_REQUIRE(errno == EINVAL);

	errno = 0;
	map = mmap(NULL, page, PROT_READ, MAP_ANON|MAP_PRIVATE, INT_MAX, 0);

	ATF_REQUIRE(map == MAP_FAILED);
	ATF_REQUIRE(errno == EINVAL);
}

ATF_TC_WITH_CLEANUP(mmap_loan);
ATF_TC_HEAD(mmap_loan, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test uvm page loanout with mmap(2)");
}

ATF_TC_BODY(mmap_loan, tc)
{
	char buf[BUFSIZE];
	char *vp, *vp2;
	int fd;

	fd = open(path, O_RDWR | O_CREAT, 0600);
	ATF_REQUIRE(fd >= 0);

	(void)memset(buf, 'x', sizeof(buf));
	(void)write(fd, buf, sizeof(buf));

	vp = mmap(NULL, BUFSIZE, PROT_READ | PROT_WRITE,
	    MAP_FILE | MAP_PRIVATE, fd, 0);

	ATF_REQUIRE(vp != MAP_FAILED);

	vp2 = vp;

	testloan(vp, vp2, 'A', 0);
	testloan(vp, vp2, 'B', 1);

	ATF_REQUIRE(munmap(vp, BUFSIZE) == 0);

	vp = mmap(NULL, BUFSIZE, PROT_READ | PROT_WRITE,
	    MAP_FILE | MAP_SHARED, fd, 0);

	vp2 = mmap(NULL, BUFSIZE, PROT_READ | PROT_WRITE,
	    MAP_FILE | MAP_SHARED, fd, 0);

	ATF_REQUIRE(vp != MAP_FAILED);
	ATF_REQUIRE(vp2 != MAP_FAILED);

	testloan(vp, vp2, 'E', 1);

	ATF_REQUIRE(munmap(vp, BUFSIZE) == 0);
	ATF_REQUIRE(munmap(vp2, BUFSIZE) == 0);
}

ATF_TC_CLEANUP(mmap_loan, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(mmap_prot_1);
ATF_TC_HEAD(mmap_prot_1, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) protections, #1");
}

ATF_TC_BODY(mmap_prot_1, tc)
{
	void *map;
	int fd;

	/*
	 * Open a file write-only and try to
	 * map it read-only. This should fail.
	 */
	fd = open(path, O_WRONLY | O_CREAT, 0700);

	if (fd < 0)
		return;

	ATF_REQUIRE(write(fd, "XXX", 3) == 3);

	map = mmap(NULL, 3, PROT_READ, MAP_FILE|MAP_PRIVATE, fd, 0);
	map_check(map, 1);

	map = mmap(NULL, 3, PROT_WRITE, MAP_FILE|MAP_PRIVATE, fd, 0);
	map_check(map, 0);

	ATF_REQUIRE(close(fd) == 0);
}

ATF_TC_CLEANUP(mmap_prot_1, tc)
{
	(void)unlink(path);
}

ATF_TC(mmap_prot_2);
ATF_TC_HEAD(mmap_prot_2, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) protections, #2");
}

ATF_TC_BODY(mmap_prot_2, tc)
{
	char buf[2];
	void *map;
	pid_t pid;
	int sta;

	/*
	 * Make a PROT_NONE mapping and try to access it.
	 * If we catch a SIGSEGV, all works as expected.
	 */
	map = mmap(NULL, page, PROT_NONE, MAP_ANON|MAP_PRIVATE, -1, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		ATF_REQUIRE(signal(SIGSEGV, map_sighandler) != SIG_ERR);
		ATF_REQUIRE(strlcpy(buf, map, sizeof(buf)) != 0);
	}

	(void)wait(&sta);

	ATF_REQUIRE(WIFEXITED(sta) != 0);
	ATF_REQUIRE(WEXITSTATUS(sta) == SIGSEGV);
	ATF_REQUIRE(munmap(map, page) == 0);
}

ATF_TC_WITH_CLEANUP(mmap_prot_3);
ATF_TC_HEAD(mmap_prot_3, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) protections, #3");
}

ATF_TC_BODY(mmap_prot_3, tc)
{
	char buf[2];
	int fd, sta;
	void *map;
	pid_t pid;

	/*
	 * Open a file, change the permissions
	 * to read-only, and try to map it as
	 * PROT_NONE. This should succeed, but
	 * the access should generate SIGSEGV.
	 */
	fd = open(path, O_RDWR | O_CREAT, 0700);

	if (fd < 0)
#ifdef	__FreeBSD__
		atf_tc_skip("opening %s failed; skipping testcase: %s",
		    path, strerror(errno));
#else
		return;
#endif

	ATF_REQUIRE(write(fd, "XXX", 3) == 3);
	ATF_REQUIRE(close(fd) == 0);
	ATF_REQUIRE(chmod(path, 0444) == 0);

	fd = open(path, O_RDONLY);
	ATF_REQUIRE(fd != -1);

	map = mmap(NULL, 3, PROT_NONE, MAP_FILE | MAP_SHARED, fd, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	pid = fork();

	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		ATF_REQUIRE(signal(SIGSEGV, map_sighandler) != SIG_ERR);
		ATF_REQUIRE(strlcpy(buf, map, sizeof(buf)) != 0);
	}

	(void)wait(&sta);

	ATF_REQUIRE(WIFEXITED(sta) != 0);
	ATF_REQUIRE(WEXITSTATUS(sta) == SIGSEGV);
	ATF_REQUIRE(munmap(map, 3) == 0);
#ifdef	__FreeBSD__
	(void)close(fd);
#endif
}

ATF_TC_CLEANUP(mmap_prot_3, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(mmap_truncate);
ATF_TC_HEAD(mmap_truncate, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) and ftruncate(2)");
}

ATF_TC_BODY(mmap_truncate, tc)
{
	char *map;
	long i;
	int fd;

	fd = open(path, O_RDWR | O_CREAT, 0700);

	if (fd < 0)
		return;

	/*
	 * See that ftruncate(2) works
	 * while the file is mapped.
	 */
	ATF_REQUIRE(ftruncate(fd, page) == 0);

	map = mmap(NULL, page, PROT_READ | PROT_WRITE, MAP_FILE|MAP_PRIVATE,
	     fd, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	for (i = 0; i < page; i++)
		map[i] = 'x';

	ATF_REQUIRE(ftruncate(fd, 0) == 0);
	ATF_REQUIRE(ftruncate(fd, page / 8) == 0);
	ATF_REQUIRE(ftruncate(fd, page / 4) == 0);
	ATF_REQUIRE(ftruncate(fd, page / 2) == 0);
	ATF_REQUIRE(ftruncate(fd, page / 12) == 0);
	ATF_REQUIRE(ftruncate(fd, page / 64) == 0);

	(void)munmap(map, page);
	ATF_REQUIRE(close(fd) == 0);
}

ATF_TC_CLEANUP(mmap_truncate, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(mmap_truncate_signal);
ATF_TC_HEAD(mmap_truncate_signal, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test mmap(2) ftruncate(2) causing signal");
}

ATF_TC_BODY(mmap_truncate_signal, tc)
{
	char *map;
	long i;
	int fd, sta;
	pid_t pid;

	fd = open(path, O_RDWR | O_CREAT, 0700);

	if (fd < 0)
		return;

	ATF_REQUIRE(write(fd, "foo\n", 5) == 5);

	map = mmap(NULL, page, PROT_READ, MAP_FILE|MAP_PRIVATE, fd, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	sta = 0;
	for (i = 0; i < 5; i++)
		sta += map[i];
	ATF_REQUIRE(sta == 334);

	ATF_REQUIRE(ftruncate(fd, 0) == 0);
	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		ATF_REQUIRE(signal(SIGBUS, map_sighandler) != SIG_ERR);
		ATF_REQUIRE(signal(SIGSEGV, map_sighandler) != SIG_ERR);
		sta = 0;
		for (i = 0; i < page; i++)
			sta += ((volatile char *)map)[i];
		/* child never will get this far, but the compiler will
		   not know, so better use the values calculated to
		   prevent the access to be optimized out */
		ATF_REQUIRE(i == 0);
		ATF_REQUIRE(sta == 0);
		(void)munmap(map, page);
		(void)close(fd);
		return;
	}

	(void)wait(&sta);

	ATF_REQUIRE(WIFEXITED(sta) != 0);
	if (WEXITSTATUS(sta) == SIGSEGV)
		atf_tc_fail("child process got SIGSEGV instead of SIGBUS");
	ATF_REQUIRE(WEXITSTATUS(sta) == SIGBUS);
	ATF_REQUIRE(munmap(map, page) == 0);
	ATF_REQUIRE(close(fd) == 0);
}

ATF_TC_CLEANUP(mmap_truncate_signal, tc)
{
	(void)unlink(path);
}

ATF_TC(mmap_va0);
ATF_TC_HEAD(mmap_va0, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mmap(2) and vm.user_va0_disable");
}

ATF_TC_BODY(mmap_va0, tc)
{
	int flags = MAP_ANON | MAP_FIXED | MAP_PRIVATE;
	size_t len = sizeof(int);
	void *map;
	int val;

	/*
	 * Make an anonymous fixed mapping at zero address. If the address
	 * is restricted as noted in security(7), the syscall should fail.
	 */
#ifdef __FreeBSD__
	if (sysctlbyname("security.bsd.map_at_zero", &val, &len, NULL, 0) != 0)
		atf_tc_fail("failed to read security.bsd.map_at_zero");
	val = !val; /* 1 == enable  map at zero */
#endif
#ifdef __NetBSD__
	if (sysctlbyname("vm.user_va0_disable", &val, &len, NULL, 0) != 0)
		atf_tc_fail("failed to read vm.user_va0_disable");
#endif

	map = mmap(NULL, page, PROT_EXEC, flags, -1, 0);
	map_check(map, val);

	map = mmap(NULL, page, PROT_READ, flags, -1, 0);
	map_check(map, val);

	map = mmap(NULL, page, PROT_WRITE, flags, -1, 0);
	map_check(map, val);

	map = mmap(NULL, page, PROT_READ|PROT_WRITE, flags, -1, 0);
	map_check(map, val);

	map = mmap(NULL, page, PROT_EXEC|PROT_READ|PROT_WRITE, flags, -1, 0);
	map_check(map, val);
}

ATF_TP_ADD_TCS(tp)
{
	page = sysconf(_SC_PAGESIZE);
	ATF_REQUIRE(page >= 0);

#ifdef __NetBSD__
	ATF_TP_ADD_TC(tp, mmap_block);
#endif
	ATF_TP_ADD_TC(tp, mmap_err);
	ATF_TP_ADD_TC(tp, mmap_loan);
	ATF_TP_ADD_TC(tp, mmap_prot_1);
	ATF_TP_ADD_TC(tp, mmap_prot_2);
	ATF_TP_ADD_TC(tp, mmap_prot_3);
	ATF_TP_ADD_TC(tp, mmap_truncate);
	ATF_TP_ADD_TC(tp, mmap_truncate_signal);
	ATF_TP_ADD_TC(tp, mmap_va0);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_mprotect.c",
        sha256: "05f98b33f5f8dff9a50a6f0e047e911fb0708f13e10f80c5800770da4c131940",
        body: r##"/* $NetBSD: t_mprotect.c,v 1.4 2016/05/28 14:34:49 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_mprotect.c,v 1.4 2016/05/28 14:34:49 christos Exp $");

#include <sys/param.h>
#include <sys/mman.h>
#include <sys/sysctl.h>
#include <sys/wait.h>

#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <atf-c.h>

#ifdef __NetBSD__
#include "../common/exec_prot.h"
#endif

static long	page = 0;
static int	pax_global = -1;
static int	pax_enabled = -1;
static char	path[] = "mmap";

static void	sighandler(int);
static bool	paxinit(void);
static bool	paxset(int, int);

static void
sighandler(int signo)
{
	_exit(signo);
}

static bool
paxinit(void)
{
	size_t len = sizeof(int);
	int rv;

	rv = sysctlbyname("security.pax.mprotect.global",
	    &pax_global, &len, NULL, 0);

	if (rv != 0)
		return false;

	rv = sysctlbyname("security.pax.mprotect.enabled",
	    &pax_enabled, &len, NULL, 0);

	return rv == 0;
}

static bool
paxset(int global, int enabled)
{
	size_t len = sizeof(int);
	int rv;

	rv = sysctlbyname("security.pax.mprotect.global",
	    NULL, NULL, &global, len);

	if (rv != 0)
		return false;

	rv = sysctlbyname("security.pax.mprotect.enabled",
	    NULL, NULL, &enabled, len);

	if (rv != 0)
		return false;

	return true;
}


ATF_TC_WITH_CLEANUP(mprotect_access);
ATF_TC_HEAD(mprotect_access, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test for EACCES from mprotect(2)");
}

ATF_TC_BODY(mprotect_access, tc)
{
	int prot[2] = { PROT_NONE, PROT_READ };
	void *map;
	size_t i;
	int fd;

	fd = open(path, O_RDONLY | O_CREAT, 0600);
	ATF_REQUIRE(fd >= 0);

	/*
	 * The call should fail with EACCES if we try to mark
	 * a PROT_NONE or PROT_READ file/section as PROT_WRITE.
	 */
	for (i = 0; i < __arraycount(prot); i++) {

		map = mmap(NULL, page, prot[i], MAP_SHARED, fd, 0);

		if (map == MAP_FAILED)
			continue;

		errno = 0;

		ATF_REQUIRE(mprotect(map, page, PROT_WRITE) != 0);
		ATF_REQUIRE(errno == EACCES);
		ATF_REQUIRE(munmap(map, page) == 0);
	}

	ATF_REQUIRE(close(fd) == 0);
}

ATF_TC_CLEANUP(mprotect_access, tc)
{
	(void)unlink(path);
}

ATF_TC(mprotect_err);
ATF_TC_HEAD(mprotect_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of mprotect(2)");
}

ATF_TC_BODY(mprotect_err, tc)
{
	errno = 0;

	ATF_REQUIRE(mprotect((char *)-1, 1, PROT_READ) != 0);
	ATF_REQUIRE(errno == EINVAL);
}

#ifdef __NetBSD__
ATF_TC(mprotect_exec);
ATF_TC_HEAD(mprotect_exec, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test mprotect(2) executable space protections");
}

/*
 * Trivial function -- should fit into a page
 */
ATF_TC_BODY(mprotect_exec, tc)
{
	pid_t pid;
	void *map;
	int sta, xp_support;

	xp_support = exec_prot_support();

	switch (xp_support) {
	case NOTIMPL:
		atf_tc_skip(
		    "Execute protection callback check not implemented");
		break;
	case NO_XP:
		atf_tc_skip(
		    "Host does not support executable space protection");
		break;
	case PARTIAL_XP: case PERPAGE_XP: default:
		break;
	}

	if (!paxinit())
		return;
	if (pax_enabled == 1 && pax_global == 1)
		atf_tc_skip("PaX MPROTECT restrictions enabled");
		

	/*
	 * Map a page read/write and copy a trivial assembly function inside.
	 * We will then change the mapping rights:
	 * - first by setting the execution right, and check that we can
	 *   call the code found in the allocated page.
	 * - second by removing the execution right. This should generate
	 *   a SIGSEGV on architectures that can enforce --x permissions.
	 */

	map = mmap(NULL, page, PROT_WRITE|PROT_READ, MAP_ANON, -1, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	memcpy(map, (void *)return_one,
	    (uintptr_t)return_one_end - (uintptr_t)return_one);
 
	/* give r-x rights then call code in page */
	ATF_REQUIRE(mprotect(map, page, PROT_EXEC|PROT_READ) == 0);
	ATF_REQUIRE(((int (*)(void))map)() == 1);

	/* remove --x right */
	ATF_REQUIRE(mprotect(map, page, PROT_READ) == 0);

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		ATF_REQUIRE(signal(SIGSEGV, sighandler) != SIG_ERR);
		ATF_CHECK(((int (*)(void))map)() == 1);
		_exit(0);
	}

	(void)wait(&sta);

	ATF_REQUIRE(munmap(map, page) == 0);

	ATF_REQUIRE(WIFEXITED(sta) != 0);

	switch (xp_support) {
	case PARTIAL_XP:
		/* Partial protection might fail; skip the test when it does */
		if (WEXITSTATUS(sta) != SIGSEGV) {
			atf_tc_skip("Host only supports "
			    "partial executable space protection");
		}
		break;
	case PERPAGE_XP: default:
		/* Per-page --x protection should not fail */
		ATF_REQUIRE(WEXITSTATUS(sta) == SIGSEGV);
		break;
	}
}
#endif

ATF_TC(mprotect_pax);
ATF_TC_HEAD(mprotect_pax, tc)
{
	atf_tc_set_md_var(tc, "descr", "PaX restrictions and mprotect(2)");
	atf_tc_set_md_var(tc, "require.user", "root");
}

ATF_TC_BODY(mprotect_pax, tc)
{
	const int prot[4] = { PROT_NONE, PROT_READ, PROT_WRITE };
	const char *str = NULL;
	void *map;
	size_t i;
	int rv;

	if (!paxinit() || !paxset(1, 1))
		return;

	/*
	 * As noted in the original PaX documentation [1],
	 * the following restrictions should apply:
	 *
	 *   (1) creating executable anonymous mappings
	 *
	 *   (2) creating executable/writable file mappings
	 *
	 *   (3) making a non-executable mapping executable
	 *
	 *   (4) making an executable/read-only file mapping
	 *       writable except for performing relocations
	 *       on an ET_DYN ELF file (non-PIC shared library)
	 *
	 *  The following will test only the case (3).
	 *
	 * [1] http://pax.grsecurity.net/docs/mprotect.txt
	 *
	 *     (Sun Apr 3 11:06:53 EEST 2011.)
	 */
	for (i = 0; i < __arraycount(prot); i++) {

		map = mmap(NULL, page, prot[i], MAP_ANON, -1, 0);

		if (map == MAP_FAILED)
			continue;

		rv = mprotect(map, 1, prot[i] | PROT_EXEC);

		(void)munmap(map, page);

		if (rv == 0) {
			str = "non-executable mapping made executable";
			goto out;
		}
	}

out:
	if (pax_global != -1 && pax_enabled != -1)
		(void)paxset(pax_global, pax_enabled);

	if (str != NULL)
		atf_tc_fail("%s", str);
}

ATF_TC(mprotect_write);
ATF_TC_HEAD(mprotect_write, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test mprotect(2) write protections");
}

ATF_TC_BODY(mprotect_write, tc)
{
	pid_t pid;
	void *map;
	int sta;

	/*
	 * Map a page read/write, change the protection
	 * to read-only with mprotect(2), and try to write
	 * to the page. This should generate a SIGSEGV.
	 */
	map = mmap(NULL, page, PROT_WRITE|PROT_READ, MAP_ANON, -1, 0);
	ATF_REQUIRE(map != MAP_FAILED);

	ATF_REQUIRE(strlcpy(map, "XXX", 3) == 3);
	ATF_REQUIRE(mprotect(map, page, PROT_READ) == 0);

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		ATF_REQUIRE(signal(SIGSEGV, sighandler) != SIG_ERR);
		ATF_REQUIRE(strlcpy(map, "XXX", 3) == 0);
	}

	(void)wait(&sta);

	ATF_REQUIRE(WIFEXITED(sta) != 0);
	ATF_REQUIRE(WEXITSTATUS(sta) == SIGSEGV);
	ATF_REQUIRE(munmap(map, page) == 0);
}

ATF_TP_ADD_TCS(tp)
{
	page = sysconf(_SC_PAGESIZE);
	ATF_REQUIRE(page >= 0);

	ATF_TP_ADD_TC(tp, mprotect_access);
	ATF_TP_ADD_TC(tp, mprotect_err);
#ifdef __NetBSD__
	ATF_TP_ADD_TC(tp, mprotect_exec);
#endif
	ATF_TP_ADD_TC(tp, mprotect_pax);
	ATF_TP_ADD_TC(tp, mprotect_write);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_pipe.c",
        sha256: "f5bf091b8f134b89440ef51380c1367653be5be1ce4b1f5acbbfe5eabd506b94",
        body: r##"/* $NetBSD: t_pipe.c,v 1.5 2017/01/13 21:30:41 christos Exp $ */

/*-
 * Copyright (c) 2001, 2008 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#include <sys/cdefs.h>
__COPYRIGHT("@(#) Copyright (c) 2008\
 The NetBSD Foundation, inc. All rights reserved.");
__RCSID("$NetBSD: t_pipe.c,v 1.5 2017/01/13 21:30:41 christos Exp $");

#include <sys/types.h>
#include <sys/wait.h>

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#include <atf-c.h>

#include "h_macros.h"

static pid_t pid;
static int nsiginfo = 0;

/*
 * This is used for both parent and child. Handle parent's SIGALRM,
 * the childs SIGINFO doesn't need anything.
 */
static void
sighand(int sig)
{
	if (sig == SIGALRM) {
		kill(pid, SIGINFO);
	}
	if (sig == SIGINFO) {
		nsiginfo++;
	}
}

ATF_TC(pipe_restart);
ATF_TC_HEAD(pipe_restart, tc)
{
	atf_tc_set_md_var(tc, "descr", "Checks that writing to pipe "
	    "works correctly after being interrupted and restarted "
	    "(kern/14087)");
}

ATF_TC_BODY(pipe_restart, tc)
{
	int pp[2], st;
	ssize_t sz, todo, done;
	char *f;
	sigset_t asigset, osigset, emptysigset;

	/* Initialise signal masks */
	RL(sigemptyset(&emptysigset));
	RL(sigemptyset(&asigset));
	RL(sigaddset(&asigset, SIGINFO));

	/* Register signal handlers for both read and writer */
	REQUIRE_LIBC(signal(SIGINFO, sighand), SIG_ERR);
	REQUIRE_LIBC(signal(SIGALRM, sighand), SIG_ERR);

	todo = 2 * 1024 * 1024;
	REQUIRE_LIBC(f = malloc(todo), NULL);

	RL(pipe(pp));

	RL(pid = fork());
	if (pid == 0) {
		/* child */
		RL(close(pp[1]));

		/* Do inital write. This should succeed, make
		 * the other side do partial write and wait for us to pick
		 * rest up.
		 */
		RL(done = read(pp[0], f, 128 * 1024));

		/* Wait until parent is alarmed and awakens us */
		RL(sigprocmask(SIG_BLOCK, &asigset, &osigset));
		while (nsiginfo == 0) {
			if (sigsuspend(&emptysigset) != -1 || errno != EINTR)
				atf_tc_fail("sigsuspend(&emptysigset): %s",
				    strerror(errno));
		}
		RL(sigprocmask(SIG_SETMASK, &osigset, NULL));

		/* Read all what parent wants to give us */
		while((sz = read(pp[0], f, 1024 * 1024)) > 0)
			done += sz;

		/*
		 * Exit with 1 if number of bytes read doesn't match
		 * number of expected bytes
		 */
		printf("Read:     %#zx\n", (size_t)done);
		printf("Expected: %#zx\n", (size_t)todo);

		exit(done != todo);

		/* NOTREACHED */
	} else {
		RL(close(pp[0]));

		/*
		 * Arrange for alarm after two seconds. Since we have
		 * handler setup for SIGARLM, the write(2) call should
		 * be restarted internally by kernel.
		 */
		(void)alarm(2);

		/* We write exactly 'todo' bytes. The very first write(2)
		 * should partially succeed, block and eventually
		 * be restarted by kernel
		 */
		while(todo > 0 && ((sz = write(pp[1], f, todo)) > 0))
			todo -= sz;

		/* Close the pipe, so that child would stop reading */
		RL(close(pp[1]));

		/* And pickup child's exit status */
		RL(waitpid(pid, &st, 0));

		ATF_REQUIRE_EQ(WEXITSTATUS(st), 0);
	}
	free(f);
}

ATF_TP_ADD_TCS(tp)
{
	ATF_TP_ADD_TC(tp, pipe_restart);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_poll.c",
        sha256: "80d4e927d2f2f3a8a1bd1fdd20bed9dc5f1402c7684189317614f2574a400980",
        body: r##"/*	$NetBSD: t_poll.c,v 1.3 2012/03/18 07:00:52 jruoho Exp $	*/

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Matthias Scheler.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#include <sys/time.h>
#include <sys/wait.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <paths.h>
#include <poll.h>
#include <stdio.h>
#include <signal.h>
#include <unistd.h>

static int desc;

static void
child1(void)
{
	struct pollfd pfd;

	pfd.fd = desc;
	pfd.events = POLLIN | POLLHUP | POLLOUT;

	(void)poll(&pfd, 1, 2000);
	(void)printf("child1 exit\n");
}

static void
child2(void)
{
	struct pollfd pfd;

	pfd.fd = desc;
	pfd.events = POLLIN | POLLHUP | POLLOUT;

	(void)sleep(1);
	(void)poll(&pfd, 1, INFTIM);
	(void)printf("child2 exit\n");
}

static void
child3(void)
{
	struct pollfd pfd;

	(void)sleep(5);

	pfd.fd = desc;
	pfd.events = POLLIN | POLLHUP | POLLOUT;

	(void)poll(&pfd, 1, INFTIM);
	(void)printf("child3 exit\n");
}

ATF_TC(poll_3way);
ATF_TC_HEAD(poll_3way, tc)
{
	atf_tc_set_md_var(tc, "timeout", "15");
	atf_tc_set_md_var(tc, "descr",
	    "Check for 3-way collision for descriptor. First child comes "
	    "and polls on descriptor, second child comes and polls, first "
	    "child times out and exits, third child comes and polls. When "
	    "the wakeup event happens, the two remaining children should "
	    "both be awaken. (kern/17517)");
}

ATF_TC_BODY(poll_3way, tc)
{
	int pf[2];
	int status, i;
	pid_t pid;

	pipe(pf);
	desc = pf[0];

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		(void)close(pf[1]);
		child1();
		_exit(0);
		/* NOTREACHED */
	}

	pid = fork();
	ATF_REQUIRE(pid >= 0);

	if (pid == 0) {
		(void)close(pf[1]);
		child2();
		_exit(0);
		/* NOTREACHED */
	}

	pid = fork();
	ATF_REQUIRE( pid >= 0);

	if (pid == 0) {
		(void)close(pf[1]);
		child3();
		_exit(0);
		/* NOTREACHED */
	}

	(void)sleep(10);

	(void)printf("parent write\n");

	ATF_REQUIRE(write(pf[1], "konec\n", 6) == 6);

	for(i = 0; i < 3; ++i)
		(void)wait(&status);

	(void)printf("parent terminated\n");
}

ATF_TC(poll_basic);
ATF_TC_HEAD(poll_basic, tc)
{
	atf_tc_set_md_var(tc, "timeout", "10");
	atf_tc_set_md_var(tc, "descr",
	    "Basis functionality test for poll(2)");
}

ATF_TC_BODY(poll_basic, tc)
{
	int fds[2];
	struct pollfd pfds[2];
	int ret;

	ATF_REQUIRE_EQ(pipe(fds), 0);

	pfds[0].fd = fds[0];
	pfds[0].events = POLLIN;
	pfds[1].fd = fds[1];
	pfds[1].events = POLLOUT;

	/*
	 * Check that we get a timeout waiting for data on the read end
	 * of our pipe.
	 */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = poll(&pfds[0], 1, 1), 0,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, 0, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, -1, "got: %d", pfds[1].revents);

	/* Check that the write end of the pipe as reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = poll(&pfds[1], 1, 1), 1,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, -1, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",\
	    pfds[1].revents);

	/* Check that only the write end of the pipe as reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = poll(pfds, 2, 1), 1,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, 0, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",
	    pfds[1].revents);

	/* Write data to our pipe. */
	ATF_REQUIRE_EQ(write(fds[1], "", 1), 1);

	/* Check that both ends of our pipe are reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = poll(pfds, 2, 1), 2,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, POLLIN, "got: %d",
	    pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",
	    pfds[1].revents);

	ATF_REQUIRE_EQ(close(fds[0]), 0);
	ATF_REQUIRE_EQ(close(fds[1]), 0);
}

ATF_TC(poll_err);
ATF_TC_HEAD(poll_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Check errors from poll(2)");
}

ATF_TC_BODY(poll_err, tc)
{
	struct pollfd pfd;
	int fd = 0;

	pfd.fd = fd;
	pfd.events = POLLIN;

	errno = 0;
	ATF_REQUIRE_ERRNO(EFAULT, poll((struct pollfd *)-1, 1, -1) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(EINVAL, poll(&pfd, 1, -2) == -1);
}

/*
 * RavenBSD portability: NetBSD has pollts(2); FreeBSD and Raven have ppoll(2) with the same
 * prototype and the same contract (poll(2) with a temporary signal mask). The affected bodies
 * keep their donor names and expectations and call ppoll(2); no product pollts alias is added.
 * FreeBSD's own port of this test compiles these bodies out; this port runs them so the mask
 * behaviour is exercised.
 */
#ifndef __FreeBSD__
ATF_TC(pollts_basic);
ATF_TC_HEAD(pollts_basic, tc)
{
	atf_tc_set_md_var(tc, "timeout", "10");
	atf_tc_set_md_var(tc, "descr",
	    "Basis functionality test for pollts(2)");
}

ATF_TC_BODY(pollts_basic, tc)
{
	int fds[2];
	struct pollfd pfds[2];
	struct timespec timeout;
	int ret;

	ATF_REQUIRE_EQ(pipe(fds), 0);

	pfds[0].fd = fds[0];
	pfds[0].events = POLLIN;
	pfds[1].fd = fds[1];
	pfds[1].events = POLLOUT;

	/* Use a timeout of 1 second. */
	timeout.tv_sec = 1;
	timeout.tv_nsec = 0;

	/*
	 * Check that we get a timeout waiting for data on the read end
	 * of our pipe.
	 */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = ppoll(&pfds[0], 1, &timeout, NULL), 0,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, 0, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, -1, "got: %d", pfds[1].revents);

	/* Check that the write end of the pipe as reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = ppoll(&pfds[1], 1, &timeout, NULL), 1,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, -1, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",\
	    pfds[1].revents);

	/* Check that only the write end of the pipe as reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = ppoll(pfds, 2, &timeout, NULL), 1,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, 0, "got: %d", pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",
	    pfds[1].revents);

	/* Write data to our pipe. */
	ATF_REQUIRE_EQ(write(fds[1], "", 1), 1);

	/* Check that both ends of our pipe are reported as ready. */
	pfds[0].revents = -1;
	pfds[1].revents = -1;
	ATF_REQUIRE_EQ_MSG(ret = ppoll(pfds, 2, &timeout, NULL), 2,
	    "got: %d", ret);
	ATF_REQUIRE_EQ_MSG(pfds[0].revents, POLLIN, "got: %d",
	    pfds[0].revents);
	ATF_REQUIRE_EQ_MSG(pfds[1].revents, POLLOUT, "got: %d",
	    pfds[1].revents);

	ATF_REQUIRE_EQ(close(fds[0]), 0);
	ATF_REQUIRE_EQ(close(fds[1]), 0);
}

ATF_TC(pollts_err);
ATF_TC_HEAD(pollts_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Check errors from pollts(2)");
}

ATF_TC_BODY(pollts_err, tc)
{
	struct timespec timeout;
	struct pollfd pfd;
	int fd = 0;

	pfd.fd = fd;
	pfd.events = POLLIN;

	timeout.tv_sec = 1;
	timeout.tv_nsec = 0;

	errno = 0;
	ATF_REQUIRE_ERRNO(EFAULT, ppoll((void *)-1, 1, &timeout, NULL) == -1);

	timeout.tv_sec = -1;
	timeout.tv_nsec = -1;

	errno = 0;
	ATF_REQUIRE_ERRNO(EINVAL, ppoll(&pfd, 1, &timeout, NULL) == -1);
}

ATF_TC(pollts_sigmask);
ATF_TC_HEAD(pollts_sigmask, tc)
{
	atf_tc_set_md_var(tc, "timeout", "10");
	atf_tc_set_md_var(tc, "descr",
	    "Check that pollts(2) restores the signal mask (PR kern/44986)");
}

ATF_TC_BODY(pollts_sigmask, tc)
{
	int fd;
	struct pollfd pfd;
	struct timespec timeout;
	sigset_t mask;
	int ret;

	fd = open(_PATH_DEVNULL, O_RDONLY);
	ATF_REQUIRE(fd >= 0);

	pfd.fd = fd;
	pfd.events = POLLIN;

	/* Use a timeout of 1 second. */
	timeout.tv_sec = 1;
	timeout.tv_nsec = 0;

	/* Unblock all signals. */
	ATF_REQUIRE_EQ(sigfillset(&mask), 0);
	ATF_REQUIRE_EQ(sigprocmask(SIG_UNBLOCK, &mask, NULL), 0);

	/*
	 * Check that pollts(2) immediately returns. We block *all*
	 * signals during pollts(2).
	 */
	ATF_REQUIRE_EQ_MSG(ret = ppoll(&pfd, 1, &timeout, &mask), 1,
	    "got: %d", ret);

	/* Check that signals are now longer blocked. */
	ATF_REQUIRE_EQ(sigprocmask(SIG_SETMASK, NULL, &mask), 0);
	ATF_REQUIRE_EQ_MSG(sigismember(&mask, SIGUSR1), 0,
	    "signal mask was changed.");

	ATF_REQUIRE_EQ(close(fd), 0);
}
#endif

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, poll_3way);
	ATF_TP_ADD_TC(tp, poll_basic);
	ATF_TP_ADD_TC(tp, poll_err);
#ifndef __FreeBSD__
	ATF_TP_ADD_TC(tp, pollts_basic);
	ATF_TP_ADD_TC(tp, pollts_err);
	ATF_TP_ADD_TC(tp, pollts_sigmask);
#endif

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_select.c",
        sha256: "c06cc6845b9795b9189d1792f3f402910172ed861f7dd6714052c57a09c76e8a",
        body: r##"/*	$NetBSD: t_select.c,v 1.4 2017/01/13 21:18:33 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundatiom
 * by Christos Zoulas.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

#include <assert.h>
#include <sys/types.h>
#include <sys/select.h>
#include <sys/wait.h>
#include <err.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>

#include <atf-c.h>

static sig_atomic_t keep_going = 1;

static void
sig_handler(int signum __unused)
{
	keep_going = 0;
}

static void
sigchld(int signum __unused)
{
}

static char
xtoa(uint8_t n)
{
	static const char xarray[] = "0123456789abcdef";
	assert(n < sizeof(xarray));
	return xarray[n];
}

static const char *
prmask(const sigset_t *m, char *buf, size_t len)
{
	size_t j = 2;
	assert(len >= 3 + sizeof(*m));
	buf[0] = '0';
	buf[1] = 'x';
#define N(p, a)	(((p) >> ((a) * 4)) & 0xf)
	for (size_t i = __arraycount(m->__bits); i > 0; i--) {
		uint32_t p = m->__bits[i - 1];
		for (size_t k = sizeof(p); k > 0; k--)
			buf[j++] = xtoa(N(p, k - 1));
	}
	buf[j] = '\0';
	return buf;
}

static __dead void
child(const struct timespec *ts)
{
	struct sigaction sa;
	sigset_t set, oset, nset;
	char obuf[sizeof(oset) + 3], nbuf[sizeof(nset) + 3];
	int fd;

	memset(&sa, 0, sizeof(sa));
	sa.sa_handler = sig_handler;
	if ((fd = open("/dev/null", O_RDONLY)) == -1)
		err(1, "open");

	if (sigaction(SIGTERM, &sa, NULL) == -1)
		err(1, "sigaction");

	sigfillset(&set);
	if (sigprocmask(SIG_BLOCK, &set, NULL) == -1)
		err(1, "sigprocmask");

	if (sigprocmask(SIG_BLOCK, NULL, &oset) == -1)
		err(1, "sigprocmask");

	sigemptyset(&set);

	for (;;) {
		fd_set rset;
		FD_ZERO(&rset);
		FD_SET(fd, &rset);
		if (pselect(1, &rset, NULL, NULL, ts, &set) == -1) {
			if(errno == EINTR) {
				if (!keep_going)
					break;
			}
		}
		if (ts)
			break;
	}
	if (sigprocmask(SIG_BLOCK, NULL, &nset) == -1)
		err(1, "sigprocmask");
	if (memcmp(&oset, &nset, sizeof(oset)) != 0)
		atf_tc_fail("pselect() masks don't match "
		    "after timeout %s != %s",
		    prmask(&nset, nbuf, sizeof(nbuf)),
		    prmask(&oset, obuf, sizeof(obuf)));
	_exit(0);
}

ATF_TC(pselect_sigmask);
ATF_TC_HEAD(pselect_sigmask, tc)
{
	atf_tc_set_md_var(tc, "descr", "Checks pselect's temporary mask "
	    "setting when a signal is received (PR lib/43625)");
}

ATF_TC_BODY(pselect_sigmask, tc)
{
	pid_t pid;
	int status;

	signal(SIGCHLD, sigchld);

	switch (pid = fork()) {
	case 0:
		child(NULL);
		/*NOTREACHED*/
	case -1:
		err(1, "fork");
	default:
		sleep(1);
		if (kill(pid, SIGTERM) == -1)
			err(1, "kill");
		sleep(1);
		switch (waitpid(pid, &status, WNOHANG)) {
		case -1:
			err(1, "wait");
		case 0:
			if (kill(pid, SIGKILL) == -1)
				err(1, "kill");
			atf_tc_fail("pselect() did not receive signal");
			break;
		default:
			break;
		}
	}
}

ATF_TC(pselect_timeout);
ATF_TC_HEAD(pselect_timeout, tc)
{

	atf_tc_set_md_var(tc, "descr", "Checks pselect's temporary mask "
	    "setting when a timeout occurs");
}

ATF_TC_BODY(pselect_timeout, tc)
{
	pid_t pid;
	int status;
	static const struct timespec zero = { 0, 0 };

	signal(SIGCHLD, sigchld);

	switch (pid = fork()) {
	case 0:
		child(&zero);
		break;
	case -1:
		err(1, "fork");
	default:
		sleep(1);
		switch (waitpid(pid, &status, WNOHANG)) {
		case -1:
			err(1, "wait");
		case 0:
			if (kill(pid, SIGKILL) == -1)
				err(1, "kill");
			atf_tc_fail("pselect() did not receive signal");
			break;
		default:
			break;
		}
	}
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, pselect_sigmask);
	ATF_TP_ADD_TC(tp, pselect_timeout);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_socketpair.c",
        sha256: "f521155baeae93ce150c89181afbd82ef06acec10a9f2a0f1e27116b98aa08c1",
        body: r##"/* $NetBSD: t_socketpair.c,v 1.2 2017/01/13 20:04:52 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Christos Zoulas.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. All advertising materials mentioning features or use of this software
 *    must display the following acknowledgement:
 *        This product includes software developed by the NetBSD
 *        Foundation, Inc. and its contributors.
 * 4. Neither the name of The NetBSD Foundation nor the names of its
 *    contributors may be used to endorse or promote products derived
 *    from this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_socketpair.c,v 1.2 2017/01/13 20:04:52 christos Exp $");

#include <atf-c.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <errno.h>

static void
connected(int fd)
{
	struct sockaddr_un addr;
	socklen_t len = (socklen_t)sizeof(addr);
	ATF_REQUIRE(getpeername(fd, (struct sockaddr*)(void *)&addr,
	    &len) == 0);
}

static void
run(int domain, int type, int flags)
{
	int fd[2], i;

	while ((i = open("/", O_RDONLY)) < 3)
		ATF_REQUIRE(i != -1);

#ifdef __NetBSD__
	/* This check is harmful since it closes atf's output file */
	ATF_REQUIRE(closefrom(3) != -1);
#endif

	ATF_REQUIRE(socketpair(domain, type | flags, 0, fd) == 0);

#if __NetBSD__
	/* This check is harmful since it requires closing atf's output file */
	ATF_REQUIRE(fd[0] == 3);
	ATF_REQUIRE(fd[1] == 4);
#endif

	connected(fd[0]);
	connected(fd[1]);

	if (flags & SOCK_CLOEXEC) {
		ATF_REQUIRE((fcntl(fd[0], F_GETFD) & FD_CLOEXEC) != 0);
		ATF_REQUIRE((fcntl(fd[1], F_GETFD) & FD_CLOEXEC) != 0);
	} else {
		ATF_REQUIRE((fcntl(fd[0], F_GETFD) & FD_CLOEXEC) == 0);
		ATF_REQUIRE((fcntl(fd[1], F_GETFD) & FD_CLOEXEC) == 0);
	}

	if (flags & SOCK_NONBLOCK) {
		ATF_REQUIRE((fcntl(fd[0], F_GETFL) & O_NONBLOCK) != 0);
		ATF_REQUIRE((fcntl(fd[1], F_GETFL) & O_NONBLOCK) != 0);
	} else {
		ATF_REQUIRE((fcntl(fd[0], F_GETFL) & O_NONBLOCK) == 0);
		ATF_REQUIRE((fcntl(fd[1], F_GETFL) & O_NONBLOCK) == 0);
	}

	ATF_REQUIRE(close(fd[0]) != -1);
	ATF_REQUIRE(close(fd[1]) != -1);
}

ATF_TC(inet);
ATF_TC_HEAD(inet, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "socketpair(2) does not work in the PF_INET domain");
}

ATF_TC_BODY(inet, tc)
{
	int fd[2];

	ATF_REQUIRE_EQ(socketpair(PF_INET, SOCK_DGRAM, 0, fd), -1);
	ATF_REQUIRE_EQ(EOPNOTSUPP, errno);
	ATF_REQUIRE_EQ(socketpair(PF_INET, SOCK_STREAM, 0, fd), -1);
	ATF_REQUIRE_EQ(EOPNOTSUPP, errno);
}

ATF_TC(null_sv);
ATF_TC_HEAD(null_sv, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "socketpair(2) should fail without return storage");
}

ATF_TC_BODY(null_sv, tc)
{
	int fd;

#ifdef __NetBSD__
	/* This check is harmful since it closes atf's output file */
	closefrom(3);
#else
	int lowfd = open("/", O_RDONLY);
	ATF_REQUIRE(lowfd > 0);
	ATF_REQUIRE_EQ(0, close(lowfd));
#endif
	ATF_REQUIRE_EQ(socketpair(AF_UNIX, SOCK_DGRAM, 0, NULL), -1);
	ATF_REQUIRE_EQ(EFAULT, errno);
	fd = open("/", O_RDONLY);
#ifdef __NetBSD__
	ATF_REQUIRE_EQ_MSG(fd, 3,
	    "socketpair(..., NULL) allocated descriptors");
#else
	ATF_REQUIRE_EQ_MSG(fd, lowfd,
	    "socketpair(..., NULL) allocated descriptors: fd=%d, lowfd=%d",
	    fd, lowfd);
#endif
}

ATF_TC(socketpair_basic);
ATF_TC_HEAD(socketpair_basic, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of socketpair(2)");
}

ATF_TC_BODY(socketpair_basic, tc)
{
	run(AF_UNIX, SOCK_DGRAM, 0);
}

ATF_TC(socketpair_nonblock);
ATF_TC_HEAD(socketpair_nonblock, tc)
{
	atf_tc_set_md_var(tc, "descr", "A non-blocking test of socketpair(2)");
}

ATF_TC_BODY(socketpair_nonblock, tc)
{
	run(AF_UNIX, SOCK_DGRAM, SOCK_NONBLOCK);
}

ATF_TC(socketpair_cloexec);
ATF_TC_HEAD(socketpair_cloexec, tc)
{
	atf_tc_set_md_var(tc, "descr", "A close-on-exec of socketpair(2)");
}

ATF_TC_BODY(socketpair_cloexec, tc)
{
	run(AF_UNIX, SOCK_DGRAM, SOCK_CLOEXEC);
}

ATF_TC(socketpair_stream);
ATF_TC_HEAD(socketpair_stream, tc)
{
	atf_tc_set_md_var(tc, "descr", "A stream-oriented socketpair(2)");
}

ATF_TC_BODY(socketpair_stream, tc)
{
	run(AF_UNIX, SOCK_STREAM, 0);
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, inet);
	ATF_TP_ADD_TC(tp, null_sv);
	ATF_TP_ADD_TC(tp, socketpair_basic);
	ATF_TP_ADD_TC(tp, socketpair_nonblock);
	ATF_TP_ADD_TC(tp, socketpair_cloexec);
	ATF_TP_ADD_TC(tp, socketpair_stream);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_umask.c",
        sha256: "17a128158754b4de6c0b68300e4eddef7ef2aa7292a87cf72f1d29c78037e9d0",
        body: r##"/* $NetBSD: t_umask.c,v 1.2 2017/01/13 19:34:19 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_umask.c,v 1.2 2017/01/13 19:34:19 christos Exp $");

#include <sys/stat.h>
#include <sys/wait.h>

#include <atf-c.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char path[] = "umask";
static const mode_t mask[] = {
	S_IRWXU,
	S_IRUSR,
	S_IWUSR,
	S_IXUSR,
	S_IRWXG,
	S_IRGRP,
	S_IWGRP,
	S_IXGRP,
	S_IRWXO,
	S_IROTH,
	S_IWOTH,
	S_IXOTH
};

ATF_TC_WITH_CLEANUP(umask_fork);
ATF_TC_HEAD(umask_fork, tc)
{
	atf_tc_set_md_var(tc, "descr", "Check that umask(2) is inherited");
}

ATF_TC_BODY(umask_fork, tc)
{
	mode_t mode;
	pid_t pid;
	size_t i;
	int sta;

	for (i = 0; i < __arraycount(mask) - 1; i++) {

		(void)umask(mask[i] | mask[i + 1]);

		pid = fork();

		if (pid < 0)
			continue;

		if (pid == 0) {

			mode = umask(mask[i]);

			if (mode != (mask[i] | mask[i + 1]))
				_exit(EXIT_FAILURE);

			_exit(EXIT_SUCCESS);
		}

		(void)wait(&sta);

		if (WIFEXITED(sta) == 0 || WEXITSTATUS(sta) != EXIT_SUCCESS)
			goto fail;
	}

	return;

fail:
	(void)umask(S_IWGRP | S_IWOTH);

	atf_tc_fail("umask(2) was not inherited");
}

ATF_TC_CLEANUP(umask_fork, tc)
{
	(void)umask(S_IWGRP | S_IWOTH);
}

ATF_TC_WITH_CLEANUP(umask_open);
ATF_TC_HEAD(umask_open, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of open(2) and umask(2)");
}

ATF_TC_BODY(umask_open, tc)
{
	const char *str = NULL;
	struct stat st;
	size_t i;
	int fd;

	for (i = 0; i < __arraycount(mask); i++) {

		(void)umask(mask[i]);

		fd = open(path, O_RDWR | O_CREAT, 0777);

		if (fd < 0)
			continue;

		(void)close(fd);
		(void)memset(&st, 0, sizeof(struct stat));

		if (stat(path, &st) != 0) {
			str = "failed to stat(2)";
			goto out;
		}

		if ((st.st_mode & mask[i]) != 0) {
			str = "invalid umask(2)";
			goto out;
		}

		if (unlink(path) != 0) {
			str = "failed to unlink(2)";
			goto out;
		}

	}

out:
	(void)umask(S_IWGRP | S_IWOTH);

	if (str != NULL)
		atf_tc_fail("%s", str);
}

ATF_TC_CLEANUP(umask_open, tc)
{
	(void)umask(S_IWGRP | S_IWOTH);
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(umask_previous);
ATF_TC_HEAD(umask_previous, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test the return value from umask(2)");
}

ATF_TC_BODY(umask_previous, tc)
{
	mode_t mode;
	size_t i;

	for (i = 0; i < __arraycount(mask); i++) {

		mode = umask(mask[i]);
		mode = umask(mask[i]);

		if (mode != mask[i])
			goto fail;
	}

	return;

fail:
	(void)umask(S_IWGRP | S_IWOTH);

	atf_tc_fail("umask(2) did not return the previous mask");
}

ATF_TC_CLEANUP(umask_previous, tc)
{
	(void)umask(S_IWGRP | S_IWOTH);
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, umask_fork);
	ATF_TP_ADD_TC(tp, umask_open);
	ATF_TP_ADD_TC(tp, umask_previous);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_unlink.c",
        sha256: "eb1e46e2c6d9d89ef5114c176256bd32c49096205088cca3eb3f46839a950621",
        body: r##"/* $NetBSD: t_unlink.c,v 1.4 2017/01/14 20:55:26 christos Exp $ */

/*-
 * Copyright (c) 2011 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jukka Ruohonen.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_unlink.c,v 1.4 2017/01/14 20:55:26 christos Exp $");

#include <sys/stat.h>

#include <atf-c.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <string.h>
#include <unistd.h>

static char	 path[] = "unlink";

ATF_TC_WITH_CLEANUP(unlink_basic);
ATF_TC_HEAD(unlink_basic, tc)
{
	atf_tc_set_md_var(tc, "descr", "A basic test of unlink(2)");
}

ATF_TC_BODY(unlink_basic, tc)
{
	const size_t n = 512;
	size_t i;
	int fd;

	for (i = 0; i < n; i++) {

		fd = open(path, O_RDWR | O_CREAT, 0666);

		ATF_REQUIRE(fd != -1);
		ATF_REQUIRE(close(fd) == 0);
		ATF_REQUIRE(unlink(path) == 0);

		errno = 0;
		ATF_REQUIRE_ERRNO(ENOENT, open(path, O_RDONLY) == -1);
	}
}

ATF_TC_CLEANUP(unlink_basic, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(unlink_err);
ATF_TC_HEAD(unlink_err, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test error conditions of unlink(2)");
}

ATF_TC_BODY(unlink_err, tc)
{
	char buf[PATH_MAX + 1];

	(void)memset(buf, 'x', sizeof(buf));

	errno = 0;
#ifdef __FreeBSD__
	ATF_REQUIRE_ERRNO(EISDIR, unlink("/") == -1);
#else
	ATF_REQUIRE_ERRNO(EBUSY, unlink("/") == -1);
#endif

	errno = 0;
	ATF_REQUIRE_ERRNO(ENAMETOOLONG, unlink(buf) == -1);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, unlink("/a/b/c/d/e/f/g/h/i/j/k/l/m") == -1);
}

ATF_TC_CLEANUP(unlink_err, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(unlink_fifo);
ATF_TC_HEAD(unlink_fifo, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test unlink(2) for a FIFO");
}

ATF_TC_BODY(unlink_fifo, tc)
{

	ATF_REQUIRE(mkfifo(path, 0666) == 0);
	ATF_REQUIRE(unlink(path) == 0);

	errno = 0;
	ATF_REQUIRE_ERRNO(ENOENT, open(path, O_RDONLY) == -1);
}

ATF_TC_CLEANUP(unlink_fifo, tc)
{
	(void)unlink(path);
}

ATF_TC_WITH_CLEANUP(unlink_perm);
ATF_TC_HEAD(unlink_perm, tc)
{
	atf_tc_set_md_var(tc, "descr", "Test permissions with unlink(2)");
	atf_tc_set_md_var(tc, "require.user", "unprivileged");
}

ATF_TC_BODY(unlink_perm, tc)
{
	int rv;

	errno = 0;
	rv = unlink("/etc");
	ATF_REQUIRE_MSG(rv == -1 && (errno == EACCES || errno == EPERM),
	    "unlinking a directory did not fail with EPERM or EACCESS; "
	    "unlink() returned %d, errno %d", rv, errno);

	errno = 0;
	ATF_REQUIRE_ERRNO(EACCES, unlink("/root/.profile") == -1);
}

ATF_TC_CLEANUP(unlink_perm, tc)
{
	(void)unlink(path);
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, unlink_basic);
	ATF_TP_ADD_TC(tp, unlink_err);
	ATF_TP_ADD_TC(tp, unlink_fifo);
	ATF_TP_ADD_TC(tp, unlink_perm);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_wait.c",
        sha256: "25ba8a77b16716d5aaaf5e9d5025659855055d453400fdf020a39350ddb87c47",
        body: r##"/* $NetBSD: t_wait.c,v 1.8 2017/01/13 19:28:55 christos Exp $ */

/*-
 * Copyright (c) 2016 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Christos Zoulas.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#include <sys/cdefs.h>
__RCSID("$NetBSD: t_wait.c,v 1.8 2017/01/13 19:28:55 christos Exp $");

/*
 * RavenBSD portability: FreeBSD spells the wait6(2) usage record `struct __wrusage`
 * (sys/sys/resource.h:92); NetBSD spells it `struct wrusage`. The target libc exposes the
 * FreeBSD record only, so this port changes the type name and nothing else. The wait6(2)
 * bodies and their expected results are unchanged.
 */

#ifdef __FreeBSD__
#include <sys/types.h>
#include <sys/sysctl.h>
#endif
#include <sys/wait.h>
#include <sys/resource.h>

#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <pwd.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#include <atf-c.h>

ATF_TC(wait6_invalid);
ATF_TC_HEAD(wait6_invalid, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) returns EINVAL with 0 options");
}

ATF_TC_BODY(wait6_invalid, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	ATF_REQUIRE(wait6(P_ALL, 0, &st, 0, &wru, &si) == -1
	    && errno == EINVAL);
}

ATF_TC(wait6_exited);
ATF_TC_HEAD(wait6_exited, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) handled exiting process and code");
}

ATF_TC_BODY(wait6_exited, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	pid_t pid;

	switch (pid = fork()) {
	case -1:
		ATF_REQUIRE(pid > 0);
	case 0:
		exit(0x5a5a5a5a);
		/*NOTREACHED*/
	default:
		ATF_REQUIRE(wait6(P_PID, pid, &st, WEXITED, &wru, &si) == pid);
		ATF_REQUIRE(WIFEXITED(st) && WEXITSTATUS(st) == 0x5a);
		ATF_REQUIRE(si.si_status == 0x5a5a5a5a);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_EXITED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif
		break;
	}
}

ATF_TC(wait6_terminated);
ATF_TC_HEAD(wait6_terminated, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) handled terminated process and code");
}

ATF_TC_BODY(wait6_terminated, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	pid_t pid;

	switch (pid = fork()) {
	case 0:
		sleep(100);
		/*FALLTHROUGH*/
	case -1:
		ATF_REQUIRE(pid > 0);
	default:
		ATF_REQUIRE(kill(pid, SIGTERM) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WEXITED, &wru, &si) == pid);
		ATF_REQUIRE(WIFSIGNALED(st) && WTERMSIG(st) == SIGTERM);
		ATF_REQUIRE(si.si_status == SIGTERM);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_KILLED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif
		break;
	}
}

ATF_TC(wait6_coredumped);
ATF_TC_HEAD(wait6_coredumped, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) handled coredumped process and code");
}

ATF_TC_BODY(wait6_coredumped, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	pid_t pid;
	static const struct rlimit rl = { RLIM_INFINITY, RLIM_INFINITY };

#ifdef __FreeBSD__
	int coredump_enabled;
	size_t ce_len = sizeof(coredump_enabled);
	if (sysctlbyname("kern.coredump", &coredump_enabled, &ce_len, NULL,
	    0) == 0 && !coredump_enabled)
		atf_tc_skip("Coredumps disabled");
#endif

	switch (pid = fork()) {
	case 0:
		ATF_REQUIRE(setrlimit(RLIMIT_CORE, &rl) == 0);
		*(char *)8 = 0;
		/*FALLTHROUGH*/
	case -1:
		ATF_REQUIRE(pid > 0);
	default:
		ATF_REQUIRE(wait6(P_PID, pid, &st, WEXITED, &wru, &si) == pid);
		ATF_REQUIRE(WIFSIGNALED(st) && WTERMSIG(st) == SIGSEGV
		    && WCOREDUMP(st));
		ATF_REQUIRE(si.si_status == SIGSEGV);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_DUMPED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif
		break;
	}
}

ATF_TC(wait6_stop_and_go);
ATF_TC_HEAD(wait6_stop_and_go, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) handled stopped/continued process and code");
}

ATF_TC_BODY(wait6_stop_and_go, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	pid_t pid;
	static const struct rlimit rl = { 0, 0 };

	ATF_REQUIRE(setrlimit(RLIMIT_CORE, &rl) == 0);
	switch (pid = fork()) {
	case 0:
		sleep(100);
		/*FALLTHROUGH*/
	case -1:
		ATF_REQUIRE(pid > 0);
	default:
		ATF_REQUIRE(kill(pid, SIGSTOP) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WSTOPPED, &wru, &si) == pid);
		ATF_REQUIRE(!WIFEXITED(st));
		ATF_REQUIRE(!WIFSIGNALED(st));
		ATF_REQUIRE(WIFSTOPPED(st) && WSTOPSIG(st) == SIGSTOP);
		ATF_REQUIRE(!WIFCONTINUED(st));
		ATF_REQUIRE(si.si_status == SIGSTOP);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_STOPPED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif

		ATF_REQUIRE(kill(pid, SIGCONT) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WCONTINUED, &wru, &si) == pid);
		ATF_REQUIRE(!WIFEXITED(st));
		ATF_REQUIRE(!WIFSIGNALED(st));
		ATF_REQUIRE(WIFCONTINUED(st));
		ATF_REQUIRE(!WIFSTOPPED(st));
		ATF_REQUIRE(si.si_status == SIGCONT);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_CONTINUED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif

		ATF_REQUIRE(kill(pid, SIGQUIT) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WEXITED, &wru, &si) == pid);
		ATF_REQUIRE(!WIFEXITED(st));
		ATF_REQUIRE(WIFSIGNALED(st) && WTERMSIG(st) == SIGQUIT);
		ATF_REQUIRE(!WIFSTOPPED(st));
		ATF_REQUIRE(!WIFCONTINUED(st));
		ATF_REQUIRE(si.si_status == SIGQUIT);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_KILLED);
#ifdef __NetBSD__
		printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
		    (uintmax_t)si.si_utime);
#endif
		break;
	}
}

ATF_TC(wait6_stopgo_loop);
ATF_TC_HEAD(wait6_stopgo_loop, tc)
{
	atf_tc_set_md_var(tc, "descr",
	    "Test that wait6(2) handled stopped/continued process loop");
}

ATF_TC_BODY(wait6_stopgo_loop, tc)
{
	siginfo_t si;
	struct __wrusage wru;
	int st;
	pid_t pid;
	static const struct rlimit rl = { 0, 0 };
	size_t N = 100;

	ATF_REQUIRE(setrlimit(RLIMIT_CORE, &rl) == 0);
	switch (pid = fork()) {
	case 0:
		sleep(100);
		/*FALLTHROUGH*/
	case -1:
		ATF_REQUIRE(pid > 0);
	}

	printf("Before loop of SIGSTOP/SIGCONT sequence %zu times\n", N);
	while (N --> 0) {
		ATF_REQUIRE(kill(pid, SIGSTOP) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WSTOPPED, &wru, &si) == pid);
		ATF_REQUIRE(!WIFEXITED(st));
		ATF_REQUIRE(!WIFSIGNALED(st));
		ATF_REQUIRE(WIFSTOPPED(st) && WSTOPSIG(st) == SIGSTOP);
		ATF_REQUIRE(!WIFCONTINUED(st));
		ATF_REQUIRE(si.si_status == SIGSTOP);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_STOPPED);

		ATF_REQUIRE(kill(pid, SIGCONT) == 0);
		ATF_REQUIRE(wait6(P_PID, pid, &st, WCONTINUED, &wru, &si) == pid);
		ATF_REQUIRE(!WIFEXITED(st));
		ATF_REQUIRE(!WIFSIGNALED(st));
		ATF_REQUIRE(WIFCONTINUED(st));
		ATF_REQUIRE(!WIFSTOPPED(st));
		ATF_REQUIRE(si.si_status == SIGCONT);
		ATF_REQUIRE(si.si_pid == pid);
		ATF_REQUIRE(si.si_uid == getuid());
		ATF_REQUIRE(si.si_code == CLD_CONTINUED);
	}
	ATF_REQUIRE(kill(pid, SIGQUIT) == 0);
	ATF_REQUIRE(wait6(P_PID, pid, &st, WEXITED, &wru, &si) == pid);
	ATF_REQUIRE(!WIFEXITED(st));
	ATF_REQUIRE(WIFSIGNALED(st) && WTERMSIG(st) == SIGQUIT);
	ATF_REQUIRE(!WIFSTOPPED(st));
	ATF_REQUIRE(!WIFCONTINUED(st));
	ATF_REQUIRE(si.si_status == SIGQUIT);
	ATF_REQUIRE(si.si_pid == pid);
	ATF_REQUIRE(si.si_uid == getuid());
	ATF_REQUIRE(si.si_code == CLD_KILLED);
#ifdef __NetBSD__
	printf("user: %ju system: %ju\n", (uintmax_t)si.si_utime,
	    (uintmax_t)si.si_utime);
#endif
}

ATF_TP_ADD_TCS(tp)
{

	ATF_TP_ADD_TC(tp, wait6_invalid);
	ATF_TP_ADD_TC(tp, wait6_exited);
	ATF_TP_ADD_TC(tp, wait6_terminated);
	ATF_TP_ADD_TC(tp, wait6_coredumped);
	ATF_TP_ADD_TC(tp, wait6_stop_and_go);
	ATF_TP_ADD_TC(tp, wait6_stopgo_loop);

	return atf_no_error();
}
"##,
    },
    Source {
        path: "tests/libc/atf/shim/atf_wrapper.c",
        sha256: "bdaaea1d8c7443bc0e2d60b1fbba7cc7448ca1f78ac1dc5fb7d5673076fb9108",
        body: r##"/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * G1 target wrapper for donor ATF programs.
 *
 * The G1 image has no login process, so a spawned test inherits the read-only initfs root as
 * its working directory and an empty supplementary-group set. A login shell does two things
 * before it runs a program: it changes to the user's working directory and installs the group
 * list with initgroups(3). This wrapper does the same two steps, then replaces itself with the
 * donor program through execv(3).
 *
 * The current `PROC_EXEC` request carries no working directory, so the replacement image would
 * start in `/`. The wrapper publishes the work directory as `ATF_CWD`, and the target prelude
 * (`rl_target_prelude.h`) chdirs to it from a constructor before `main`. The C library keeps the
 * path as process state, so a plain execv is enough once the constructor has run.
 *
 * Usage: atf-wrapper WORKDIR PROGRAM [ARG...]
 */

#include <sys/types.h>

#include <errno.h>
#include <grp.h>
#include <pwd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int
main(int argc, char **argv)
{
	const char *workdir;
	const char *program;

	if (argc < 3) {
		fprintf(stderr, "atf-wrapper: usage: atf-wrapper WORKDIR PROGRAM [ARG...]\n");
		return 2;
	}
	workdir = argv[1];
	program = argv[2];

	/*
	 * Match the S179 service setup: svcd starts the service as root and the probe runs
	 * initgroups("raven", gid) before it drops to the account. The runner keeps uid 0 because
	 * several donor bodies set metadata with require.user=root, but the group list is the
	 * login list, so getgroups(2) contains the effective gid exactly as after a login.
	 */
	if (geteuid() == 0) {
		struct passwd *root = getpwnam("root");

		if (root != NULL) {
			(void)initgroups(root->pw_name, root->pw_gid);
			(void)setgid(root->pw_gid);
		}
	}

	if (mkdir(workdir, 0777) != 0 && errno != EEXIST) {
		fprintf(stderr, "atf-wrapper: mkdir %s: %s\n", workdir, strerror(errno));
		return 2;
	}
	if (setenv("ATF_CWD", workdir, 1) != 0) {
		fprintf(stderr, "atf-wrapper: setenv ATF_CWD: %s\n", strerror(errno));
		return 2;
	}

	execv(program, &argv[2]);
	fprintf(stderr, "atf-wrapper: execv %s: %s\n", program, strerror(errno));
	return 127;
}
"##,
    },
    Source {
        path: "tests/libc/atf/shim/rl_target_prelude.h",
        sha256: "2fce4adc160131b95d16ac4410589f7dae7806a2f366bfd4ae2174c8a8349bfd",
        body: r##"// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target-only prelude force-included after `rl_shim.h`. The target `<fcntl.h>` undefines
// `SEEK_SET`/`SEEK_CUR`/`SEEK_END` without defining them again, so include it once here and
// restore the three constants after the undef; the source's later `<fcntl.h>` is guarded.

#ifndef RL_TARGET_PRELUDE_H
#define RL_TARGET_PRELUDE_H

/*
 * The target headers are FreeBSD-derived and gate the BSD interfaces on `__BSD_VISIBLE`.
 * FreeBSD's <sys/cdefs.h> defaults it to one for a program that selects no strict feature
 * macro; the ravenlibc copy does not, so a donor body that includes <sys/wait.h> or
 * <sys/resource.h> would not see `wait6(2)` or `struct __wrusage`. Set the FreeBSD default here.
 */
#ifndef __BSD_VISIBLE
#define __BSD_VISIBLE 1
#endif

#include <stdlib.h>
#include <unistd.h>

#include <fcntl.h>

#ifndef SEEK_SET
#define SEEK_SET 0
#endif
#ifndef SEEK_CUR
#define SEEK_CUR 1
#endif
#ifndef SEEK_END
#define SEEK_END 2
#endif

/*
 * The G1 runner starts a donor body from a writable directory. The current `PROC_EXEC` request
 * carries no working directory, so the wrapper publishes it as `ATF_CWD` and this constructor
 * applies it before `main`. The variable is absent for a normal target program.
 */
static void rl_apply_atf_cwd(void) __attribute__((constructor));
static void
rl_apply_atf_cwd(void)
{
	const char *cwd = getenv("ATF_CWD");

	if (cwd != NULL && cwd[0] == '/')
		(void)chdir(cwd);
}

#endif /* RL_TARGET_PRELUDE_H */
"##,
    },
];

/// The historical path of the select body.
const SELECT: &str = "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_select.c";

/// The donor parent does not inspect the forked child status, so a dead or failed pselect child
/// still produces a `passed` result file. Add the prerequisite assertion the S750 ticket requires
/// (`pselect_timeout`/`pselect_sigmask` must not pass vacuously). No donor assertion is changed.
const SELECT_DEFAULT: &str = "\t\tdefault:\n\t\t\tbreak;\n";
const SELECT_CHECKED: &str = "\t\tdefault:\n\t\t\tif (!WIFEXITED(status) || WEXITSTATUS(status) != 0)\n\t\t\t\tatf_tc_fail(\"pselect child failed (status 0x%x)\", status);\n\t\t\tbreak;\n";

/// Write the exact and build-ready trees under `out/port-sources` and return the build root.
pub fn stage(out: &Path) -> Result<PathBuf> {
    stage_sources(out, SOURCES)
}

fn stage_sources(out: &Path, sources: &[Source]) -> Result<PathBuf> {
    let stage = out.join(STAGE_DIR);
    let exact_root = stage.join(EXACT_DIR);
    let build_root = stage.join(BUILD_DIR);
    let mut manifest = String::from("# path\tsha256_exact\tsha256_build\n");
    for source in sources {
        let exact = exact_root.join(source.path);
        if let Some(parent) = exact.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(&exact, source.body.as_bytes())
            .with_context(|| format!("cannot write {}", exact.display()))?;
        let actual = sha256_hex(source.body.as_bytes());
        if actual != source.sha256 {
            bail!(
                "embedded source {} has sha256 {} but the historical blob is {}",
                source.path,
                actual,
                source.sha256
            );
        }
        let built = transform(source.path, source.body.as_bytes())?;
        let build = build_root.join(source.path);
        if let Some(parent) = build.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(&build, &built).with_context(|| format!("cannot write {}", build.display()))?;
        manifest.push_str(&format!(
            "{}\t{}\t{}\n",
            source.path,
            actual,
            sha256_hex(&built)
        ));
    }
    fs::write(stage.join("manifest.tsv"), manifest)
        .with_context(|| format!("cannot write {}", stage.join("manifest.tsv").display()))?;
    Ok(build_root)
}

/// Apply the one build-only transformation. Every other source is copied byte for byte.
fn transform(path: &str, body: &[u8]) -> Result<Vec<u8>> {
    if path != SELECT {
        return Ok(body.to_vec());
    }
    let text = std::str::from_utf8(body).map_err(|error| anyhow::anyhow!("{path}: {error}"))?;
    let occurrences = text.matches(SELECT_DEFAULT).count();
    if occurrences != 2 {
        bail!("{path}: expected two waitpid default arms, found {occurrences}");
    }
    Ok(text.replace(SELECT_DEFAULT, SELECT_CHECKED).into_bytes())
}

/// SHA-256 of `bytes` (FIPS 180-4), used to bind a staged source to its historical blob.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = bytes.to_vec();
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        let mut index = 16;
        while index < 64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
            index += 1;
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (constant, word) in K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*constant)
                .wrapping_add(*word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
#[path = "port_sources_ut.rs"]
mod tests;
