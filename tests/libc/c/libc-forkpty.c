/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K196: forkpty on the real pty: server. The child runs with the slave as its controlling
 * terminal and stdio; the parent reads the transcript from the master, sees EOF when the
 * slave closes and reaps the child status. */

#include <errno.h>
#include <libutil.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("libc-forkpty: fail %s=%d errno=%d\n", what, value, errno);
    return 1;
}

/* K548: the sshd privsep/channel pipe shape. The child holds the write end across the fork;
 * the parent closes its own write end. The parent's read must still block for the child's byte
 * and see EOF only after the last write handle closes. */
static int pipe_reference_check(void) {
    int ends[2];
    if (pipe(ends) != 0)
        return fail("pipe", -1);

    pid_t child = fork();
    if (child < 0)
        return fail("pipe-fork", (int)child);
    if (child == 0) {
        close(ends[0]);
        usleep(100000);
        char byte = 'x';
        ssize_t written = write(ends[1], &byte, 1);
        if (written != 1)
            _exit(11);
        close(ends[1]);
        // Stay alive after the logical close so a leaked inherited handle would delay the peer EOF.
        usleep(500000);
        _exit(0);
    }

    /* The child still owns the inherited write handle; closing ours must not close the pipe. */
    close(ends[1]);
    printf("libc-forkpty: pipe child=%d\n", (int)child);

    char byte = 0;
    ssize_t count = read(ends[0], &byte, 1);
    if (count != 1 || byte != 'x') {
        int saved = errno;
        printf("libc-forkpty: fail pipe-first count=%ld errno=%d\n", (long)count, saved);
        close(ends[0]);
        (void)waitpid(child, NULL, 0);
        return 1;
    }
    printf("libc-forkpty: pipe byte=x\n");

    // The child closed its write fd but is still alive; a leaked inherited pipe handle would keep
    // the write side open and suppress this EOF until the child exits.
    struct pollfd ready;
    ready.fd = ends[0];
    ready.events = POLLIN;
    ready.revents = 0;
    int polled = poll(&ready, 1, 300);
    if (polled != 1 || (ready.revents & (POLLIN | POLLHUP)) == 0) {
        printf("libc-forkpty: fail pipe-eof poll=%d revents=0x%x\n", polled,
               (unsigned)ready.revents);
        close(ends[0]);
        (void)waitpid(child, NULL, 0);
        return 1;
    }
    count = read(ends[0], &byte, 1);
    if (count != 0) {
        printf("libc-forkpty: fail pipe-eof count=%ld errno=%d\n", (long)count, errno);
        close(ends[0]);
        (void)waitpid(child, NULL, 0);
        return 1;
    }
    printf("libc-forkpty: pipe eof=1\n");

    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("pipe-waitpid", 0);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("pipe-status", status);
    close(ends[0]);
    printf("libc-forkpty: pipe ok\n");
    return 0;
}

int main(void) {
    printf("libc-forkpty: start\n");

    int master = -1;
    char name[128];
    memset(name, 0, sizeof(name));
    pid_t pid = forkpty(&master, name, NULL, NULL);
    if (pid < 0)
        return fail("forkpty", (int)pid);

    if (pid == 0) {
        const char *text = "pty-child\n";
        ssize_t written = write(1, text, strlen(text));
        _exit(written == (ssize_t)strlen(text) ? 3 : 4);
    }

    printf("libc-forkpty: parent pid=%d name=%s\n", (int)pid, name);

    char buffer[256];
    size_t total = 0;
    int eof = 0;
    for (int spins = 0; spins < 4000; spins++) {
        ssize_t count = read(master, buffer + total, sizeof(buffer) - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            continue;
        }
        if (count == 0) {
            eof = 1;
            break;
        }
        if (errno == EAGAIN || errno == EINTR) {
            usleep(1000);
            continue;
        }
        return fail("read", (int)count);
    }
    buffer[total] = 0;
    printf("libc-forkpty: master=%s", buffer);
    if (!eof)
        return fail("eof", 0);
    if (strstr(buffer, "pty-child") == NULL)
        return fail("transcript", 0);

    int status = 0;
    if (waitpid(pid, &status, 0) != pid)
        return fail("waitpid", 0);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 3)
        return fail("status", status);

    close(master);
    if (pipe_reference_check() != 0)
        return 1;
    printf("libc-forkpty: ok\n");
    return 0;
}
