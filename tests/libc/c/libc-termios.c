/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K444: the OpenSSH pty-req termios contract. OpenSSH's ttymodes.c parses the RFC 4254 section 8
 * mode word into `struct termios` and applies it with tcsetattr(TCSANOW) on the pty slave
 * (session.c pty-req -> ssh_tty_parse_modes). This case applies an exact SSH-style record and
 * asserts every field comes back unchanged on the slave and that the master sees the same
 * record. It then queues output, applies a second tcsetattr(TCSADRAIN), and proves no byte was
 * lost. Any mismatch prints the exact field diff and exits 1. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

static int failures;

static void field(const char *who, const char *name, unsigned long want,
                  unsigned long got, int verbose) {
    if (want == got) {
        if (verbose)
            printf("termios: %s %s=%#lo ok\n", who, name, got);
    } else {
        printf("termios: %s %s FAIL want=%#lo got=%#lo\n", who, name, want, got);
        failures++;
    }
}

static void compare(const char *who, const struct termios *want,
                    const struct termios *got, int verbose) {
    field(who, "iflag", want->c_iflag, got->c_iflag, verbose);
    field(who, "oflag", want->c_oflag, got->c_oflag, verbose);
    field(who, "cflag", want->c_cflag, got->c_cflag, verbose);
    field(who, "lflag", want->c_lflag, got->c_lflag, verbose);
    field(who, "line", want->c_line, got->c_line, verbose);
    field(who, "ispeed", cfgetispeed(want), cfgetispeed(got), verbose);
    field(who, "ospeed", cfgetospeed(want), cfgetospeed(got), verbose);
    int cc_bad = 0;
    for (int index = 0; index < NCCS; index++) {
        if (want->c_cc[index] != got->c_cc[index]) {
            printf("termios: %s c_cc[%d] FAIL want=%u got=%u\n", who, index,
                   (unsigned)want->c_cc[index], (unsigned)got->c_cc[index]);
            failures++;
            cc_bad = 1;
        }
    }
    if (!cc_bad && verbose)
        printf("termios: %s c_cc=ok\n", who);
}

static int read_expect(int fd, const char *want, size_t length) {
    char buffer[128];
    size_t total = 0;
    memset(buffer, 0, sizeof(buffer));
    for (int spins = 0; spins < 4000 && total < length; spins++) {
        ssize_t count = read(fd, buffer + total, sizeof(buffer) - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            continue;
        }
        if (count == 0)
            break;
        if (errno == EAGAIN || errno == EINTR)
            continue;
        printf("termios: drain read error errno=%d\n", errno);
        return -1;
    }
    if (total != length || memcmp(buffer, want, length) != 0) {
        printf("termios: drain FAIL want=%zu got=%zu bytes\n", length, total);
        return -1;
    }
    return 0;
}

int main(void) {
    printf("termios: start uid=%d\n", (int)getuid());

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0)
        return 1;
    char *name = ptsname(master);
    if (name == NULL)
        return 1;
    int slave = open(name, O_RDWR);
    printf("termios: slave-open fd=%d\n", slave);
    if (slave < 0)
        return 1;

    struct termios want;
    memset(&want, 0, sizeof(want));
    want.c_iflag = IGNBRK | ICRNL | IXON;
    want.c_oflag = OPOST | ONLCR;
    want.c_cflag = CS8 | CREAD | B9600;
    want.c_lflag = ISIG | ICANON | ECHO | IEXTEN;
    want.c_line = 0;
    want.c_cc[VINTR] = 3;
    want.c_cc[VQUIT] = 28;
    want.c_cc[VERASE] = 127;
    want.c_cc[VKILL] = 21;
    want.c_cc[VEOF] = 4;
    want.c_cc[VMIN] = 7;
    want.c_cc[VTIME] = 3;
    want.c_cc[VSTART] = 17;
    want.c_cc[VSTOP] = 19;
    want.c_cc[VSUSP] = 26;
    cfsetispeed(&want, B19200);
    cfsetospeed(&want, B38400);

    int applied = tcsetattr(slave, TCSANOW, &want);
    printf("termios: tcsetattr=%d\n", applied);
    if (applied != 0) {
        printf("termios: tcsetattr FAIL errno=%d\n", errno);
        return 1;
    }

    struct termios got;
    memset(&got, 0, sizeof(got));
    if (tcgetattr(slave, &got) != 0) {
        printf("termios: tcgetattr FAIL errno=%d\n", errno);
        return 1;
    }
    compare("slave", &want, &got, 1);

    struct termios master_got;
    memset(&master_got, 0, sizeof(master_got));
    if (tcgetattr(master, &master_got) != 0) {
        printf("termios: master tcgetattr FAIL errno=%d\n", errno);
        return 1;
    }
    compare("master", &want, &master_got, 0);
    printf("termios: master=%s\n", failures == 0 ? "ok" : "FAIL");

    /* TCSADRAIN with pending output: the queued bytes are already in the master queue and must
     * survive the attribute change. ONLCR makes the LF a CRLF. */
    if (write(slave, "drain\n", 6) != 6) {
        printf("termios: drain write FAIL errno=%d\n", errno);
        return 1;
    }
    struct termios next = want;
    next.c_cc[VMIN] = 9;
    next.c_lflag &= (tcflag_t)~ECHO;
    int drained = tcsetattr(slave, TCSADRAIN, &next);
    printf("termios: tcsadrain=%d\n", drained);
    if (drained != 0) {
        printf("termios: tcsadrain FAIL errno=%d\n", errno);
        return 1;
    }
    if (read_expect(master, "drain\r\n", 7) != 0)
        return 1;
    printf("termios: drain=ok\n");

    struct termios after;
    memset(&after, 0, sizeof(after));
    if (tcgetattr(slave, &after) != 0) {
        printf("termios: drain tcgetattr FAIL errno=%d\n", errno);
        return 1;
    }
    compare("drain", &next, &after, 1);

    close(slave);
    close(master);
    if (failures != 0) {
        printf("termios: FAIL fields=%d\n", failures);
        return 1;
    }
    printf("termios: ok\n");
    return 0;
}
