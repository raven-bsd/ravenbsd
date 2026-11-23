// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K548 rev341: the static-program exec/startup split for Ken's pre-banner buckets.
//
// The parent records CLOCK_MONOTONIC before fork, after fork returns in the parent, and after
// waitpid. The child is this same static binary re-exec'd; its earliest constructor records the
// post-libc-init point, main records entry, and the first write records the first-io point. All
// values are absolute guest monotonic nanoseconds, so the host can compute:
//   fork = exec_ret - fork_t
//   exec_to_ctor = ctor - fork_t      (kernel execve + ELF load + first faults + libc init)
//   ctor_to_main = main - ctor
//   main_to_io = io - main
//   child_start = io - fork_t
// The fixture prints the raw values; it does not claim a policy.

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static uint64_t ctor_ns;

__attribute__((constructor)) static void exec_timing_ctor(void) {
    ctor_ns = now_ns();
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "child") == 0) {
        uint64_t main_ns = now_ns();
        char line[192];
        int length = snprintf(line, sizeof(line), "EXEC-TIMING child ctor=%llu main=%llu pid=%d\n",
                              (unsigned long long)ctor_ns, (unsigned long long)main_ns,
                              (int)getpid());
        uint64_t io_ns = now_ns();
        (void)write(1, line, (size_t)length);
        length = snprintf(line, sizeof(line), "EXEC-TIMING child io=%llu\n",
                          (unsigned long long)io_ns);
        (void)write(1, line, (size_t)length);
        return 0;
    }

    uint64_t fork_ns = now_ns();
    pid_t child = fork();
    if (child == 0) {
        char *args[] = {"libc-test", "child", NULL};
        char *env[] = {NULL};
        (void)execve("/bin/libc-test", args, env);
        _exit(127);
    }
    uint64_t exec_ret_ns = now_ns();
    int status = 0;
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    uint64_t wait_ns = now_ns();
    printf("EXEC-TIMING parent fork=%llu exec_ret=%llu wait=%llu pid=%d status=%d\n",
           (unsigned long long)fork_ns, (unsigned long long)exec_ret_ns,
           (unsigned long long)wait_ns, (int)child, status);
    if (child > 0) {
        printf("EXEC-TIMING fork_ns=%llu\n", (unsigned long long)(exec_ret_ns - fork_ns));
        printf("EXEC-TIMING total_ns=%llu\n", (unsigned long long)(wait_ns - fork_ns));
    }
    fflush(stdout);
    return child < 0 ? 1 : 0;
}
