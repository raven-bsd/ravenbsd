/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K499: openpty must accept a NULL name. OpenSSH calls it that way (sshpty.c:63); FreeBSD
 * lib/libutil/pty.c:74 and NetBSD pty.c:77 make the name optional. The test also asserts the
 * required NULL out-pointers fail with EINVAL and that two consecutive NULL-name calls get
 * distinct pairs; ptyd must report files=0 pairs=0 after the closes. */

#include <errno.h>
#include <libutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

#include <sys/ioctl.h>

static int fail_call(const char *what, int rc) {
    printf("openpty-null: fail %s rc=%d errno=%d\n", what, rc, errno);
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
            if (strstr(buffer, want) != NULL) {
                return 0;
            }
            continue;
        }
        if (count == 0) {
            break;
        }
        if (errno == EAGAIN || errno == EINTR) {
            usleep(1000);
            continue;
        }
        printf("openpty-null: read error errno=%d\n", errno);
        return -1;
    }
    printf("openpty-null: read missing want=%s got=%s\n", want, buffer);
    return -1;
}

static int roundtrip(int master, int slave) {
    if (write(master, "ping\n", 5) != 5) {
        return fail_call("write-master", -1);
    }
    if (read_expect(slave, "ping") < 0) {
        return fail_call("read-slave", -1);
    }
    if (write(slave, "pong\n", 5) != 5) {
        return fail_call("write-slave", -1);
    }
    if (read_expect(master, "pong") < 0) {
        return fail_call("read-master", -1);
    }
    return 0;
}

int main(void) {
    int master = -1;
    int slave = -1;
    int rc;
    char *pts;

    printf("openpty-null: start uid=%d\n", (int)getuid());

    /* The exact OpenSSH call: NULL name, the core K499 red. */
    master = -1;
    slave = -1;
    errno = 0;
    rc = openpty(&master, &slave, NULL, NULL, NULL);
    if (rc != 0) {
        return fail_call("null-name", rc);
    }
    pts = ptsname(master);
    if (pts == NULL || strncmp(pts, "/dev/pts/", 9) != 0) {
        return fail_call("null-name-pts", -1);
    }
    printf("openpty-null: null-name master=%d slave=%d pts=%s\n", master, slave, pts);
    if (roundtrip(master, slave) != 0) {
        return 1;
    }
    printf("openpty-null: null-name roundtrip=ok\n");
    close(slave);
    close(master);

    /* Only master and slave are required; each missing out-pointer is EINVAL. */
    slave = -1;
    errno = 0;
    rc = openpty(NULL, &slave, NULL, NULL, NULL);
    if (rc != -1 || errno != EINVAL) {
        return fail_call("null-master", rc);
    }
    printf("openpty-null: null-master rc=-1 errno=%d\n", errno);

    master = -1;
    errno = 0;
    rc = openpty(&master, NULL, NULL, NULL, NULL);
    if (rc != -1 || errno != EINVAL) {
        return fail_call("null-slave", rc);
    }
    printf("openpty-null: null-slave rc=-1 errno=%d\n", errno);

    {
        int first_master = -1;
        int first_slave = -1;
        int second_master = -1;
        int second_slave = -1;
        char first_pts[128];
        char second_pts[128];

        memset(first_pts, 0, sizeof(first_pts));
        memset(second_pts, 0, sizeof(second_pts));
        errno = 0;
        rc = openpty(&first_master, &first_slave, NULL, NULL, NULL);
        if (rc != 0) {
            return fail_call("consecutive-first", rc);
        }
        pts = ptsname(first_master);
        if (pts == NULL) {
            return fail_call("consecutive-first-pts", -1);
        }
        strncpy(first_pts, pts, sizeof(first_pts) - 1);
        errno = 0;
        rc = openpty(&second_master, &second_slave, NULL, NULL, NULL);
        if (rc != 0) {
            return fail_call("consecutive-second", rc);
        }
        pts = ptsname(second_master);
        if (pts == NULL) {
            return fail_call("consecutive-second-pts", -1);
        }
        strncpy(second_pts, pts, sizeof(second_pts) - 1);
        if (strcmp(first_pts, second_pts) == 0 || first_master == second_master ||
            first_slave == second_slave) {
            return fail_call("consecutive-distinct", -1);
        }
        printf("openpty-null: consecutive first=%d/%d pts=%s second=%d/%d pts=%s distinct=ok\n",
               first_master, first_slave, first_pts, second_master, second_slave, second_pts);
        close(first_slave);
        close(first_master);
        close(second_slave);
        close(second_master);
    }

    {
        char name[128];
        int named_master = -1;
        int named_slave = -1;

        memset(name, 0, sizeof(name));
        errno = 0;
        rc = openpty(&named_master, &named_slave, name, NULL, NULL);
        if (rc != 0) {
            return fail_call("named", rc);
        }
        if (strncmp(name, "/dev/pts/", 9) != 0) {
            return fail_call("named-path", -1);
        }
        printf("openpty-null: named name=%s\n", name);
        close(named_slave);
        close(named_master);
    }

    printf("openpty-null: ok\n");
    return 0;
}
