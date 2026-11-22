/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K502 ASLR probe. The dynamic PIE parent execs itself 20 times; each child reports its stack
 * top, first anonymous mmap base, PIE load base and interpreter base through a pipe. The parent
 * checks page alignment, the documented windows and the spread, and prints one table row per
 * run. A no-exec fork row proves the child inherits the parent layout. With `aslr=off` all runs
 * are identical and the probe prints `aslr: fixed-layout` instead of `aslr: pass`; the fixed
 * qtest case expects that line and the normal case does not. */

#include <dlfcn.h>
#include <elf.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/auxv.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define RUNS 20
#define PAGE 4096UL
#define USER_END 0x00007ffffffff000UL
#define STACK_SIZE 262144UL
#define STACK_GUARD 4096UL
#define STACK_WINDOW (8UL * 1024 * 1024)
#define MMAP_BASE 0x0000500000000000UL
#define MMAP_WINDOW 0x40000000UL
#define PIE_BASE 0x0000100000000000UL
#define PIE_WINDOW 0x40000000UL
#define INTERP_BASE 0x0000200000000000UL
#define INTERP_WINDOW 0x40000000UL

struct layout {
    uint64_t stack;
    uint64_t mmap_base;
    uint64_t pie_base;
    uint64_t interp_base;
};

int main(int argc, char **argv);

static int page_aligned(uint64_t value) {
    return value != 0 && (value & (PAGE - 1)) == 0;
}

