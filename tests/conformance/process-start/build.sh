#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S714 process-start conformance image. The probe separates the three real startup paths on
# target: a fresh-image posix_spawn, a fork child that resumes the caller's saved frame, and a
# same-PID exec. Fork rows assert parent/child stack isolation after a parent write and after
# the parent exits without reaping the child.
#
# The C source is generated into the build tree, following the S177/S198 conformance layout:
# no C source is committed to the scaffold. Only files under
# tests/conformance/process-start/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/process-start/build.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=$root/build/conformance/process-start
stage=$root/build/libc-test
mkdir -p "$out" "$stage"

cat > "$out/process_start.c" <<'PROCESS_START_C'
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S714 process-start conformance probe. Rows are printed as `S714-RNN name ... ok=<0|1>`; the
// runner requires every row and the final `S714-PROCESS-START ok` marker.
//
// POSIX fork/posix_spawn/exec semantics; FreeBSD sys/kern/kern_fork.c and NetBSD
// sys/uvm/uvm_map.c/uvm_fault.c are the behaviour oracles. A forked child must resume the
// caller's saved frame (FreeBSD kern_fork.c cpu_fork saves the caller context) and must keep a
// private copy of every writable parent page, including the one holding that frame (NetBSD
// uvm_map.c uvm_mapent_forkcopy write-protects the parent). No donor code is copied.

#include <errno.h>
#include <signal.h>
#include <spawn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

extern char **environ;

#define SELF "/bin/libc-test"
#define CANARY_WORDS 8u
#define CANARY_A 0x5a17e7c4d3b2a190ULL
#define CANARY_B 0xa5e81b3c2c4d5e6fULL
#define REPORT_MAGIC 0x53373134u /* "S714" */

struct child_report {
    uint32_t magic;
    uint32_t canary_ok;
    uint32_t handler_ok;
    uint32_t ppid_ok;
    int32_t pid;
    int32_t ppid;
};

static volatile sig_atomic_t usr1_seen;

static void on_usr1(int signo) {
    (void)signo;
    usr1_seen = 1;
}

