/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K436: one guest assertion per call in the OpenSSH sshpty.c pty_allocate sequence:
 * posix_openpt -> grantpt -> unlockpt -> ptsname -> open(ptsname) -> tcgetattr.
 * The composed sequence is the same calls in order; a red call names the owning layer. */

#include <errno.h>
#include <fcntl.h>
#include <libutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("pty-calls: fail %s=%d errno=%d\n", what, value, errno);
    return 1;
}

static int read_expect(int fd, const char *want) {
    char buffer[256];
    size_t total = 0;
    buffer[0] = 0;
    for (int spins = 0; spins < 4000; spins++) {
        ssize_t count = read(fd, buffer + total, sizeof(buffer) - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            buffer[total] = 0;
            if (strstr(buffer, want) != NULL)
                return 0;
            continue;
        }
        if (count == 0)
            break;
        if (errno == EAGAIN || errno == EINTR)
            continue;
        printf("pty-calls: read error errno=%d\n", errno);
        return -1;
    }
    printf("pty-calls: read missing want=%s got=%s\n", want, buffer);
    return -1;
}

int main(void) {
    printf("pty-calls: start uid=%d\n", (int)getuid());

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    printf("pty-calls: posix-openpt fd=%d\n", master);
    if (master < 0)
        return fail("posix_openpt", master);

    int granted = grantpt(master);
    printf("pty-calls: grantpt=%d\n", granted);
    if (granted != 0)
        return fail("grantpt", granted);

    int unlocked = unlockpt(master);
    printf("pty-calls: unlockpt=%d\n", unlocked);
    if (unlocked != 0)
        return fail("unlockpt", unlocked);

    char *name = ptsname(master);
    printf("pty-calls: ptsname=%s\n", name == NULL ? "(null)" : name);
    if (name == NULL)
        return fail("ptsname", -1);
    if (strncmp(name, "/dev/pts/", 9) != 0)
        return fail("ptsname-path", -1);

    int slave = open(name, O_RDWR);
    printf("pty-calls: slave-open fd=%d\n", slave);
    if (slave < 0)
        return fail("open", slave);

    struct termios term;
    if (tcgetattr(slave, &term) != 0)
        return fail("tcgetattr", -1);
    printf("pty-calls: tcgetattr=ok\n");

    char *tty = ttyname(slave);
    printf("pty-calls: ttyname=%s\n", tty == NULL ? "(null)" : tty);
    if (tty == NULL)
        return fail("ttyname", -1);
    if (strncmp(tty, "/dev/pts/", 9) != 0)
        return fail("ttyname-path", -1);

    if (write(master, "hello\n", 6) != 6)
        return fail("write-master", -1);
    if (read_expect(slave, "hello") < 0)
        return fail("read-slave", -1);
    if (write(slave, "world\n", 6) != 6)
        return fail("write-slave", -1);
    if (read_expect(master, "world") < 0)
        return fail("read-master", -1);
    printf("pty-calls: roundtrip=ok\n");

    close(slave);
    close(master);
    printf("pty-calls: composed=ok\n");
    printf("pty-calls: ok\n");
    return 0;
}