static int layout_of(struct layout *layout) {
    // The kernel-provided stack is the live frame stack; the libc main-thread attr falls back to
    // an 8 MiB window under the TCB because the spawn stack base is not wired yet (T-thr-20).
    layout->stack = (uint64_t)(uintptr_t)__builtin_frame_address(0);
    void *mapped = mmap(NULL, PAGE, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (mapped == MAP_FAILED)
        return -1;
    munmap(mapped, PAGE);
    Dl_info info;
    if (dladdr((void *)(uintptr_t)&main, &info) == 0 || info.dli_fbase == NULL)
        return -1;
    layout->mmap_base = (uint64_t)(uintptr_t)mapped;
    layout->pie_base = (uint64_t)(uintptr_t)info.dli_fbase;
    layout->interp_base = (uint64_t)getauxval(AT_BASE);
    return 0;
}

static int write_all(int fd, const void *buffer, size_t length) {
    size_t total = 0;
    while (total < length) {
        ssize_t count = write(fd, (const char *)buffer + total, length - total);
        if (count <= 0) {
            if (count < 0 && (errno == EINTR || errno == EAGAIN))
                continue;
            return -1;
        }
        total += (size_t)count;
    }
    return 0;
}

static int read_all(int fd, void *buffer, size_t length) {
    size_t total = 0;
    while (total < length) {
        ssize_t count = read(fd, (char *)buffer + total, length - total);
        if (count <= 0) {
            if (count < 0 && (errno == EINTR || errno == EAGAIN))
                continue;
            return -1;
        }
        total += (size_t)count;
    }
    return 0;
}

static int in_window(uint64_t value, uint64_t base, uint64_t window) {
    return value >= base && value - base < window;
}

static int window_ok(const struct layout *layout, char *why, size_t why_len) {
    uint64_t window_end = USER_END - STACK_GUARD;
    uint64_t window_start = window_end - (STACK_WINDOW - PAGE) - STACK_SIZE;
    if (layout->stack < window_start || layout->stack >= window_end) {
        snprintf(why, why_len, "stack=%#lx", layout->stack);
        return 0;
    }
    if (!page_aligned(layout->mmap_base) ||
        !in_window(layout->mmap_base, MMAP_BASE, MMAP_WINDOW)) {
        snprintf(why, why_len, "mmap=%#lx", layout->mmap_base);
        return 0;
    }
    if (!page_aligned(layout->pie_base) || !in_window(layout->pie_base, PIE_BASE, PIE_WINDOW)) {
        snprintf(why, why_len, "pie=%#lx", layout->pie_base);
        return 0;
    }
    if (!page_aligned(layout->interp_base) ||
        !in_window(layout->interp_base, INTERP_BASE, INTERP_WINDOW)) {
        snprintf(why, why_len, "interp=%#lx", layout->interp_base);
        return 0;
    }
    return 1;
}

static int same_layout(const struct layout *left, const struct layout *right) {
    // The fork child calls layout_of from a different frame depth; the stack VMA is the same,
    // so compare the containing page plus the three exact bases.
    return (left->stack >> 12) == (right->stack >> 12) &&
           left->mmap_base == right->mmap_base && left->pie_base == right->pie_base &&
           left->interp_base == right->interp_base;
}

static int child_main(int fd) {
    struct layout layout;
    if (layout_of(&layout) != 0)
        return 1;
    if (write_all(fd, &layout, sizeof(layout)) != 0)
        return 1;
    return 0;
}

static int distinct(const struct layout *runs, uint64_t (*field)(const struct layout *)) {
    int count = 0;
    for (int i = 0; i < RUNS; i++) {
        int seen = 0;
        for (int j = 0; j < i; j++) {
            if (field(&runs[j]) == field(&runs[i])) {
                seen = 1;
                break;
            }
        }
        if (!seen)
            count++;
    }
    return count;
}

static uint64_t stack_field(const struct layout *layout) { return layout->stack; }
static uint64_t mmap_field(const struct layout *layout) { return layout->mmap_base; }
static uint64_t pie_field(const struct layout *layout) { return layout->pie_base; }
static uint64_t interp_field(const struct layout *layout) { return layout->interp_base; }

int main(int argc, char **argv) {
    if (argc == 3 && strcmp(argv[1], "child") == 0)
        return child_main(atoi(argv[2]));

    struct layout parent;
    if (layout_of(&parent) != 0) {
        printf("aslr: FAIL parent layout\n");
        return 1;
    }
    struct layout parent_again;
    if (layout_of(&parent_again) != 0 || !same_layout(&parent, &parent_again)) {
        printf("aslr: FAIL stable layout changed\n");
        return 1;
    }
    printf("aslr: stable same=1\n");

    int fork_pipe[2];
    if (pipe(fork_pipe) != 0) {
        printf("aslr: FAIL fork pipe errno=%d\n", errno);
        return 1;
    }
    pid_t fork_pid = fork();
    if (fork_pid == 0) {
        close(fork_pipe[0]);
        _exit(child_main(fork_pipe[1]));
    }
    close(fork_pipe[1]);
    struct layout inherited;
    if (read_all(fork_pipe[0], &inherited, sizeof(inherited)) != 0) {
        printf("aslr: FAIL fork read errno=%d\n", errno);
        return 1;
    }
    close(fork_pipe[0]);
    int status = 0;
    if (waitpid(fork_pid, &status, 0) != fork_pid || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        printf("aslr: FAIL fork child status=%d\n", status);
        return 1;
    }
    if (!same_layout(&parent, &inherited)) {
        printf("aslr: FAIL fork layout changed\n");
        return 1;
    }
    printf("aslr: fork inherits same=1\n");

    struct layout runs[RUNS];
    for (int run = 0; run < RUNS; run++) {
        int pipefd[2];
        if (pipe(pipefd) != 0) {
            printf("aslr: FAIL run pipe=%d errno=%d\n", run, errno);
            return 1;
        }
        char fd_text[16];
        snprintf(fd_text, sizeof(fd_text), "%d", pipefd[1]);
        pid_t pid = fork();
        if (pid == 0) {
            close(pipefd[0]);
            char *args[] = { "/bin/libc-test", "child", fd_text, NULL };
            execv(args[0], args);
            _exit(1);
        }
        if (pid < 0) {
            printf("aslr: FAIL run fork=%d errno=%d\n", run, errno);
            return 1;
        }
        close(pipefd[1]);
        if (read_all(pipefd[0], &runs[run], sizeof(runs[run])) != 0) {
            printf("aslr: FAIL run read=%d errno=%d\n", run, errno);
            return 1;
        }
        close(pipefd[0]);
        if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status) ||
            WEXITSTATUS(status) != 0) {
            printf("aslr: FAIL run child=%d status=%d\n", run, status);
            return 1;
        }
    }

    char why[128];
    int ok = 1;
    for (int run = 0; run < RUNS; run++) {
        printf("aslr: run=%d stack=0x%lx mmap=0x%lx pie=0x%lx interp=0x%lx\n", run,
               runs[run].stack, runs[run].mmap_base, runs[run].pie_base,
               runs[run].interp_base);
        if (!window_ok(&runs[run], why, sizeof(why))) {
            printf("aslr: FAIL run=%d %s\n", run, why);
            ok = 0;
        }
    }

    int stack_count = distinct(runs, stack_field);
    int mmap_count = distinct(runs, mmap_field);
    int pie_count = distinct(runs, pie_field);
    int interp_count = distinct(runs, interp_field);
    printf("aslr: summary distinct-stack=%d distinct-mmap=%d distinct-pie=%d distinct-interp=%d\n",
           stack_count, mmap_count, pie_count, interp_count);

    if (!ok)
        return 1;
    if (stack_count == 1 && mmap_count == 1 && pie_count == 1 && interp_count == 1) {
        printf("aslr: fixed-layout runs=%d\n", RUNS);
        return 0;
    }
    if (stack_count < 2 || mmap_count < 2 || pie_count < 2 || interp_count < 2) {
        printf("aslr: FAIL partial-spread\n");
        return 1;
    }
    printf("aslr: pass runs=%d\n", RUNS);
    return 0;
}
