// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K531 guest fixture: prove /tmp is 1777+sticky for a normal user without `su`. The process starts
// as root, creates a root-owned file, forks a child that drops to uid/gid 1000, and the child does
// the create/remove/mode rows. The parent then removes everything as root.

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *what) {
    printf("tmp-writable: fail %s errno=%d\n", what, errno);
    return 1;
}

int main(void) {
    struct stat st;

    setvbuf(stdout, NULL, _IONBF, 0);

    if (stat("/tmp", &st) != 0) {
        return fail("stat");
    }
    printf("tmp-writable: mode=%o uid=%u gid=%u\n", (unsigned)(st.st_mode & 07777),
           (unsigned)st.st_uid, (unsigned)st.st_gid);

    int root_fd = open("/tmp/k531-root", O_CREAT | O_WRONLY | O_TRUNC, 0600);
    if (root_fd < 0) {
        return fail("root-create");
    }
    if (write(root_fd, "root\n", 5) != 5) {
        close(root_fd);
        return fail("root-write");
    }
    close(root_fd);
    if (chmod("/tmp/k531-root", 0600) != 0) {
        return fail("root-chmod");
    }

    pid_t pid = fork();
    if (pid < 0) {
        return fail("fork");
    }
    if (pid == 0) {
        if (setgid(1000) != 0 || setuid(1000) != 0) {
            printf("user-create-denied errno=%d\n", errno);
            _exit(1);
        }

        int user_fd = open("/tmp/k531-user", O_CREAT | O_WRONLY | O_TRUNC, 0600);
        if (user_fd < 0) {
            printf("user-create-denied errno=%d\n", errno);
            _exit(1);
        }
        close(user_fd);
        printf("user-create-ok\n");

        if (unlink("/tmp/k531-root") == 0) {
            printf("user-remove-unexpected\n");
            _exit(2);
        }
        printf("user-remove-denied errno=%d (%s)\n", errno, strerror(errno));

        if (mkdir("/tmp/k531-dir", 0755) != 0) {
            printf("dir-create-denied errno=%d (%s)\n", errno, strerror(errno));
            _exit(3);
        }
        int mode_fd = open("/tmp/k531-file", O_CREAT | O_WRONLY | O_TRUNC, 0600);
        if (mode_fd < 0) {
            printf("file-create-denied errno=%d (%s)\n", errno, strerror(errno));
            _exit(4);
        }
        close(mode_fd);
        if (chmod("/tmp/k531-file", 0600) != 0) {
            printf("file-chmod-denied errno=%d (%s)\n", errno, strerror(errno));
            _exit(5);
        }

        struct stat dir_st;
        struct stat file_st;
        if (stat("/tmp/k531-dir", &dir_st) != 0 || stat("/tmp/k531-file", &file_st) != 0) {
            printf("user-stat-denied errno=%d\n", errno);
            _exit(6);
        }
        printf("dir-mode=%o\n", (unsigned)(dir_st.st_mode & 0777));
        printf("file-mode=%o\n", (unsigned)(file_st.st_mode & 0777));
        _exit(0);
    }

    int status = 0;
    if (waitpid(pid, &status, 0) != pid) {
        return fail("waitpid");
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("tmp-writable: fail child status=%d\n", status);
        return 1;
    }

    if (unlink("/tmp/k531-root") != 0 || unlink("/tmp/k531-user") != 0 ||
        unlink("/tmp/k531-file") != 0 || rmdir("/tmp/k531-dir") != 0) {
        return fail("root-remove");
    }
    printf("root-remove-ok\n");
    printf("tmp-writable: ok\n");
    return 0;
}
