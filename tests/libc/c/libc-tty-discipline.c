/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K485: the stored termios bits must change pty behaviour. One row per behaviour, driven only
 * through tcsetattr/tcgetattr and read/write on a pty pair:
 *   canonical VERASE/VKILL/VEOF/VWERASE, ISIG VINTR, OPOST/ONLCR on output,
 *   ICRNL/INLCR/IGNCR/ISTRIP on input, and VMIN/VTIME read timing.
 * Every row prints its input, expected and observed values. VMIN/VTIME are measured with a
 * monotonic clock and a range, never a single number and never a busy spin. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

static int failures;

static int fail_row(const char *row, const char *what, long got) {
    printf("tty-discipline: row=%s FAIL %s got=%ld errno=%d\n", row, what, got, errno);
    fflush(stdout);
    failures++;
    return 1;
}

static long now_ms(void) {
    struct timespec ts;
    memset(&ts, 0, sizeof(ts));
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
        return -1;
    return (long)ts.tv_sec * 1000L + (long)(ts.tv_nsec / 1000000L);
}

/* Wake from pause when a queued signal arrives; alarm(1) bounds the no-signal case. The libc
 * process-signal queue is the observable side of the kernel delivery; the ptyd serial records
 * the signal number and target. Returns 1 for a prompt signal, 0 for the alarm timeout, -1 for
 * an unexpected pause result. */
static int observe_signal(void) {
    long start = now_ms();
    (void)alarm(1);
    errno = 0;
    int result = pause();
    long elapsed = now_ms() - start;
    (void)alarm(0);
    if (result != -1 || errno != EINTR)
        return -1;
    printf("tty-discipline: signal-observe elapsed_ms=%ld\n", elapsed);
    fflush(stdout);
    return elapsed < 500 ? 1 : 0;
}

