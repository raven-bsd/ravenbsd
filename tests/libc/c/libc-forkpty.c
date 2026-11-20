/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K196: forkpty on the real pty: server. The child runs with the slave as its controlling
 * terminal and stdio; the parent reads the transcript from the master, sees EOF when the
 * slave closes and reaps the child status. */

#include <errno.h>
#include <libutil.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("libc-forkpty: fail %s=%d errno=%d\n", what, value, errno);
    return 1;
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
    printf("libc-forkpty: ok\n");
    return 0;
}
