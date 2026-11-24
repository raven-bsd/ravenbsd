// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S177 memory-mapping conformance probe. Runs as the unprivileged `raven` account on the release
// image; prints one `mmap: <row> key=value ... ok=<0|1>` line per row. The runner asserts the rows.
// POSIX mmap/mprotect/munmap/mremap semantics; FreeBSD sys/vm/vm_mmap.c and vm_fault.c are the
// behaviour oracle. No donor code is copied.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define PAGE 4096

static int mutation_private_shared = 0;

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
    const char *path = "/home/raven/mmap-eof-file";
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

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--mutation=private-shared") == 0) {
        mutation_private_shared = 1;
    }
    int expected_zero = 0;
    (void)expected_zero;

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
    return 0;
}
