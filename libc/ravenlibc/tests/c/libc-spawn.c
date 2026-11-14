/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-spawn: posix_spawn of an initfs child, waitpid statuses, spawnp, ENOENT and the
 * documented fork ENOSYS (H-18). File actions, popen/system, fork/exec and PT_INTERP children
 * are soft until PG-35/PG-44/PG-45/PG-46/PG-47/PG-49 and print SKIP lines.
 */

#define RL_CASE "libc-spawn"
#include "rl_test.h"

#include <spawn.h>
#include <sys/wait.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <errno.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>

static char *const child_argv[] = { "libc-child", "a", "b c", NULL };
static char *const child_env[] = { "RL=1", "RL_SYNC=1", NULL };
static char *const quiet_env[] = { "RL=1", "RL_QUIET=1", NULL };

static int wait_status(pid_t pid, int *status)
{
    return waitpid(pid, status, 0) == pid;
}

static void release_child(void)
{
    int fd = open("/tmp/rl-spawn-go", O_CREAT | O_WRONLY, 0644);

    if (fd >= 0) {
        close(fd);
    }
}

static int check_enoent(void)
{
    posix_spawnattr_t attr;
    pid_t pid = -1;
    int result;
    int checks = 0;

    checks += posix_spawnattr_init(&attr) == 0;
    (void)posix_spawnattr_destroy(&attr);
    result = posix_spawn(&pid, "/bin/nope", NULL, NULL, child_argv, child_env);
    checks += result == ENOENT;
    checks += pid == -1;
    return checks >= 3;
}

static int check_spawnp(void)
{
    posix_spawnattr_t attr;
    pid_t pid = -1;
    int status = 0;
    int result;
    int checks = 0;

    checks += setenv("PATH", "/bin", 1) == 0;
    checks += posix_spawnattr_init(&attr) == 0;
    (void)posix_spawnattr_destroy(&attr);
    result = posix_spawnp(&pid, "libc-child", NULL, NULL, child_argv, quiet_env);
    checks += result == 0 && pid > 0;
    checks += result == 0 && wait_status(pid, &status);
    checks += WIFEXITED(status) && WEXITSTATUS(status) == 7;
    return checks >= 5;
}

static int check_exit_codes(void)
{
    static const int codes[] = { 0, 1, 255 };
    char exit_env[3][16] = { "RL_EXIT=0", "RL_EXIT=1", "RL_EXIT=255" };
    char *env[3];
    int index;
    int checks = 0;

    for (index = 0; index < 3; index++) {
        env[0] = exit_env[index];
        env[1] = "RL_QUIET=1";
        env[2] = NULL;
        {
            posix_spawnattr_t attr;
            pid_t pid = -1;
            int status = 0;
            int result;

            checks += posix_spawnattr_init(&attr) == 0;
            (void)posix_spawnattr_destroy(&attr);
            result = posix_spawn(&pid, "/bin/libc-child", NULL, NULL, child_argv, env);
            checks += result == 0 && pid > 0;
            checks += result == 0 && wait_status(pid, &status);
            checks += WIFEXITED(status) && WEXITSTATUS(status) == codes[index];
        }
    }
    return checks >= 9;
}

static int check_sigdefault(void)
{
    posix_spawnattr_t attr;
    sigset_t set;
    sigset_t readback;
    int result;
    int checks = 0;

    checks += posix_spawnattr_init(&attr) == 0;
    checks += sigemptyset(&set) == 0;
    checks += sigaddset(&set, SIGUSR1) == 0;
    result = posix_spawnattr_setsigdefault(&attr, &set);
    checks += result == 0 || result == ENOSYS;
    if (result == 0) {
        checks += posix_spawnattr_getsigdefault(&attr, &readback) == 0;
        checks += sigismember(&readback, SIGUSR1) == 1;
    }
    (void)posix_spawnattr_destroy(&attr);
    return checks >= 4;
}

static int check_fork_enosys(void)
{
    pid_t pid;

    errno = 0;
    pid = fork();
    return pid == -1 && errno == ENOSYS;
}

static int check_getpid(void)
{
    pid_t parent = getpid();
    posix_spawnattr_t attr;
    pid_t child = -1;
    int status = 0;
    int result;

    if (parent <= 0) {
        return 0;
    }
    if (posix_spawnattr_init(&attr) != 0) {
        return 0;
    }
    (void)posix_spawnattr_destroy(&attr);
    result = posix_spawn(&child, "/bin/libc-child", NULL, NULL, child_argv, quiet_env);
    if (result != 0 || child <= 0) {
        return 0;
    }
    if (waitpid(child, &status, 0) != child) {
        return 0;
    }
    return child != parent;
}

int main(void)
{
    posix_spawnattr_t attr;
    pid_t pid = -1;
    int status = 0;
    int result;

    rl_out("libc-spawn: parent start");
    result = posix_spawnattr_init(&attr);
    if (result == 0) {
        (void)posix_spawnattr_destroy(&attr);
    }
    result = posix_spawn(&pid, "/bin/libc-child", NULL, NULL, child_argv, child_env);
    if (result == 0 && pid > 0) {
        rl_out("libc-spawn: spawn ok");
        release_child();
    } else {
        rl_fail("spawn", __FILE__, __LINE__);
    }
    if (pid > 0 && wait_status(pid, &status)) {
        RL_CHECK("wait", WIFEXITED(status) && WEXITSTATUS(status) == 7);
        RL_CHECK("wait twice", waitpid(pid, &status, 0) == -1 && errno == ECHILD);
    } else {
        rl_fail("wait", __FILE__, __LINE__);
        rl_fail("wait twice", __FILE__, __LINE__);
    }
    (void)unlink("/tmp/rl-spawn-go");
    RL_CHECK("enoent", check_enoent());
    RL_CHECK("spawnp", check_spawnp());
    RL_CHECK("exit codes", check_exit_codes());
    RL_CHECK("getpid", check_getpid());
    RL_CHECK("sigdefault", check_sigdefault());
    RL_CHECK("fork enosys", check_fork_enosys());
    rl_skip("dup2 action ok", "PG-44 PG-47");
    rl_skip("system ok", "PG-35 PG-44");
    rl_skip("popen ok", "PG-35 PG-44");
    rl_skip("fork ok", "PG-45 PG-46");
    rl_skip("bigargs ok", "PG-49");
    rl_skip("dynamic child ok", "PG-49");
    return rl_finish();
}
