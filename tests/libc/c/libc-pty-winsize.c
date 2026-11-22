/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K455: the pty window-size contract. OpenSSH sends the client's geometry in pty-req
 * (session.c pty_set_size/TIOCSWINSZ), and programs read it with TIOCGWINSZ or the POSIX.1-2024
 * tcgetwinsize/tcsetwinsize. The pty must store one struct winsize per pair, return it unchanged
 * on both slave and master, and report zero for a fresh pair. A non-tty fd must answer ENOTTY,
 * never ENOSYS. Every mismatch prints the exact field diff and exits 1. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

static int failures;

static void field(const char *who, const char *name, unsigned short want,
                  unsigned short got) {
    if (want == got) {
        printf("pty-winsize: %s %s=%u ok\n", who, name, (unsigned)got);
    } else {
        printf("pty-winsize: %s %s FAIL want=%u got=%u\n", who, name,
               (unsigned)want, (unsigned)got);
        failures++;
    }
}

static void compare(const char *who, const struct winsize *want,
                    const struct winsize *got) {
    field(who, "row", want->ws_row, got->ws_row);
    field(who, "col", want->ws_col, got->ws_col);
    field(who, "xpixel", want->ws_xpixel, got->ws_xpixel);
    field(who, "ypixel", want->ws_ypixel, got->ws_ypixel);
}

static int get_size(const char *who, int fd, const struct winsize *want) {
    struct winsize got;
    memset(&got, 0, sizeof(got));
    int rc = tcgetwinsize(fd, &got);
    printf("pty-winsize: get %s=%d\n", who, rc);
    if (rc != 0) {
        printf("pty-winsize: get %s FAIL errno=%d\n", who, errno);
        return -1;
    }
    compare(who, want, &got);
    return 0;
}

int main(void) {
    printf("pty-winsize: start uid=%d\n", (int)getuid());

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0)
        return 1;
    char *name = ptsname(master);
    if (name == NULL)
        return 1;
    int slave = open(name, O_RDWR);
    printf("pty-winsize: slave-open fd=%d\n", slave);
    if (slave < 0)
        return 1;

    struct winsize first;
    memset(&first, 0, sizeof(first));
    first.ws_row = 43;
    first.ws_col = 120;
    int set1 = tcsetwinsize(slave, &first);
    printf("pty-winsize: set1=%d\n", set1);
    if (set1 != 0) {
        printf("pty-winsize: set1 FAIL errno=%d\n", errno);
        return 1;
    }
    if (get_size("slave", slave, &first) != 0)
        return 1;
    if (get_size("master", master, &first) != 0)
        return 1;

    struct winsize via_ioctl;
    memset(&via_ioctl, 0, sizeof(via_ioctl));
    int get_ioctl = ioctl(slave, TIOCGWINSZ, &via_ioctl);
    printf("pty-winsize: ioctl-get=%d\n", get_ioctl);
    if (get_ioctl != 0) {
        printf("pty-winsize: ioctl-get FAIL errno=%d\n", errno);
        return 1;
    }
    compare("ioctl-slave", &first, &via_ioctl);

    struct winsize second;
    memset(&second, 0, sizeof(second));
    second.ws_row = 24;
    second.ws_col = 80;
    second.ws_xpixel = 640;
    second.ws_ypixel = 480;
    int set2 = ioctl(master, TIOCSWINSZ, &second);
    printf("pty-winsize: set2-ioctl=%d\n", set2);
    if (set2 != 0) {
        printf("pty-winsize: set2-ioctl FAIL errno=%d\n", errno);
        return 1;
    }
    if (get_size("second-slave", slave, &second) != 0)
        return 1;

    struct winsize zeros;
    memset(&zeros, 0, sizeof(zeros));
    int set3 = tcsetwinsize(slave, &zeros);
    printf("pty-winsize: set3=%d\n", set3);
    if (set3 != 0) {
        printf("pty-winsize: set3 FAIL errno=%d\n", errno);
        return 1;
    }
    if (get_size("zeros-master", master, &zeros) != 0)
        return 1;

    int plain = open("/dev/null", O_RDONLY);
    if (plain < 0) {
        printf("pty-winsize: devnull FAIL errno=%d\n", errno);
        return 1;
    }
    struct winsize ignored;
    memset(&ignored, 0, sizeof(ignored));
    errno = 0;
    int get_plain = tcgetwinsize(plain, &ignored);
    printf("pty-winsize: non-tty-get=%d errno=%d\n", get_plain, errno);
    if (get_plain != -1 || errno != ENOTTY) {
        printf("pty-winsize: non-tty-get FAIL\n");
        return 1;
    }
    errno = 0;
    int set_plain = tcsetwinsize(plain, &ignored);
    printf("pty-winsize: non-tty-set=%d errno=%d\n", set_plain, errno);
    if (set_plain != -1 || errno != ENOTTY) {
        printf("pty-winsize: non-tty-set FAIL\n");
        return 1;
    }
    close(plain);

    close(slave);
    close(master);
    if (failures != 0) {
        printf("pty-winsize: FAIL fields=%d\n", failures);
        return 1;
    }
    printf("pty-winsize: ok\n");
    return 0;
}