static int write_all(int fd, const void *data, size_t length) {
    const unsigned char *bytes = data;
    size_t offset = 0;
    while (offset < length) {
        ssize_t count = write(fd, bytes + offset, length - offset);
        if (count < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (count == 0)
            return -1;
        offset += (size_t)count;
    }
    return 0;
}

static int set_foreground(int master) {
    if (setpgid(0, 0) != 0 && errno != EPERM)
        return -1;
    int own = getpgrp();
    if (own <= 0)
        return -1;
    if (tcsetpgrp(master, own) != 0)
        return -1;
    return own;
}

static int open_pair(int *master, int *slave, char *name, size_t capacity) {
    int opened = posix_openpt(O_RDWR | O_NOCTTY);
    if (opened < 0)
        return -1;
    if (grantpt(opened) != 0 || unlockpt(opened) != 0) {
        close(opened);
        return -1;
    }
    char *path = ptsname(opened);
    if (path == NULL || strlen(path) >= capacity) {
        close(opened);
        return -1;
    }
    strcpy(name, path);
    int peer = open(name, O_RDWR);
    if (peer < 0) {
        close(opened);
        return -1;
    }
    *master = opened;
    *slave = peer;
    return 0;
}

static int read_timeout(int fd, unsigned char *buffer, size_t length, int timeout_ms) {
    for (int spins = 0; spins < (timeout_ms / 10) + 1; spins++) {
        struct pollfd waiting;
        waiting.fd = fd;
        waiting.events = POLLIN;
        waiting.revents = 0;
        int ready = poll(&waiting, 1, 10);
        if (ready < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (ready == 0)
            continue;
        ssize_t count = read(fd, buffer, length);
        if (count >= 0)
            return (int)count;
        if (errno != EAGAIN && errno != EINTR)
            return -1;
    }
    errno = ETIMEDOUT;
    return -1;
}

static int expect_bytes(const char *row, int fd, const unsigned char *want, size_t length,
                        int timeout_ms) {
    unsigned char got[128];
    memset(got, 0, sizeof(got));
    size_t total = 0;
    while (total < length) {
        int count = read_timeout(fd, got + total, length - total, timeout_ms);
        if (count < 0)
            return fail_row(row, "read", count);
        if (count == 0)
            break;
        total += (size_t)count;
    }
    if (total != length || memcmp(got, want, length) != 0) {
        printf("tty-discipline: row=%s read got=%zu want=%zu hex=", row, total, length);
        for (size_t index = 0; index < total; index++)
            printf("%02x", got[index]);
        printf("\n");
        failures++;
        return 1;
    }
    printf("tty-discipline: row=%s ok\n", row);
    fflush(stdout);
    return 0;
}

static int expect_silent(const char *row, int fd, int timeout_ms) {
    unsigned char byte = 0;
    int count = read_timeout(fd, &byte, 1, timeout_ms);
    if ((count < 0 && errno == ETIMEDOUT) || count == 0) {
        printf("tty-discipline: row=%s silent ok\n", row);
        return 0;
    }
    printf("tty-discipline: row=%s unexpected data count=%d byte=%u\n", row, count,
           (unsigned)byte);
    failures++;
    return 1;
}

static void raw_termios(struct termios *term, tcflag_t iflag) {
    memset(term, 0, sizeof(*term));
    term->c_iflag = iflag;
    term->c_oflag = 0;
    term->c_cflag = CS8 | CREAD | B38400;
    term->c_lflag = 0;
    term->c_cc[VMIN] = 1;
    term->c_cc[VTIME] = 0;
}

static void canonical_termios(struct termios *term, int echo) {
    raw_termios(term, 0);
    term->c_lflag = ICANON | (echo ? ECHO : 0);
    term->c_cc[VINTR] = 3;
    term->c_cc[VQUIT] = 28;
    term->c_cc[VSUSP] = 26;
    term->c_cc[VERASE] = 127;
    term->c_cc[VKILL] = 21;
    term->c_cc[VEOF] = 4;
    term->c_cc[VWERASE] = 23;
}

static int row_canonical(const char *name, const unsigned char *input, size_t input_len,
                         const unsigned char *want, size_t want_len) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row(name, "open", -1);
    struct termios term;
    canonical_termios(&term, 0);
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row(name, "tcsetattr", -1);
    if (write(master, input, input_len) != (ssize_t)input_len)
        return fail_row(name, "write", -1);
    int result = expect_bytes(name, slave, want, want_len, 1000);
    close(slave);
    close(master);
    return result;
}

static int row_eof(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("canonical-eof", "open", -1);
    struct termios term;
    canonical_termios(&term, 0);
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("canonical-eof", "tcsetattr", -1);
    if (write(master, "abc\x04", 4) != 4)
        return fail_row("canonical-eof", "write-line", -1);
    if (expect_bytes("canonical-eof", slave, (const unsigned char *)"abc\n", 4, 1000) != 0)
        return 1;
    if (write(master, "\x04", 1) != 1)
        return fail_row("canonical-eof-empty", "write-eof", -1);
    unsigned char byte = 0;
    int count = read_timeout(slave, &byte, 1, 1000);
    if (count != 0) {
        printf("tty-discipline: row=canonical-eof-empty FAIL read=%d\n", count);
        failures++;
        return 1;
    }
    printf("tty-discipline: row=canonical-eof-empty ok\n");
    close(slave);
    close(master);
    return 0;
}

static int row_isig(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("isig-intr", "open", -1);
    struct termios term;
    canonical_termios(&term, 1);
    term.c_lflag |= ISIG;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("isig-intr", "tcsetattr", -1);
    int initial = tcgetpgrp(master);
    if (initial != 0)
        return fail_row("isig-intr", "initial-pgrp", initial);
    if (write_all(master, "\x03", 1) != 0)
        return fail_row("isig-intr", "write-intr", -1);
    if (write_all(master, "abc\n", 4) != 0)
        return fail_row("isig-intr", "write-line", -1);
    if (expect_bytes("isig-intr-data", slave, (const unsigned char *)"abc\n", 4, 500) != 0)
        return 1;
    if (expect_bytes("isig-intr-echo", master, (const unsigned char *)"^C\r\n", 4, 500) != 0)
        return 1;
    if (observe_signal() != 0)
        return fail_row("isig-intr", "pgrp-unset-signal", -1);
    printf("tty-discipline: row=isig-intr sigint=0 pgrp-unset ok\n");
    fflush(stdout);
    close(slave);
    close(master);
    return 0;
}

static int row_isig_foreground(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("isig-foreground", "open", -1);
    struct termios term;
    canonical_termios(&term, 1);
    term.c_lflag |= ISIG;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("isig-foreground", "tcsetattr", -1);
    int own = set_foreground(master);
    if (own <= 0)
        return fail_row("isig-foreground", "tcsetpgrp", -1);
    int read_back = tcgetpgrp(master);
    if (read_back != own)
        return fail_row("isig-foreground", "get-after-set", read_back);
    if (write_all(master, "\x03", 1) != 0)
        return fail_row("isig-foreground", "write-intr", -1);
    if (write_all(master, "abc\n", 4) != 0)
        return fail_row("isig-foreground", "write-line", -1);
    if (expect_bytes("isig-foreground-data", slave, (const unsigned char *)"abc\n", 4, 1000) !=
        0)
        return 1;
    if (expect_bytes("isig-foreground-echo", master, (const unsigned char *)"^C\r\n", 4, 1000) !=
        0)
        return 1;
    if (observe_signal() != 1)
        return fail_row("isig-foreground", "sigint-count", -1);
    printf("tty-discipline: row=isig-foreground sigint=1 ok\n");
    fflush(stdout);
    close(slave);
    close(master);
    return 0;
}

static int row_isig_background(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("isig-background", "open", -1);
    struct termios term;
    canonical_termios(&term, 1);
    term.c_lflag |= ISIG;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("isig-background", "tcsetattr", -1);
    if (set_foreground(master) <= 0)
        return fail_row("isig-background", "tcsetpgrp", -1);

    int pipefd[2];
    if (pipe(pipefd) != 0)
        return fail_row("isig-background", "pipe", -1);
    fflush(stdout);
    pid_t child = fork();
    if (child == 0) {
        close(pipefd[0]);
        if (setpgid(0, 0) != 0)
            _exit(2);
        close(master);
        close(slave);
        unsigned char ready = 'R';
        if (write(pipefd[1], &ready, 1) != 1)
            _exit(3);
        /* A background group must never be woken by the ptyd VINTR. pause() consumes a queued
         * signal through the working libc PROC_SIGRECV path; a silent child at the parent's
         * 300 ms poll is the negative result. The test process exits at the end, so the child
         * does not need a kill path (the libc kill wrapper is outside this ticket). */
        errno = 0;
        int result = pause();
        unsigned char signal = result == -1 && errno == EINTR ? 'S' : 'N';
        ssize_t ignored = write(pipefd[1], &signal, 1);
        (void)ignored;
        _exit(signal == 'S' ? 1 : 0);
    }
    close(pipefd[1]);
    if (child < 0)
        return fail_row("isig-background", "fork", -1);
    unsigned char ready = 0;
    if (read_timeout(pipefd[0], &ready, 1, 1000) != 1 || ready != 'R') {
        close(pipefd[0]);
        return fail_row("isig-background", "child-ready", -1);
    }
    if (write_all(master, "\x03", 1) != 0)
        return fail_row("isig-background", "write-intr", -1);
    if (write_all(master, "abc\n", 4) != 0)
        return fail_row("isig-background", "write-line", -1);
    if (expect_bytes("isig-background-data", slave, (const unsigned char *)"abc\n", 4, 1000) !=
        0)
        return 1;
    if (observe_signal() != 1) {
        close(pipefd[0]);
        return fail_row("isig-background", "foreground-count", -1);
    }
    struct pollfd waiting;
    waiting.fd = pipefd[0];
    waiting.events = POLLIN;
    waiting.revents = 0;
    int background = poll(&waiting, 1, 300) > 0 ? 1 : 0;
    close(pipefd[0]);
    if (background != 0)
        return fail_row("isig-background", "background-signal", background);
    printf("tty-discipline: row=isig-background foreground=1 background=0 ok\n");
    fflush(stdout);
    close(slave);
    close(master);
    return 0;
}

static int row_isig_off(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("isig-off", "open", -1);
    struct termios term;
    canonical_termios(&term, 1);
    term.c_lflag &= ~ISIG;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("isig-off", "tcsetattr", -1);
    if (set_foreground(master) <= 0)
        return fail_row("isig-off", "tcsetpgrp", -1);
    if (write_all(master, "\x03", 1) != 0)
        return fail_row("isig-off", "write-intr", -1);
    if (write_all(master, "abc\n", 4) != 0)
        return fail_row("isig-off", "write-line", -1);
    const unsigned char want[] = {0x03, 'a', 'b', 'c', '\n'};
    if (expect_bytes("isig-off-data", slave, want, sizeof(want), 1000) != 0)
        return 1;
    if (observe_signal() != 0)
        return fail_row("isig-off", "signal-with-isig-off", -1);
    printf("tty-discipline: row=isig-off data=1 sigint=0 ok\n");
    fflush(stdout);
    close(slave);
    close(master);
    return 0;
}

static int row_vquit_foreground(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("isig-quit", "open", -1);
    struct termios term;
    canonical_termios(&term, 1);
    term.c_lflag |= ISIG;
    term.c_cc[VQUIT] = 0x1c;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("isig-quit", "tcsetattr", -1);
    if (set_foreground(master) <= 0)
        return fail_row("isig-quit", "tcsetpgrp", -1);
    if (write_all(master, "\x1c", 1) != 0)
        return fail_row("isig-quit", "write-quit", -1);
    if (write_all(master, "abc\n", 4) != 0)
        return fail_row("isig-quit", "write-line", -1);
    if (expect_bytes("isig-quit-data", slave, (const unsigned char *)"abc\n", 4, 1000) != 0)
        return 1;
    if (expect_bytes("isig-quit-echo", master, (const unsigned char *)"^\\\r\n", 4, 1000) != 0)
        return 1;
    if (observe_signal() != 1)
        return fail_row("isig-quit", "sigquit-count", -1);
    printf("tty-discipline: row=isig-quit sigquit=1 ok\n");
    fflush(stdout);
    close(slave);
    close(master);
    return 0;
}

static int row_output(const char *name, tcflag_t oflag, const unsigned char *want,
                      size_t want_len) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row(name, "open", -1);
    struct termios term;
    raw_termios(&term, 0);
    term.c_oflag = oflag;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row(name, "tcsetattr", -1);
    if (write(slave, "A\n", 2) != 2)
        return fail_row(name, "write", -1);
    int result = expect_bytes(name, master, want, want_len, 1000);
    close(slave);
    close(master);
    return result;
}

