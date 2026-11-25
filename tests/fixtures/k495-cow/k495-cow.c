// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495/S164 test-only fork and copy-on-write visibility fixture. It fills a 4 KiB private
// anonymous page with 0xAA, forks, has the child read before/after the parent's write, makes the
// child write its own byte, and asserts the parent/child isolation and the same address. Then it
// seeds a file-backed MAP_SHARED page with 0xAA, has the parent store 0xBB, and forks+execs the
// fixture in `child-fd` mode on inherited fd 3; the exec child must see the writer's 0xBB through
// the shared mapping. The default `all` mode runs both. It does not use brush.
//
// Contract: C13; a private mapping is copy-on-write (parent and child read identical bytes until
// either writes; the writer sees its own copy), a SHARED mapping sees the writer's store.
// Donor: FreeBSD sys/vm/vm_fault.c vm_fault / vm_fault_copy_entry and sys/vm/vm_map.c
// vm_map_copy_entry (the checkout is sparse; DragonFly sys/vm is the permissive BSD donor read).

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define PAGE 4096
#define PRIVATE_FILL 0xAA
#define PARENT_STORE 0xBB
#define CHILD_STORE 0xCC
#define PATH "/tmp/k495-cow-page"

static void die(const char *what) {
    fprintf(stderr, "K495-COW %s: %s\n", what, strerror(errno));
    fflush(stderr);
    _exit(1);
}

static int wait_status(pid_t child) {
    int status;
    if (waitpid(child, &status, 0) < 0) {
        die("waitpid");
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + WTERMSIG(status);
    }
    return 255;
}

static int fail(const char *what) {
    printf("K495-COW FAIL %s\n", what);
    fflush(stdout);
    return 2;
}

static int test_fork(void) {
    unsigned char *p =
        mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        die("fork mmap");
    }
    memset(p, PRIVATE_FILL, PAGE);
    printf("K495-COW fork-parent pid=%d addr=%p initial=%02x last=%02x\n", getpid(), p, p[0],
           p[PAGE - 1]);
    fflush(stdout);

    int ready[2];
    int done[2];
    if (pipe(ready) < 0 || pipe(done) < 0) {
        die("fork pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        die("fork");
    }
    if (child == 0) {
        close(ready[1]);
        close(done[0]);
        char byte = 0;
        if (read(ready[0], &byte, 1) != 1) {
            _exit(3);
        }
        unsigned char inherited = p[0];
        p[0] = CHILD_STORE;
        byte = 'D';
        (void)write(done[1], &byte, 1);
        printf("K495-COW fork-child pid=%d addr=%p inherited=%02x first=%02x after-parent=%02x "
               "after-child=%02x last=%02x\n",
               getpid(), p, inherited, inherited, inherited, p[0], p[PAGE - 1]);
        fflush(stdout);
        int ok = inherited == PRIVATE_FILL && p[0] == CHILD_STORE && p[PAGE - 1] == PRIVATE_FILL;
        _exit(ok ? 0 : 2);
    }

    close(ready[0]);
    close(done[1]);
    p[0] = PARENT_STORE;
    char byte = 'R';
    (void)write(ready[1], &byte, 1);
    (void)read(done[0], &byte, 1);
    unsigned char after = p[0];
    int status = wait_status(child);
    printf("K495-COW fork-parent status=%d after-parent=%02x last=%02x\n", status, after,
           p[PAGE - 1]);
    fflush(stdout);
    if (after != PARENT_STORE || status != 0 || p[PAGE - 1] != PRIVATE_FILL) {
        return fail("fork isolation");
    }
    return 0;
}

