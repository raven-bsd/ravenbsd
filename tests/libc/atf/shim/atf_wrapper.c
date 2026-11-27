/* SPDX-License-Identifier: BSD-3-Clause
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
