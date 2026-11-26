#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S177 memory-map conformance probe against the Raven libc sysroot. The probe is a
# static x86_64-unknown-ravenbsd executable staged at /bin/mmap-probe by the suite build.sh.
# No product source is changed.
#
# The two probe sources are generated into the build tree at run time, the S710b
# tests/signals/build.sh layout-probe pattern. No C source is committed to the scaffold.
#
# Usage: sh tests/conformance/memory-map/probe/build.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/memory-map
mkdir -p "$out"

cat > "$out/mmap_probe.c" <<'MMAP_PROBE_C'
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S177 memory-mapping conformance probe. Runs as the unprivileged `raven` account on the release
// image; prints one `mmap: <row> key=value ... ok=<0|1>` line per row. The runner asserts the rows.
// POSIX mmap/mprotect/munmap/mremap semantics; FreeBSD sys/vm/vm_mmap.c and vm_fault.c are the
// behaviour oracle. No donor code is copied.

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define PAGE 4096

static int mutation_private_shared = 0;

// S715 sibling-exit state. The worker keeps touching a shared user page and periodically blocks
// in an interruptible one-millisecond poll while the main thread calls _exit. The kernel must
// quiesce the worker before it releases the address space; the serial trace names the probe pid.
static char *sibling_page;
static int sibling_pipe[2];
static volatile sig_atomic_t user_ready;
static volatile sig_atomic_t kernel_ready;
static volatile unsigned long user_ticks;
static volatile unsigned long kernel_ticks;

static void *sibling_user_worker(void *argument) {
    (void)argument;
    unsigned long ticks = 0;
    sibling_page[0] = (char)0xa5;
    for (;;) {
        sibling_page[1] = (char)(ticks & 0xff);
        ticks++;
        user_ticks = ticks;
        if (!user_ready) {
            user_ready = 1;
            printf("S715-SIBLING user ready pid=%d ticks=%lu\n", (int)getpid(), ticks);
            fflush(stdout);
        }
    }
    return NULL;
}

static void *sibling_kernel_worker(void *argument) {
    (void)argument;
    unsigned long ticks = 0;
    for (;;) {
        struct pollfd ready = {.fd = sibling_pipe[0], .events = POLLIN, .revents = 0};
        (void)poll(&ready, 1, 1);
        ticks++;
        kernel_ticks = ticks;
        if (!kernel_ready) {
            kernel_ready = 1;
            printf("S715-SIBLING kernel ready pid=%d ticks=%lu\n", (int)getpid(), ticks);
            fflush(stdout);
        }
    }
    return NULL;
}

// Exit while the sibling is running user memory writes and an interruptible kernel wait. The
// row itself is printed before _exit; the runner correlates its pid with the kernel's
// `s194: quiesce waited` and `s194: last-exit` lines.
static int sibling_exit(void) {
    sibling_page = mmap(NULL, PAGE, PROT_READ | PROT_WRITE,
                        MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    if (sibling_page == MAP_FAILED) {
        printf("S715-MAIN fail=map errno=%d\n", errno);
        return 1;
    }
    if (pipe(sibling_pipe) != 0) {
        printf("S715-MAIN fail=pipe errno=%d\n", errno);
        return 1;
    }
    pthread_t user_worker;
    pthread_t kernel_worker;
    int created = pthread_create(&user_worker, NULL, sibling_user_worker, NULL);
    if (created != 0) {
        printf("S715-MAIN fail=user-create rc=%d\n", created);
        return 1;
    }
    created = pthread_create(&kernel_worker, NULL, sibling_kernel_worker, NULL);
    if (created != 0) {
        printf("S715-MAIN fail=kernel-create rc=%d\n", created);
        return 1;
    }
    while (!user_ready || !kernel_ready) {
        sched_yield();
    }
    struct timespec delay = {.tv_sec = 0, .tv_nsec = 200 * 1000 * 1000};
    (void)nanosleep(&delay, NULL);
    unsigned long user = user_ticks;
    unsigned long kernel = kernel_ticks;
    int pid = (int)getpid();
    printf("S715-MAIN exit pid=%d user=%lu kernel=%lu\n", pid, user, kernel);
    printf(
        "mmap: sibling-exit pid=%d user=%lu kernel=%lu ok=%d\n",
        pid, user, kernel, user > 0 && kernel > 0
    );
    fflush(stdout);
    _exit(0);
}

static void row(const char *name, int ok, const char *extra) {
    printf("mmap: %s %sok=%d\n", name, extra ? extra : "", ok);
    fflush(stdout);
}

static int child_signal(pid_t pid) {
    int status = 0;
    if (waitpid(pid, &status, 0) < 0) {
        return -1;
    }
    return WIFSIGNALED(status) ? WTERMSIG(status) : 0;
}

// A private or shared anonymous mapping; the parent writes after fork and the child reads after
// a pipe handshake. Private must keep the pre-fork zero; shared must see the parent's 0x11.
static void cow_row(int shared, int expect_shared, const char *name) {
    int sync[2];
    if (pipe(sync) < 0) {
        row(name, 0, "pipe=1 ");
        return;
    }
    char *page = mmap(NULL, PAGE, PROT_READ | PROT_WRITE,
                      (shared ? MAP_SHARED : MAP_PRIVATE) | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        row(name, 0, "map=0 ");
        return;
    }
    pid_t pid = fork();
    if (pid < 0) {
        row(name, 0, "fork=1 ");
        return;
    }
    if (pid == 0) {
        char token = 0;
        (void)read(sync[0], &token, 1);
        unsigned char value = (unsigned char)page[0];
        _exit(value);
    }
    page[0] = 0x11;
    (void)write(sync[1], "x", 1);
    (void)close(sync[0]);
    (void)close(sync[1]);
    int status = 0;
    (void)waitpid(pid, &status, 0);
    int code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    int expected = expect_shared ? 0x11 : 0x00;
    char extra[64];
    (void)snprintf(extra, sizeof extra, "child=%d expected=%d ", code, expected);
    row(name, code == expected, extra);
    (void)munmap(page, PAGE);
}

static void mprotect_row(void) {
    char *page = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED || mprotect(page, PAGE, PROT_READ) != 0) {
        row("mprotect-segv", 0, "setup=1 ");
        return;
    }
    pid_t pid = fork();
    if (pid == 0) {
        page[0] = 0x22;
        _exit(0);
    }
    int sig = child_signal(pid);
    char extra[32];
    (void)snprintf(extra, sizeof extra, "signal=%d ", sig);
    row("mprotect-segv", sig == SIGSEGV, extra);
    (void)munmap(page, PAGE);
}

