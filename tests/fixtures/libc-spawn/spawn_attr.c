/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * P9c: posix_spawn with non-NULL attribute and file-action blobs, the shape
 * std::process::Command uses. The C73 libc-spawn case passes NULL for both.
 */

#define RL_CASE "spawn-attr"
#include "rl_test.h"

#include <spawn.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static char *const child_argv[] = { "libc-child", "a", "b c", NULL };
static char *const child_env[] = { "RL=1", "RL_EXIT=7", NULL };

int main(void)
{
    posix_spawnattr_t attr;
    posix_spawn_file_actions_t actions;
    sigset_t set;
    pid_t pid = -1;
    int status = 0;
    int result;

    rl_out("spawn-attr: start");
    RL_CHECK("attr init", posix_spawnattr_init(&attr) == 0);
    RL_CHECK("actions init", posix_spawn_file_actions_init(&actions) == 0);
    RL_CHECK("empty set", sigemptyset(&set) == 0);
    RL_CHECK("sigdefault", posix_spawnattr_setsigdefault(&attr, &set) == 0);
    RL_CHECK("setflags", posix_spawnattr_setflags(&attr, 0) == 0);
    RL_CHECK("adddup2", posix_spawn_file_actions_adddup2(&actions, 1, 1) == 0);

    result = posix_spawnp(&pid, "/bin/libc-child", &actions, &attr, child_argv, child_env);
    RL_CHECK("spawn", result == 0 && pid > 0);
    if (result == 0 && waitpid(pid, &status, 0) == pid) {
        RL_CHECK("wait", WIFEXITED(status) && WEXITSTATUS(status) == 7);
    } else {
        rl_fail("wait", __FILE__, __LINE__);
    }
    RL_CHECK("actions destroy", posix_spawn_file_actions_destroy(&actions) == 0);
    RL_CHECK("attr destroy", posix_spawnattr_destroy(&attr) == 0);
    rl_out("spawn-attr: done");
    return rl_finish();
}