static int read_full(int fd, void *buffer, size_t len) {
    size_t done = 0;
    while (done < len) {
        ssize_t got = read(fd, (char *)buffer + done, len - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int write_full(int fd, const void *buffer, size_t len) {
    size_t done = 0;
    while (done < len) {
        ssize_t put = write(fd, (const char *)buffer + done, len - done);
        if (put <= 0) {
            return -1;
        }
        done += (size_t)put;
    }
    return 0;
}

static void canary_fill(volatile uint64_t *words, uint64_t base) {
    for (unsigned index = 0; index < CANARY_WORDS; index++) {
        words[index] = base ^ (uint64_t)index;
    }
}

static int canary_matches(const volatile uint64_t *words, uint64_t base) {
    for (unsigned index = 0; index < CANARY_WORDS; index++) {
        if (words[index] != (base ^ (uint64_t)index)) {
            return 0;
        }
    }
    return 1;
}

static struct child_report make_report(const volatile uint64_t *canary,
                                       int handler_ok, int ppid_ok) {
    struct child_report report;
    memset(&report, 0, sizeof(report));
    report.magic = REPORT_MAGIC;
    report.canary_ok = canary_matches(canary, CANARY_A) ? 1u : 0u;
    report.handler_ok = handler_ok ? 1u : 0u;
    report.ppid_ok = ppid_ok ? 1u : 0u;
    report.pid = (int32_t)getpid();
    report.ppid = (int32_t)getppid();
    return report;
}

// R01: posix_spawn creates a fresh image directly, without a fork frame.
static int row_fresh_spawn(void) {
    char *argv[] = {(char *)SELF, (char *)"--spawn-child", NULL};
    pid_t pid = -1;
    int rc = posix_spawn(&pid, SELF, NULL, NULL, argv, environ);
    if (rc != 0) {
        printf("S714-R01 fresh-spawn rc=%d ok=0\n", rc);
        return 1;
    }
    int status = 0;
    if (waitpid(pid, &status, 0) != pid) {
        printf("S714-R01 fresh-spawn pid=%d wait=0 ok=0\n", (int)pid);
        return 1;
    }
    int ok = WIFEXITED(status) && WEXITSTATUS(status) == 0;
    printf("S714-R01 fresh-spawn pid=%d status=%d ok=%d\n", (int)pid, status, ok);
    return ok ? 0 : 1;
}

// R02: the child resumes the caller's saved frame, and the parent's post-fork write to the same
// stack page must not become visible in the child.
static int row_fork_resume(void) {
    int ready[2];
    int go[2];
    if (pipe(ready) != 0 || pipe(go) != 0) {
        printf("S714-R02 fork-resume pipe=0 ok=0\n");
        return 1;
    }
    volatile uint64_t canary[CANARY_WORDS];
    canary_fill(canary, CANARY_A);
    pid_t parent = getpid();
    pid_t pid = fork();
    if (pid < 0) {
        printf("S714-R02 fork-resume fork=%d ok=0\n", errno);
        return 1;
    }
    if (pid == 0) {
        close(ready[0]);
        close(go[1]);
        char token = 'r';
        if (write_full(ready[1], &token, 1) != 0 || read_full(go[0], &token, 1) != 0) {
            _exit(4);
        }
        struct child_report report = make_report(canary, 1, getppid() == parent);
        if (write_full(ready[1], &report, sizeof(report)) != 0) {
            _exit(5);
        }
        close(ready[1]);
        close(go[0]);
        _exit(report.canary_ok && report.ppid_ok ? 0 : 3);
    }
    close(ready[1]);
    close(go[0]);
    char token = 0;
    int ok = read_full(ready[0], &token, 1) == 0;
    // The parent now writes its own copy of the page that holds the child's saved frame.
    canary_fill(canary, CANARY_B);
    volatile uint64_t spill[CANARY_WORDS];
    canary_fill(spill, CANARY_B);
    token = 'g';
    if (write_full(go[1], &token, 1) != 0) {
        ok = 0;
    }
    struct child_report report;
    memset(&report, 0, sizeof(report));
    if (read_full(ready[0], &report, sizeof(report)) != 0) {
        ok = 0;
    }
    int status = 0;
    pid_t reaped = waitpid(pid, &status, 0);
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    ok = ok && report.magic == REPORT_MAGIC && report.canary_ok && report.ppid_ok &&
         reaped == pid && child_exit == 0;
    int parent_copy = canary_matches(canary, CANARY_B) && canary_matches(spill, CANARY_B);
    printf("S714-R02 fork-resume parent=%d child=%d canary=%u ppid=%u parent-copy=%d status=%d ok=%d\n",
           (int)parent, (int)report.pid, report.canary_ok, report.ppid_ok, parent_copy, child_exit,
           ok && parent_copy);
    close(ready[0]);
    close(go[1]);
    return (ok && parent_copy) ? 0 : 1;
}

// R03: the forking parent exits immediately without reaping the child. The child must still see
// its own saved frame after the parent's address-space teardown.
static int row_parent_exit(void) {
    int status_pipe[2];
    if (pipe(status_pipe) != 0) {
        printf("S714-R03 parent-exit pipe=0 ok=0\n");
        return 1;
    }
    pid_t middle = fork();
    if (middle < 0) {
        printf("S714-R03 parent-exit fork=%d ok=0\n", errno);
        return 1;
    }
    if (middle == 0) {
        close(status_pipe[0]);
        volatile uint64_t canary[CANARY_WORDS];
        canary_fill(canary, CANARY_A);
        pid_t child = fork();
        if (child == 0) {
            // Give the exiting parent's teardown time to run before the check.
            struct timespec delay = {.tv_sec = 0, .tv_nsec = 200 * 1000 * 1000};
            (void)nanosleep(&delay, NULL);
            // The orphan is reparented to init only after the parent's teardown completed.
            struct child_report report = make_report(canary, 1, getppid() == 1);
            (void)write_full(status_pipe[1], &report, sizeof(report));
            _exit(0);
        }
        if (child < 0) {
            _exit(6);
        }
        // The parent leaves immediately; it neither waits nor closes the inherited write end.
        _exit(0);
    }
    close(status_pipe[1]);
    int status = 0;
    (void)waitpid(middle, &status, 0);
    struct child_report report;
    memset(&report, 0, sizeof(report));
    int ok = read_full(status_pipe[0], &report, sizeof(report)) == 0;
    ok = ok && report.magic == REPORT_MAGIC && report.canary_ok && report.ppid_ok;
    printf("S714-R03 parent-exit parent=%d child=%d canary=%u reparented=%u ok=%d\n",
           (int)middle, (int)report.pid, report.canary_ok, report.ppid_ok, ok);
    close(status_pipe[0]);
    return ok ? 0 : 1;
}

// R04: same-PID exec after fork. The exec image receives the expected pid and verifies it.
static int row_fork_exec(void) {
    pid_t pid = fork();
    if (pid < 0) {
        printf("S714-R04 fork-exec fork=%d ok=0\n", errno);
        return 1;
    }
    if (pid == 0) {
        char expected[32];
        (void)snprintf(expected, sizeof(expected), "%d", (int)getpid());
        char *argv[] = {(char *)SELF, (char *)"--exec-child", expected, NULL};
        execv(SELF, argv);
        printf("S714 exec-child exec-failed errno=%d\n", errno);
        _exit(127);
    }
    int status = 0;
    pid_t reaped = waitpid(pid, &status, 0);
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    int ok = reaped == pid && child_exit == 0;
    printf("S714-R04 fork-exec parent=%d child=%d status=%d ok=%d\n", (int)getpid(), (int)pid,
           child_exit, ok);
    return ok ? 0 : 1;
}

// R05: a caught signal delivered to the fork child resumes through the kernel sigcode page, then
// the child still sees its own saved frame.
static int row_fork_signal(void) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = on_usr1;
    if (sigemptyset(&action.sa_mask) != 0 || sigaction(SIGUSR1, &action, NULL) != 0) {
        printf("S714-R05 fork-signal setup=0 ok=0\n");
        return 1;
    }
    int pipefd[2];
    if (pipe(pipefd) != 0) {
        printf("S714-R05 fork-signal pipe=0 ok=0\n");
        return 1;
    }
    volatile uint64_t canary[CANARY_WORDS];
    canary_fill(canary, CANARY_A);
    pid_t parent = getpid();
    pid_t pid = fork();
    if (pid < 0) {
        printf("S714-R05 fork-signal fork=%d ok=0\n", errno);
        return 1;
    }
    if (pid == 0) {
        close(pipefd[0]);
        usr1_seen = 0;
        if (kill(getpid(), SIGUSR1) != 0) {
            _exit(7);
        }
        // Delivery happens on the syscall return; bound a preempted delivery.
        for (volatile long spin = 0; !usr1_seen && spin < 100000000; spin++) {
            __asm__ volatile("" ::: "memory");
        }
        struct child_report report = make_report(canary, usr1_seen != 0, getppid() == parent);
        (void)write_full(pipefd[1], &report, sizeof(report));
        _exit(0);
    }
    close(pipefd[1]);
    struct child_report report;
    memset(&report, 0, sizeof(report));
    int ok = read_full(pipefd[0], &report, sizeof(report)) == 0;
    int status = 0;
    pid_t reaped = waitpid(pid, &status, 0);
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    ok = ok && report.magic == REPORT_MAGIC && report.canary_ok && report.handler_ok &&
         reaped == pid && child_exit == 0;
    printf("S714-R05 fork-signal parent=%d child=%d handler=%u canary=%u status=%d ok=%d\n",
           (int)parent, (int)report.pid, report.handler_ok, report.canary_ok, child_exit, ok);
    close(pipefd[0]);
    return ok ? 0 : 1;
}

// R06: repeated fork/exec in one process must not leak or reuse the resume frame.
static int row_repeat(void) {
    int ok = 1;
    for (int iteration = 0; iteration < 16 && ok; iteration++) {
        pid_t pid = fork();
        if (pid < 0) {
            printf("S714-R06 repeat iteration=%d fork=%d ok=0\n", iteration, errno);
            return 1;
        }
        if (pid == 0) {
            char expected[32];
            (void)snprintf(expected, sizeof(expected), "%d", (int)getpid());
            char *argv[] = {(char *)SELF, (char *)"--exec-child", expected, NULL};
            execv(SELF, argv);
            _exit(127);
        }
        int status = 0;
        if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            ok = 0;
        }
    }
    printf("S714-R06 repeat count=16 ok=%d\n", ok);
    return ok ? 0 : 1;
}

// R07: a failed fresh-image creation reports its error and leaves the spawner usable.
static int row_failed_spawn(void) {
    char *missing[] = {(char *)"/bin/s714-no-such-image", NULL};
    pid_t failed = -1;
    int rc = posix_spawn(&failed, "/bin/s714-no-such-image", NULL, NULL, missing, environ);
    int reported = rc == ENOENT;
    char *argv[] = {(char *)SELF, (char *)"--spawn-child", NULL};
    pid_t pid = -1;
    int second = posix_spawn(&pid, SELF, NULL, NULL, argv, environ);
    int status = 0;
    int waited = second == 0 && waitpid(pid, &status, 0) == pid && WIFEXITED(status) &&
                 WEXITSTATUS(status) == 0;
    int ok = reported && waited;
    printf("S714-R07 failed-spawn rc=%d recovered=%d ok=%d\n", rc, waited, ok);
    return ok ? 0 : 1;
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc > 1 && strcmp(argv[1], "--spawn-child") == 0) {
        printf("S714 spawn-child pid=%d ok=1\n", (int)getpid());
        return 0;
    }
    if (argc > 1 && strcmp(argv[1], "--exec-child") == 0) {
        int expected = argc > 2 ? atoi(argv[2]) : -1;
        int ok = expected == (int)getpid();
        printf("S714 exec-child pid=%d expect=%d ok=%d\n", (int)getpid(), expected, ok);
        return ok ? 0 : 1;
    }
    int failures = 0;
    failures += row_fresh_spawn();
    failures += row_fork_resume();
    failures += row_parent_exit();
    failures += row_fork_exec();
    failures += row_fork_signal();
    failures += row_repeat();
    failures += row_failed_spawn();
    if (failures == 0) {
        printf("S714-PROCESS-START ok\n");
        return 0;
    }
    printf("S714-PROCESS-START failed=%d\n", failures);
    return 1;
}
PROCESS_START_C

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" "process-start"

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "process-start: $CC not found" >&2
    exit 77
}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    echo "process-start: ld.lld not found" >&2
    exit 77
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$out/process_start.c" -o "$out/process-start.o"

if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins_arg=-lclang_rt.builtins
else
    builtins_arg=
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T tests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/process-start.o" \
    -L"$sysroot/usr/lib" -lc $builtins_arg \
    "$sysroot/usr/lib/crtn.o" -o "$out/process-start"

cp "$out/process-start" "$stage/libc-test"
: > "$stage/libc-child"
: > "$stage/libplug.so"
: > "$stage/libc.so.1"
: > "$stage/ld-elf.so.1"
sh "$root/tests/libc/target/shape-check.sh" "$stage/libc-test"

cargo xtask initfs --arch "$arch" --profile libc-test >/dev/null
base=$root/build/$arch/initfs.img
cargo build -q -p raven-b70-overlay
"$root/target/debug/raven-b70-overlay" "$base" "$out/initfs.img" \
    "/bin/libc-test=$stage/libc-test:0755" \
    "/etc/svcd.conf=$root/tests/conformance/process-start/svcd.conf:0644" \
    "/etc/services.d/libc-test.toml=$root/tests/conformance/process-start/libc-test.service.toml:0644"

sha256sum "$out/process-start" "$out/initfs.img"
echo "process-start: built $out/initfs.img"
