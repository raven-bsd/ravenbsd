// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K434 S8 test-first fixture: the exact call OpenSSH makes in sshpty.c:63,
// `openpty(ptyfd, ttyfd, NULL, NULL, NULL)`. FreeBSD lib/libutil/pty.c:74 allows a NULL name
// (`if (name) strcpy(name, slavename);`); NetBSD lib/libutil/pty.c:77 says `name may be NULL`.
// This fixture is test tooling only, staged by profiles/release.d/90-ssh-openpty.toml.

#include <errno.h>
#include <libutil.h>
#include <stdio.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

#include <sys/ioctl.h>

static void close_pair(int master, int slave) {
    if (master >= 0) {
        (void)close(master);
    }
    if (slave >= 0) {
        (void)close(slave);
    }
}

int main(void) {
    int master = -1;
    int slave = -1;
    int rc_null;
    int err_null;

    /* Sentinel: a correct openpty must return 0 and not leave the caller's errno in play. */
    errno = 1234;
    rc_null = openpty(&master, &slave, NULL, NULL, NULL);
    err_null = errno;
    printf("openpty-null: call=null rc=%d errno=%d master=%d slave=%d\n",
           rc_null, err_null, master, slave);
    close_pair(master, slave);

    /* Control: the same call with a name buffer exercises the pty primitives. */
    {
        char name[128];

        memset(name, 0, sizeof(name));
        master = -1;
        slave = -1;
        errno = 0;
        int rc_name = openpty(&master, &slave, name, NULL, NULL);
        int err_name = errno;

        printf("openpty-null: call=name rc=%d errno=%d name=%s master=%d slave=%d\n",
               rc_name, err_name, rc_name == 0 ? name : "-", master, slave);
        close_pair(master, slave);

        /* EPIPE is the errno OpenSSH printed. The null call must not read it. */
        master = -1;
        slave = -1;
        errno = EPIPE;
        int rc_epipe = openpty(&master, &slave, NULL, NULL, NULL);
        int err_epipe = errno;

        printf("openpty-null: call=epipe-sentinel rc=%d errno=%d strerror=%s\n",
               rc_epipe, err_epipe, strerror(err_epipe));
        close_pair(master, slave);

        int ok = rc_null == 0 && rc_name == 0 && rc_epipe == 0;

        printf("openpty-null: verdict=%s\n", ok ? "PASS" : "FAIL");
        return ok ? 0 : 1;
    }
}
