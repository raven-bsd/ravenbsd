/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K430: reproduce the guest-side openpty()/pty: failure without OpenSSH.
 *
 * The first phase runs as root and exercises posix_openpt, grantpt, unlockpt,
 * ptsname, master/slave round-trip. The second phase drops to uid 1000, the
 * post-auth sshd-session user, and repeats the allocation. A failure prints the
 * step and errno so the qtest serial identifies the owning layer. */

#include <errno.h>
#include <libutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("pty-alloc: fail %s=%d errno=%d\n", what, value, errno);
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
        if (errno == EAGAIN || errno == EINTR) {
            usleep(1000);
            continue;
        }
        printf("pty-alloc: read error errno=%d\n", errno);
        return -1;
    }
    printf("pty-alloc: read missing want=%s got=%s\n", want, buffer);
    return -1;
}

static int phase(const char *tag) {
    int master = -1;
    int slave = -1;
    char name[128];

    memset(name, 0, sizeof(name));
    if (openpty(&master, &slave, name, NULL, NULL) < 0)
        return fail("openpty", -1);
    printf("pty-alloc: %s master=%d slave=%d name=%s\n", tag, master, slave, name);
    if (strncmp(name, "/dev/pts/", 9) != 0)
        return fail("name", -1);
    if (grantpt(master) != 0)
        return fail("grantpt", -1);
    if (unlockpt(master) != 0)
        return fail("unlockpt", -1);
    char pts[128];
    memset(pts, 0, sizeof(pts));
    if (ptsname_r(master, pts, sizeof(pts)) != 0)
        return fail("ptsname_r", -1);
    printf("pty-alloc: %s ptsname=%s\n", tag, pts);
    if (write(master, "hello\n", 6) != 6)
        return fail("write-master", -1);
    if (read_expect(slave, "hello") < 0)
        return fail("read-slave", -1);
    if (write(slave, "world\n", 6) != 6)
        return fail("write-slave", -1);
    if (read_expect(master, "world") < 0)
        return fail("read-master", -1);
    printf("pty-alloc: %s roundtrip=ok\n", tag);
    close(slave);
    close(master);
    return 0;
}

int main(void) {
    printf("pty-alloc: start uid=%d\n", (int)getuid());
    if (phase("root") != 0)
        return 1;
    if (setgid(1000) != 0 || setuid(1000) != 0)
        return fail("setuid", -1);
    printf("pty-alloc: uid=%d\n", (int)getuid());
    if (phase("user") != 0)
        return 1;
    printf("pty-alloc: ok\n");
    return 0;
}
