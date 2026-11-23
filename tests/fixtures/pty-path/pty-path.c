// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495 rev174 test-only pty path fixture. In a live `ssh -tt` session it checks that the
// /dev/pts mount and the session's slave path resolve, that the path maps to the session's slave,
// that a new pair's /dev/pts/<N> stat/open maps to the same slave, and that the path is released
// after close. It is test tooling, not product code.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <termios.h>
#include <unistd.h>

static int same_inode(int left, int right) {
    struct stat a;
    struct stat b;
    if (fstat(left, &a) != 0 || fstat(right, &b) != 0) {
        return 0;
    }
    return a.st_dev == b.st_dev && a.st_ino == b.st_ino;
}

int main(void) {
    struct stat st;
    int mount_stat = stat("/dev/pts", &st);
    // The `/dev/pts/.keep` placeholder keeps a real initfs directory at the mount point, so
    // S_ISDIR alone also holds when the mount is absent. The pty scheme root reports inode 0;
    // the placeholder's initfs directory has a nonzero inode, so this row fails on a missing
    // mount instead of accepting the placeholder.
    int mount_ok = mount_stat == 0 && S_ISDIR(st.st_mode) && st.st_ino == 0;
    printf("PTY-PATH mount=/dev/pts stat=%s ino=%llu errno=%d\n", mount_ok ? "ok" : "fail",
           mount_stat == 0 ? (unsigned long long)st.st_ino : 0ULL, errno);

    char *session = ttyname(STDIN_FILENO);
    int session_stat = 0;
    int session_same = 0;
    int session_open = -1;
    if (session != NULL) {
        session_stat = stat(session, &st) == 0;
        session_open = open(session, O_RDWR | O_NOCTTY);
        if (session_open >= 0) {
            session_same = same_inode(STDIN_FILENO, session_open);
        }
    }
    printf("PTY-PATH session path=%s stat=%s open=%d same=%d\n",
           session == NULL ? "(none)" : session, session_stat ? "ok" : "fail", session_open,
           session_same);
    if (session_open >= 0) {
        (void)close(session_open);
    }

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    int pair_stat = 0;
    int pair_slave = -1;
    int pair_path = -1;
    int pair_same = 0;
    int release = 0;
    char *pair_name = NULL;
    if (master >= 0 && grantpt(master) == 0 && unlockpt(master) == 0) {
        pair_name = ptsname(master);
        if (pair_name != NULL) {
            pair_stat = stat(pair_name, &st) == 0;
            pair_slave = open(pair_name, O_RDWR | O_NOCTTY);
            pair_path = pair_slave >= 0 ? open(pair_name, O_RDWR | O_NOCTTY) : -1;
            if (pair_slave >= 0 && pair_path >= 0) {
                pair_same = same_inode(pair_slave, pair_path);
            }
        }
        if (pair_path >= 0) {
            (void)close(pair_path);
        }
        if (pair_slave >= 0) {
            (void)close(pair_slave);
        }
        (void)close(master);
        if (pair_name != NULL) {
            struct stat gone;
            release = stat(pair_name, &gone) != 0 && errno == ENOENT;
        }
    }
    printf("PTY-PATH pair path=%s stat=%s open=%d same=%d release=%s\n",
           pair_name == NULL ? "(none)" : pair_name, pair_stat ? "ok" : "fail", pair_slave,
           pair_same, release ? "ok" : "fail");

    int ok = mount_ok && session != NULL && session_stat && session_same && pair_name != NULL &&
             pair_stat && pair_slave >= 0 && pair_same && release;
    printf("PTY-PATH %s\n", ok ? "ok" : "FAIL");
    return ok ? 0 : 1;
}