static int row_input(const char *name, tcflag_t iflag, unsigned char input,
                     unsigned char want, int silent) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row(name, "open", -1);
    struct termios term;
    raw_termios(&term, iflag);
    if (silent) {
        term.c_cc[VMIN] = 0;
        term.c_cc[VTIME] = 1;
    }
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row(name, "tcsetattr", -1);
    if (write(master, &input, 1) != 1)
        return fail_row(name, "write", -1);
    int result = silent ? expect_silent(name, slave, 200)
                        : expect_bytes(name, slave, &want, 1, 500);
    close(slave);
    close(master);
    return result;
}

static int row_vmin(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("vmin1", "open", -1);
    struct termios term;
    raw_termios(&term, 0);
    term.c_cc[VMIN] = 1;
    term.c_cc[VTIME] = 0;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("vmin1", "tcsetattr", -1);
    // One queued byte: VMIN=1 returns it immediately. A concurrent late write is a separate
    // ptyd wait-queue limitation, reported in the ticket, not claimed here.
    if (write(master, "X", 1) != 1)
        return fail_row("vmin1", "write", -1);
    long start = now_ms();
    unsigned char byte = 0;
    int count = (int)read(slave, &byte, 1);
    long elapsed = now_ms() - start;
    printf("tty-discipline: vmin1 read=%d byte=%u elapsed_ms=%ld\n", count, (unsigned)byte,
           elapsed);
    fflush(stdout);
    if (count != 1 || byte != 'X' || elapsed > 500) {
        printf("tty-discipline: row=vmin1 FAIL count=%d elapsed=%ld\n", count, elapsed);
        failures++;
        close(slave);
        close(master);
        return 1;
    }
    printf("tty-discipline: row=vmin1 ok\n");
    close(slave);
    close(master);
    return 0;
}