// POSIX/FreeBSD MAP_FIXED replaces the occupied range. The Raven syscall-level K4 contract is
// EEXIST, and the libc documents the divergence (PG-13: unmap-then-map until map::REPLACE); the
// application-level row asserts the POSIX replacement it currently provides.
static void fixed_row(void) {
    char *page = mmap(NULL, 2 * PAGE, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        row("fixed-replace", 0, "map=0 ");
        return;
    }
    page[0] = 0x5a;
    errno = 0;
    void *again = mmap(page, PAGE, PROT_READ | PROT_WRITE,
                       MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    int ok = again != MAP_FAILED && ((unsigned char *)again)[0] == 0;
    char extra[48];
    (void)snprintf(extra, sizeof extra, "errno=%d replaced=%d ", ok ? 0 : errno, ok);
    row("fixed-replace", ok, extra);
    (void)munmap(page, 2 * PAGE);
}

static void sigbus_row(void) {
    const char *path = "/tmp/mmap-eof-file";
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        char extra[32];
        (void)snprintf(extra, sizeof extra, "open=%d ", errno);
        row("sigbus-eof", 0, extra);
        return;
    }
    char page[PAGE];
    (void)memset(page, 0x5a, sizeof page);
    if (write(fd, page, sizeof page) != (ssize_t)sizeof page) {
        row("sigbus-eof", 0, "write=1 ");
        (void)close(fd);
        return;
    }
    char *map = mmap(NULL, 2 * PAGE, PROT_READ, MAP_PRIVATE, fd, 0);
    if (map == MAP_FAILED) {
        char extra[32];
        (void)snprintf(extra, sizeof extra, "map=0 errno=%d ", errno);
        row("sigbus-eof", 0, extra);
        (void)close(fd);
        return;
    }
    pid_t pid = fork();
    if (pid == 0) {
        volatile unsigned char value = (unsigned char)map[PAGE];
        (void)value;
        _exit(0);
    }
    int sig = child_signal(pid);
    char extra[32];
    (void)snprintf(extra, sizeof extra, "signal=%d expected=%d ", sig, SIGBUS);
    row("sigbus-eof", sig == SIGBUS, extra);
    (void)munmap(map, 2 * PAGE);
    (void)close(fd);
    (void)unlink(path);
}

static void munmap_row(void) {
    char *page = mmap(NULL, 3 * PAGE, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        row("munmap-partial", 0, "map=0 ");
        return;
    }
    page[0] = 1;
    page[2 * PAGE] = 1;
    if (munmap(page + PAGE, PAGE) != 0) {
        row("munmap-partial", 0, "unmap=1 ");
        return;
    }
    int edges = (int)page[0] + (int)page[2 * PAGE];
    pid_t pid = fork();
    if (pid == 0) {
        volatile unsigned char value = (unsigned char)page[PAGE];
        (void)value;
        _exit(0);
    }
    int sig = child_signal(pid);
    char extra[48];
    (void)snprintf(extra, sizeof extra, "edges=%d middle=%d ", edges, sig);
    row("munmap-partial", edges == 2 && sig == SIGSEGV, extra);
    (void)munmap(page, PAGE);
    (void)munmap(page + 2 * PAGE, PAGE);
}

