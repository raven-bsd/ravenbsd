/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * G1 target wrapper for donor ATF programs.
 *
 * The G1 image has no login process, so a spawned test inherits the read-only initfs root as
 * its working directory and an empty supplementary-group set. A login shell does two things
 * before it runs a program: it changes to the user's working directory and installs the group
 * list with initgroups(3). This wrapper does the same two steps, then starts the donor program
 * through posix_spawn(3) with a chdir file action and waits for it.
 *
 * execve(2) is not used: the current `PROC_EXEC` request carries no working directory, so the
 * replacement image would fall back to `/`. The spawn path carries the cwd and publishes it to
 * the new image.
 *
 * Usage: atf-wrapper WORKDIR PROGRAM [ARG...]
 */

#include <sys/types.h>
#include <sys/wait.h>

#include <errno.h>
#include <grp.h>
#include <pwd.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

extern char **environ;

int
main(int argc, char **argv)
{
	posix_spawn_file_actions_t actions;
	const char *workdir;
	const char *program;
	pid_t pid;
	int status;
	int error;

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

	error = posix_spawn_file_actions_init(&actions);
	if (error != 0) {
		fprintf(stderr, "atf-wrapper: file actions init: %s\n", strerror(error));
		return 2;
	}
	error = posix_spawn_file_actions_addchdir_np(&actions, workdir);
	if (error == 0) {
		error = posix_spawn(&pid, program, &actions, NULL, &argv[2], environ);
	}
	(void)posix_spawn_file_actions_destroy(&actions);
	if (error != 0) {
		fprintf(stderr, "atf-wrapper: spawn %s: %s\n", program, strerror(error));
		return 2;
	}

	if (waitpid(pid, &status, 0) != pid) {
		fprintf(stderr, "atf-wrapper: waitpid %ld: %s\n", (long)pid, strerror(errno));
		return 2;
	}
	if (WIFEXITED(status)) {
		return WEXITSTATUS(status);
	}
	if (WIFSIGNALED(status)) {
		return 128 + WTERMSIG(status);
	}
	return 2;
}