static int row_vtime(void) {
    int master = -1;
    int slave = -1;
    char path[64];
    if (open_pair(&master, &slave, path, sizeof(path)) != 0)
        return fail_row("vtime", "open", -1);
    struct termios term;
    raw_termios(&term, 0);
    term.c_cc[VMIN] = 0;
    term.c_cc[VTIME] = 1;
    if (tcsetattr(slave, TCSANOW, &term) != 0)
        return fail_row("vtime", "tcsetattr", -1);
    pid_t child = fork();
    if (child < 0)
        return fail_row("vtime", "fork", -1);
    if (child == 0) {
        close(master);
        long start = now_ms();
        unsigned char byte = 0;
        int count = (int)read(slave, &byte, 1);
        long elapsed = now_ms() - start;
        printf("tty-discipline: vtime child count=%d elapsed_ms=%ld\n", count, elapsed);
        fflush(stdout);
        _exit(count == 0 && elapsed >= 50 && elapsed <= 400 ? 0 : 1);
    }
    close(slave);
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("tty-discipline: row=vtime FAIL status=%d\n", status);
        failures++;
        close(master);
        return 1;
    }
    printf("tty-discipline: row=vtime ok\n");
    close(master);
    return 0;
}

int main(void) {
    printf("tty-discipline: start uid=%d\n", (int)getuid());
    fflush(stdout);

    (void)row_canonical("canonical-erase", (const unsigned char *)"abc\x7f" "d\n", 6,
                        (const unsigned char *)"abd\n", 4);
    (void)row_canonical("canonical-kill", (const unsigned char *)"ab\x15" "cd\n", 6,
                        (const unsigned char *)"cd\n", 3);
    (void)row_canonical("canonical-werase", (const unsigned char *)"one two\x17X\n", 10,
                        (const unsigned char *)"one X\n", 6);
    (void)row_eof();
    (void)row_isig();
    (void)row_isig_foreground();
    (void)row_isig_background();
    (void)row_isig_off();
    (void)row_vquit_foreground();
    (void)row_output("opost-onlcr", OPOST | ONLCR, (const unsigned char *)"A\r\n", 3);
    (void)row_output("opost-off", 0, (const unsigned char *)"A\n", 2);
    (void)row_input("icrnl", ICRNL, '\r', '\n', 0);
    (void)row_input("inlcr", INLCR, '\n', '\r', 0);
    (void)row_input("igncr", ICRNL | IGNCR, '\r', 0, 1);
    (void)row_input("istrip", ISTRIP, 0xff, 0x7f, 0);
    (void)row_vmin();
    (void)row_vtime();

    if (failures != 0) {
        printf("tty-discipline: FAIL rows=%d\n", failures);
        return 1;
    }
    printf("tty-discipline: ok\n");
    return 0;
}