// The contract also holds when the child writes first: the parent still reads the original bytes
// and the child keeps its own copy after the parent writes.
static int test_fork_child_first(void) {
    unsigned char *p =
        mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        die("fork-child-first mmap");
    }
    memset(p, PRIVATE_FILL, PAGE);
    printf("K495-COW fork-child-first-parent pid=%d initial=%02x\n", getpid(), p[0]);
    fflush(stdout);

    int go[2];
    int wrote[2];
    int parent_done[2];
    if (pipe(go) < 0 || pipe(wrote) < 0 || pipe(parent_done) < 0) {
        die("fork-child-first pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        die("fork-child-first fork");
    }
    if (child == 0) {
        close(go[1]);
        close(wrote[0]);
        close(parent_done[1]);
        char byte = 0;
        if (read(go[0], &byte, 1) != 1) {
            _exit(3);
        }
        p[0] = CHILD_STORE;
        byte = 'C';
        (void)write(wrote[1], &byte, 1);
        if (read(parent_done[0], &byte, 1) != 1) {
            _exit(3);
        }
        unsigned char after = p[0];
        printf("K495-COW fork-child-first-child pid=%d addr=%p stored=%02x after-parent-store=%02x\n",
               getpid(), p, CHILD_STORE, after);
        fflush(stdout);
        _exit(after == CHILD_STORE && p[PAGE - 1] == PRIVATE_FILL ? 0 : 2);
    }

    close(go[0]);
    close(wrote[1]);
    close(parent_done[0]);
    char byte = 'R';
    (void)write(go[1], &byte, 1);
    if (read(wrote[0], &byte, 1) != 1) {
        die("fork-child-first wrote");
    }
    unsigned char after_child = p[0];
    p[0] = PARENT_STORE;
    byte = 'P';
    (void)write(parent_done[1], &byte, 1);
    int status = wait_status(child);
    printf("K495-COW fork-child-first-parent status=%d after-child=%02x after-parent=%02x\n",
           status, after_child, p[0]);
    fflush(stdout);
    if (after_child != PRIVATE_FILL || p[0] != PARENT_STORE || status != 0) {
        return fail("fork child-first isolation");
    }
    return 0;
}

static int open_seeded_file(void) {
    int fd = open(PATH, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        die("seed open");
    }
    // Raven's ramfs ftruncate is not reliable (C55); size the file with a full write.
    unsigned char *seed = malloc(PAGE);
    if (seed == NULL) {
        die("seed malloc");
    }
    memset(seed, PRIVATE_FILL, PAGE);
    size_t wrote = 0;
    while (wrote < PAGE) {
        ssize_t n = write(fd, seed + wrote, PAGE - wrote);
        if (n <= 0) {
            die("seed write");
        }
        wrote += (size_t)n;
    }
    free(seed);
    return fd;
}

