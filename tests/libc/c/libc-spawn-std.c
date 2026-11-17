/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * P2r libc native-spawn command name: posix_spawn /bin/libc-child with argv[0]
 * "libc-spawn-std-child" and check both the parent's PROC_PID_INFO view and the child's
 * PROC_SELF_INFO view.
 */

#define RL_CASE "libc-spawn-std"
#include "rl_test.h"

#include <spawn.h>
#include <sys/wait.h>
#include <string.h>
#include <unistd.h>

#if defined(__x86_64__)
static long rl_proc_pid_info(int pid, unsigned long long *words, unsigned long count)
{
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(0x3AL), "D"((long)pid), "S"(words), "d"(count)
                     : "rcx", "r11", "memory");
    return result;
}
#else
#error "libc-spawn-std is an x86_64 case"
#endif

static int command_is(const unsigned long long *words, const char *want)
{
    return strcmp((const char *)&words[8], want) == 0;
}

int main(void)
{
    char *const argv[] = { "libc-spawn-std-child", NULL };
    char *const envp[] = { NULL };
    posix_spawnattr_t attr;
    unsigned long long words[41] = { 0 };
    pid_t pid = -1;
    int status = 0;
    int result;

    rl_out("libc-spawn-std: start");
    RL_REQUIRE(posix_spawnattr_init(&attr) == 0);
    (void)posix_spawnattr_destroy(&attr);
    result = posix_spawn(&pid, "/bin/libc-child", NULL, NULL, argv, envp);
    RL_REQUIRE(result == 0 && pid > 0);
    RL_CHECK("parent cmd",
             rl_proc_pid_info(pid, words, 41) == 0 && command_is(words, "libc-spawn-std-child"));
    RL_CHECK("wait", waitpid(pid, &status, 0) == pid && WIFEXITED(status) &&
                         WEXITSTATUS(status) == 0);
    return rl_finish();
}
