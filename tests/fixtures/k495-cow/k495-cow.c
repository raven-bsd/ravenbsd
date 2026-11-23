// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495 rev185 test-only no-brush process-memory fixture. It fills a 4 KiB page with 0xAA, forks
// and has the child print the same bytes (private anonymous COW), then repeats through a
// file-backed MAP_SHARED mapping and a fork+exec child (the exec path). It does not use brush.
// If both children read 0xAA the kernel copy/spawn/exec path is sound for this case; a zeroed
// brush table is then not a generic kernel COW failure. Test tooling, not product code.

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
#define FILL 0xAA
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

static int child_shared(const char *path) {
    int fd = open(path, O_RDWR);
    if (fd < 0) {
        die("child open");
    }
    unsigned char *p = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (p == MAP_FAILED) {
        die("child mmap");
    }
    int ok = p[0] == FILL && p[PAGE - 1] == FILL;
    printf("K495-COW exec-child pid=%d addr=%p first=%02x last=%02x\n", getpid(), p, p[0],
           p[PAGE - 1]);
    fflush(stdout);
    close(fd);
    return ok ? 0 : 2;
}

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "fork";

    if (strcmp(mode, "child") == 0) {
        return child_shared(argc > 2 ? argv[2] : PATH);
    }

    if (strcmp(mode, "fork") == 0) {
        unsigned char *p =
            mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        if (p == MAP_FAILED) {
            die("fork mmap");
        }
        memset(p, FILL, PAGE);
        printf("K495-COW fork-parent pid=%d addr=%p first=%02x last=%02x\n", getpid(), p, p[0],
               p[PAGE - 1]);
        fflush(stdout);
        pid_t child = fork();
        if (child < 0) {
            die("fork");
        }
        if (child == 0) {
            int ok = p[0] == FILL && p[PAGE - 1] == FILL;
            printf("K495-COW fork-child pid=%d addr=%p first=%02x last=%02x\n", getpid(), p, p[0],
                   p[PAGE - 1]);
            fflush(stdout);
            _exit(ok ? 0 : 2);
        }
        int status = wait_status(child);
        printf("K495-COW fork-parent status=%d\n", status);
        fflush(stdout);
        return status == 0 ? 0 : 2;
    }

    if (strcmp(mode, "spawn") == 0) {
        int fd = open(PATH, O_RDWR | O_CREAT | O_TRUNC, 0600);
        if (fd < 0) {
            die("spawn open");
        }
        if (ftruncate(fd, PAGE) < 0) {
            die("ftruncate");
        }
        unsigned char *p = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
        if (p == MAP_FAILED) {
            die("spawn mmap");
        }
        memset(p, FILL, PAGE);
        printf("K495-COW spawn-parent pid=%d addr=%p first=%02x last=%02x\n", getpid(), p, p[0],
               p[PAGE - 1]);
        fflush(stdout);
        pid_t child = fork();
        if (child < 0) {
            die("spawn fork");
        }
        if (child == 0) {
            execl(argv[0], argv[0], "child", PATH, (char *)NULL);
            die("execl");
        }
        int status = wait_status(child);
        printf("K495-COW spawn-parent status=%d\n", status);
        fflush(stdout);
        return status == 0 ? 0 : 2;
    }

    fprintf(stderr, "usage: %s fork|spawn|child [path]\n", argv[0]);
    return 64;
}
