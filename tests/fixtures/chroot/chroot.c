/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K223: per-process root. The test builds a small tree on the writable /tmp ramfs, proves the
 * non-root and file-target failures, chroots, checks absolute/.. confinement, then forks and
 * execs the child that inherits the root. The empty-root case runs in a separate child. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("chroot: fail %s=%d errno=%d\n", what, value, errno);
    fflush(stdout);
    return 1;
}

static int copy_file(const char *source, const char *destination) {
    int in = open(source, O_RDONLY);
    if (in < 0)
        return -1;
    int out = open(destination, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (out < 0) {
        close(in);
        return -1;
    }
    char buffer[4096];
    ssize_t count;
    while ((count = read(in, buffer, sizeof(buffer))) > 0) {
        ssize_t offset = 0;
        while (offset < count) {
            ssize_t written = write(out, buffer + offset, (size_t)(count - offset));
            if (written <= 0) {
                close(in);
                close(out);
                return -1;
            }
            offset += written;
        }
    }
    close(in);
    close(out);
    return chmod(destination, 0755);
}

static int wait_ok(pid_t pid) {
    int status = 0;
    if (waitpid(pid, &status, 0) != pid)
        return -1;
    return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 0 : -1;
}

static int nonroot_check(void) {
    pid_t pid = fork();
    if (pid < 0)
        return -1;
    if (pid == 0) {
        if (setuid(1000) != 0)
            _exit(3);
        errno = 0;
        if (chroot("/tmp/chroot-root") != -1 || errno != EPERM)
            _exit(4);
        _exit(0);
    }
    return wait_ok(pid);
}

static int empty_root_check(void) {
    pid_t pid = fork();
    if (pid < 0)
        return -1;
    if (pid == 0) {
        if (chroot("/tmp/var/empty") != 0)
            _exit(3);
        errno = 0;
        if (open("/etc/passwd", O_RDONLY) != -1 || errno != ENOENT)
            _exit(4);
        _exit(0);
    }
    return wait_ok(pid);
}

static int exec_child_check(void) {
    pid_t pid = fork();
    if (pid < 0)
        return -1;
    if (pid == 0) {
        char *argv[] = {(char *)"/bin/libc-child", NULL};
        char *envp[] = {NULL};
        execve("/bin/libc-child", argv, envp);
        _exit(errno == 0 ? 5 : errno);
    }
    return wait_ok(pid);
}

int main(void) {
    printf("chroot: start\n");
    fflush(stdout);

    if (mkdir("/tmp/chroot-root", 0755) != 0 && errno != EEXIST)
        return fail("mkdir root", -1);
    if (mkdir("/tmp/chroot-root/bin", 0755) != 0 && errno != EEXIST)
        return fail("mkdir bin", -1);
    if (mkdir("/tmp/var", 0755) != 0 && errno != EEXIST)
        return fail("mkdir var", -1);
    if (mkdir("/tmp/var/empty", 0755) != 0 && errno != EEXIST)
        return fail("mkdir empty", -1);

    int inside = open("/tmp/chroot-root/inside", O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (inside < 0 || write(inside, "inside\n", 7) != 7)
        return fail("inside", -1);
    close(inside);
    int outside = open("/tmp/outside", O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (outside < 0 || write(outside, "outside\n", 8) != 8)
        return fail("outside", -1);
    close(outside);
    int notdir = open("/tmp/chroot-root/not-a-dir", O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (notdir < 0)
        return fail("not-a-dir", -1);
    close(notdir);
    if (symlink("inside", "/tmp/chroot-root/good") != 0)
        return fail("symlink good", -1);
    if (symlink("/tmp/outside", "/tmp/chroot-root/escape") != 0)
        return fail("symlink escape", -1);
    if (symlink("../../outside", "/tmp/chroot-root/rel-escape") != 0)
        return fail("symlink rel-escape", -1);
    if (symlink("/inside", "/tmp/chroot-root/abs") != 0)
        return fail("symlink abs", -1);
    if (symlink("/missing", "/tmp/chroot-root/dangling") != 0)
        return fail("symlink dangling", -1);
    if (copy_file("/bin/libc-child", "/tmp/chroot-root/bin/libc-child") != 0)
        return fail("copy child", -1);

    errno = 0;
    int unrooted = open("/tmp/chroot-root/good", O_RDONLY);
    if (unrooted < 0)
        return fail("unrooted symlink", errno);
    close(unrooted);
    printf("chroot: unrooted good=1\n");

    if (nonroot_check() != 0)
        return fail("nonroot", -1);
    printf("chroot: nonroot eperm=1\n");

    if (empty_root_check() != 0)
        return fail("empty-root", -1);
    printf("chroot: empty-root ok\n");

    if (chroot("/tmp/chroot-root") != 0)
        return fail("chroot", errno);
    printf("chroot: chroot ok\n");

    if (chdir("/") != 0)
        return fail("chdir", errno);
    char cwd[64];
    if (getcwd(cwd, sizeof(cwd)) == NULL)
        return fail("getcwd", errno);
    printf("chroot: cwd=%s\n", cwd);

    int fd = open("/inside", O_RDONLY);
    if (fd < 0)
        return fail("inside open", errno);
    close(fd);
    errno = 0;
    int escaped = open("/outside", O_RDONLY);
    if (escaped >= 0)
        return fail("outside opened", escaped);
    printf("chroot: inside=1 outside=0\n");

    errno = 0;
    escaped = open("/../outside", O_RDONLY);
    if (escaped >= 0)
        return fail("dotdot opened", escaped);
    errno = 0;
    escaped = open("/inside/../../outside", O_RDONLY);
    if (escaped >= 0)
        return fail("dotdot2 opened", escaped);
    printf("chroot: dotdot=0\n");

    int good = open("/good", O_RDONLY);
    int absolute = open("/abs", O_RDONLY);
    if (good < 0 || absolute < 0)
        return fail("rooted symlink inside", errno);
    close(good);
    close(absolute);
    errno = 0;
    int symlink_escape = open("/escape", O_RDONLY);
    if (symlink_escape >= 0)
        return fail("symlink escape opened", symlink_escape);
    errno = 0;
    int rel_escape = open("/rel-escape", O_RDONLY);
    if (rel_escape >= 0)
        return fail("symlink rel-escape opened", rel_escape);
    errno = 0;
    int dangling = open("/dangling", O_RDONLY);
    if (dangling >= 0)
        return fail("symlink dangling opened", dangling);
    printf("chroot: symlink inside=1 abs=1 escape=0 rel=0 dangling=0\n");

    errno = 0;
    if (chroot("/not-a-dir") != -1 || errno != ENOTDIR)
        return fail("notdir", errno);
    printf("chroot: notdir=1\n");

    if (exec_child_check() != 0)
        return fail("exec child", -1);
    printf("chroot: child ok\n");

    printf("chroot: ok\n");
    fflush(stdout);
    return 0;
}