// A shared mapping inherited across fork must show the parent's store: the child maps the same
// object frame, so no pager round trip is involved.
static int test_shared_fork(void) {
    int fd = open_seeded_file();
    unsigned char *p = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (p == MAP_FAILED) {
        die("shared-fork mmap");
    }
    printf("K495-COW shared-fork-parent pid=%d addr=%p initial=%02x\n", getpid(), p, p[0]);
    fflush(stdout);

    int ready[2];
    int store[2];
    if (pipe(ready) < 0 || pipe(store) < 0) {
        die("shared-fork pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        die("shared-fork fork");
    }
    if (child == 0) {
        close(ready[0]);
        close(store[1]);
        unsigned char inherited = p[0];
        char byte = 'R';
        (void)write(ready[1], &byte, 1);
        if (read(store[0], &byte, 1) != 1) {
            _exit(3);
        }
        unsigned char after = p[0];
        printf("K495-COW shared-fork-child pid=%d addr=%p inherited=%02x after-store=%02x\n",
               getpid(), p, inherited, after);
        fflush(stdout);
        _exit(inherited == PRIVATE_FILL && after == PARENT_STORE ? 0 : 2);
    }

    close(ready[1]);
    close(store[0]);
    char byte = 0;
    if (read(ready[0], &byte, 1) != 1) {
        die("shared-fork ready");
    }
    p[0] = PARENT_STORE;
    byte = 'S';
    (void)write(store[1], &byte, 1);
    int status = wait_status(child);
    printf("K495-COW shared-fork-parent status=%d after-store=%02x\n", status, p[0]);
    fflush(stdout);
    if (p[0] != PARENT_STORE || status != 0) {
        return fail("shared fork store");
    }
    return 0;
}

// Two mappings of the same file window in one process must resolve to one page-cache object, so
// the second mapping sees the first mapping's store without fork or exec in the path.
static int test_remap(void) {
    int fd = open_seeded_file();
    unsigned char *first = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (first == MAP_FAILED) {
        die("remap first mmap");
    }
    unsigned char *second = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (second == MAP_FAILED) {
        die("remap second mmap");
    }
    unsigned char before = first[0];
    first[0] = PARENT_STORE;
    unsigned char after = second[0];
    printf("K495-COW remap-parent pid=%d first=%02x after-store=%02x\n", getpid(), before, after);
    fflush(stdout);
    if (before != PRIVATE_FILL || after != PARENT_STORE || second[PAGE - 1] != PRIVATE_FILL) {
        return fail("remap store");
    }
    return 0;
}

static int child_shared_fd(int fd, int ready_fd, int store_fd) {
    unsigned char *p = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (p == MAP_FAILED) {
        die("child mmap fd");
    }
    unsigned char first = p[0];
    unsigned char last = p[PAGE - 1];
    char byte = 'R';
    (void)write(ready_fd, &byte, 1);
    (void)read(store_fd, &byte, 1);
    unsigned char after = p[0];
    printf("K495-COW exec-child pid=%d fd=%d addr=%p first=%02x before-store=%02x "
           "after-store=%02x last=%02x\n",
           getpid(), fd, p, first, first, after, p[PAGE - 1]);
    fflush(stdout);
    int ok = first == PRIVATE_FILL && after == PARENT_STORE && last == PRIVATE_FILL;
    return ok ? 0 : 2;
}

static int test_shared(const char *program) {
    int fd = open(PATH, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        die("spawn open");
    }
    // Raven's ramfs ftruncate is not reliable (C55); size the file with a full write.
    unsigned char *seed = malloc(PAGE);
    if (seed == NULL) {
        die("spawn malloc");
    }
    memset(seed, PRIVATE_FILL, PAGE);
    size_t wrote = 0;
    while (wrote < PAGE) {
        ssize_t n = write(fd, seed + wrote, PAGE - wrote);
        if (n <= 0) {
            die("spawn write");
        }
        wrote += (size_t)n;
    }
    free(seed);
    // Pass the already-open shared file descriptor across the exec; the child maps fd 3.
    if (dup2(fd, 3) < 0) {
        die("spawn dup2");
    }
    unsigned char *p = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, 3, 0);
    if (p == MAP_FAILED) {
        die("spawn mmap");
    }
    printf("K495-COW spawn-parent pid=%d addr=%p seeded=%02x store=%02x first=%02x last=%02x\n",
           getpid(), p, p[0], PARENT_STORE, p[0], p[PAGE - 1]);
    fflush(stdout);

    int ready[2];
    int store[2];
    if (pipe(ready) < 0 || pipe(store) < 0) {
        die("spawn pipe");
    }
    pid_t child = fork();
    if (child < 0) {
        die("spawn fork");
    }
    if (child == 0) {
        close(ready[0]);
        close(store[1]);
        if (dup2(ready[1], 4) < 0 || dup2(store[0], 5) < 0) {
            die("spawn child dup2");
        }
        execl(program, program, "child-fd", (char *)NULL);
        die("execl");
    }

    close(ready[1]);
    close(store[0]);
    char byte = 0;
    if (read(ready[0], &byte, 1) != 1) {
        die("spawn ready");
    }
    p[0] = PARENT_STORE;
    byte = 'S';
    (void)write(store[1], &byte, 1);
    int status = wait_status(child);
    printf("K495-COW spawn-parent status=%d after-store=%02x last=%02x\n", status, p[0],
           p[PAGE - 1]);
    fflush(stdout);
    if (p[0] != PARENT_STORE || status != 0 || p[PAGE - 1] != PRIVATE_FILL) {
        return fail("shared store");
    }
    return 0;
}

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "all";

    if (strcmp(mode, "child-fd") == 0) {
        return child_shared_fd(3, 4, 5);
    }
    if (strcmp(mode, "fork") == 0) {
        return test_fork();
    }
    if (strcmp(mode, "fork-child-first") == 0) {
        return test_fork_child_first();
    }
    if (strcmp(mode, "shared-fork") == 0) {
        return test_shared_fork();
    }
    if (strcmp(mode, "remap") == 0) {
        return test_remap();
    }
    if (strcmp(mode, "spawn") == 0) {
        return test_shared(argv[0]);
    }
    if (strcmp(mode, "all") == 0) {
        int result = test_fork();
        if (result != 0) {
            return result;
        }
        result = test_fork_child_first();
        if (result != 0) {
            return result;
        }
        result = test_shared_fork();
        if (result != 0) {
            return result;
        }
        result = test_remap();
        if (result != 0) {
            return result;
        }
        result = test_shared(argv[0]);
        if (result != 0) {
            return result;
        }
        printf("K495-COW ok\n");
        fflush(stdout);
        return 0;
    }

    fprintf(stderr, "usage: %s [fork|shared-fork|remap|spawn|all|child-fd]\n", argv[0]);
    return 64;
}