static void bad_len_row(void) {
    errno = 0;
    void *page = mmap(NULL, 0, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    int error = (page == MAP_FAILED) ? errno : 0;
    char extra[32];
    (void)snprintf(extra, sizeof extra, "errno=%d expected=%d ", error, EINVAL);
    row("bad-len", error == EINVAL, extra);
}

// Run every contract row once. The S198 churn gate calls this N times in one process so the
// kernel heap can be sampled across repeated map/unmap and fork/exit cycles.
static void run_rows(void) {
    char *zero = mmap(NULL, PAGE, PROT_READ | PROT_WRITE,
                      MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    int zeroed = zero != MAP_FAILED;
    if (zeroed) {
        for (size_t index = 0; index < PAGE; index++) {
            if (zero[index] != 0) {
                zeroed = 0;
                break;
            }
        }
        (void)munmap(zero, PAGE);
    }
    row("anon-zero", zeroed, "");

    cow_row(mutation_private_shared ? 1 : 0, 0, "cow-private");
    cow_row(1, 1, "cow-shared");
    mprotect_row();
    fixed_row();
    sigbus_row();
    munmap_row();
    bad_len_row();

    printf("mmap: summary mutation=%d\n", mutation_private_shared);
    fflush(stdout);
}

int main(int argc, char **argv) {
    int iterations = 1;
    int run_sibling_exit = 0;
    for (int index = 1; index < argc; index++) {
        if (strcmp(argv[index], "--mutation=private-shared") == 0) {
            mutation_private_shared = 1;
        } else if (strcmp(argv[index], "--sibling-exit") == 0) {
            run_sibling_exit = 1;
        } else if (strncmp(argv[index], "--iterations=", 13) == 0) {
            iterations = atoi(argv[index] + 13);
        }
    }
    if (run_sibling_exit) {
        return sibling_exit();
    }
    if (iterations < 1) {
        iterations = 1;
    }
    for (int index = 0; index < iterations; index++) {
        if (iterations > 1) {
            printf("mmap: iteration %d/%d\n", index + 1, iterations);
            fflush(stdout);
        }
        run_rows();
    }
    return 0;
}
MMAP_PROBE_C

cat > "$out/s198_shell.c" <<'S198_SHELL_C'
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S198 test-only login shell for the `raven` account. The release `brush` login shell faults on
// this base before the SSH command returns (the S195 K514 handover), so the conformance runner
// cannot drive `/bin/mmap-probe`. sshd invokes this shell as
// `s198-shell -c "<command>"`; it scans that command text for the probe's `--iterations=`,
// `--mutation=private-shared` and `--sibling-exit` tokens and execs the probe with them. The
// probe rows, the kernel and the product under test are unchanged.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    int iterations = 1;
    int mutation = 0;
    int sibling_exit = 0;
    for (int index = 1; index < argc; index++) {
        if (strstr(argv[index], "--mutation=private-shared") != 0) {
            mutation = 1;
        }
        if (strstr(argv[index], "--sibling-exit") != 0) {
            sibling_exit = 1;
        }
        const char *value = strstr(argv[index], "--iterations=");
        if (value != 0) {
            iterations = atoi(value + 13);
        }
    }
    if (iterations < 1) {
        iterations = 1;
    }
    char iteration_arg[32];
    (void)snprintf(iteration_arg, sizeof iteration_arg, "--iterations=%d", iterations);
    char *args[5];
    int count = 0;
    args[count++] = "/bin/mmap-probe";
    args[count++] = iteration_arg;
    if (mutation) {
        args[count++] = "--mutation=private-shared";
    }
    if (sibling_exit) {
        args[count++] = "--sibling-exit";
    }
    args[count] = (char *)0;
    execv(args[0], args);
    _exit(127);
}
S198_SHELL_C

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" "mmap-probe"

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "mmap-probe: $CC not found" >&2
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
    echo "mmap-probe: ld.lld not found" >&2
    exit 77
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$out/mmap_probe.c" -o "$out/mmap-probe.o"

if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins_arg=-lclang_rt.builtins
else
    builtins_arg=
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T tests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/mmap-probe.o" \
    -L"$sysroot/usr/lib" -lc $builtins_arg \
    "$sysroot/usr/lib/crtn.o" -o "$out/mmap-probe"

for helper in s198-hostkeys s198-shell; do
    source=$(echo "$helper" | tr '-' '_')
    if [ "$helper" = s198-shell ]; then
        src="$out/$source.c"
    else
        src="tests/conformance/memory-map/probe/$source.c"
    fi
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$src" -o "$out/$helper.o"
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T tests/libc/target/x86_64.ld \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$helper.o" \
        -L"$sysroot/usr/lib" -lc $builtins_arg \
        "$sysroot/usr/lib/crtn.o" -o "$out/$helper"
done

echo "mmap-probe: built $out/mmap-probe, $out/s198-hostkeys and $out/s198-shell"
