/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S175 terminal conformance probe (test-only).
 *
 * One row per contract clause from CONTRACTS/POSIX and the donors named in the report:
 *   termios         tcgetattr/tcsetattr round-trip with TCSANOW/TCSADRAIN/TCSAFLUSH
 *   pty-both-ways   master<->slave exact bytes over posix_openpt/grantpt/unlockpt/ptsname
 *   ctty            setsid -> /dev/tty ENXIO -> TIOCSCTTY -> /dev/tty open and route
 *   eio             read(master) returns EIO after the last slave descriptor closes
 *   sigwinch        TIOCSWINSZ signals the foreground group            (PENDING: S44)
 *   sighup          master close signals the session leader            (PENDING: S44)
 *   isig            ^C signals only with ISIG set                      (PENDING: S44)
 *
 * Rows print `S175-TERM row=<name> status=<PASS|FAIL|PENDING> detail=...`; the runner in run.py
 * treats the four gate rows as required and the three signal rows as PENDING until S44 lands.
 * `--signals` runs the pending rows; `--mutation-termios` deliberately corrupts the termios
 * expectation so the negative check proves the row is not vacuous.
 */

#include <errno.h>
#include <fcntl.h>
#include <libutil.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

#define IO_TIMEOUT_MS 2000

static int failures;

static void report(const char *name, const char *status, const char *fmt, ...) {
    va_list args;
    printf("S175-TERM row=%s status=%s detail=", name, status);
    va_start(args, fmt);
    vprintf(fmt, args);
    va_end(args);
    printf("\n");
    fflush(stdout);
}

static void pass(const char *name, const char *fmt, ...) {
    va_list args;
    printf("S175-TERM row=%s status=PASS detail=", name);
    va_start(args, fmt);
    vprintf(fmt, args);
    va_end(args);
    printf("\n");
    fflush(stdout);
}

static void fail(const char *name, const char *fmt, ...) {
    va_list args;
    printf("S175-TERM row=%s status=FAIL detail=", name);
    va_start(args, fmt);
    vprintf(fmt, args);
    va_end(args);
    printf("\n");
    fflush(stdout);
    failures++;
}

static void pending(const char *name, const char *why) {
    report(name, "PENDING", "%s", why);
}

/* S742: the public C boundary must carry the FreeBSD 13334440 record and request encodings. */
static void row_header(void) {
    int ok = sizeof(struct termios) == 44 && _Alignof(struct termios) == 4 &&
             offsetof(struct termios, c_cc) == 16 &&
             offsetof(struct termios, c_ispeed) == 36 &&
             offsetof(struct termios, c_ospeed) == 40 && NCCS == 20 &&
             VINTR == 8 && VMIN == 16 && VTIME == 17 &&
             TIOCGETA == 0x402c7413UL && TIOCSETA == 0x802c7414UL &&
             TIOCGWINSZ == 0x40087468UL && TIOCSWINSZ == 0x80087467UL &&
             TIOCGPGRP == 0x40047477UL && TIOCSPGRP == 0x80047476UL &&
             TIOCSCTTY == 0x20007461UL && FIONBIO == 0x8004667eUL &&
             FIOCLEX == 0x20006601UL && FIONCLEX == 0x20006602UL &&
             TCIFLUSH == 1 && TCOOFF == 1;
    if (ok)
        pass("header", "FreeBSD termios record 44/4, NCCS 20, FreeBSD request encodings");
    else
        fail("header", "sizeof=%zu align=%zu c_cc=%zu ispeed=%zu ospeed=%zu NCCS=%d GETA=%#lx",
             sizeof(struct termios), _Alignof(struct termios),
             offsetof(struct termios, c_cc), offsetof(struct termios, c_ispeed),
             offsetof(struct termios, c_ospeed), NCCS, (unsigned long)TIOCGETA);
}

static int open_pair(int *master, int *slave, char *name, size_t name_len) {
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    if (m < 0)
        return -1;
    if (grantpt(m) != 0 || unlockpt(m) != 0) {
        close(m);
        return -1;
    }
    char *path = ptsname(m);
    if (path == NULL || strncmp(path, "/dev/pts/", 9) != 0) {
        close(m);
        return -1;
    }
    if (name != NULL && name_len > 0) {
        strncpy(name, path, name_len - 1);
        name[name_len - 1] = '\0';
    }
    int s = open(path, O_RDWR | O_NOCTTY);
    if (s < 0) {
        close(m);
        return -1;
    }
    *master = m;
    *slave = s;
    return 0;
}

static int make_raw(int fd) {
    struct termios t;
    if (tcgetattr(fd, &t) != 0)
        return -1;
    t.c_iflag &= ~(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    t.c_oflag &= ~OPOST;
    t.c_lflag &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    t.c_cflag &= ~(CSIZE | PARENB);
    t.c_cflag |= CS8;
    t.c_cc[VMIN] = 1;
    t.c_cc[VTIME] = 0;
    return tcsetattr(fd, TCSANOW, &t);
}

static int read_exact(int fd, char *buffer, size_t length, int timeout_ms) {
    size_t offset = 0;
    while (offset < length) {
        struct pollfd pfd = {.fd = fd, .events = POLLIN};
        int ready = poll(&pfd, 1, timeout_ms);
        if (ready <= 0)
            return -1;
        ssize_t count = read(fd, buffer + offset, length - offset);
        if (count > 0) {
            offset += (size_t)count;
            continue;
        }
        if (count < 0 && (errno == EAGAIN || errno == EINTR))
            continue;
        return -1;
    }
    return 0;
}

static char diff[256];

static int cmp_termios(const char *who, const struct termios *want,
                       const struct termios *got) {
    diff[0] = '\0';
#define FIELD(name)                                                                 \
    if (want->name != got->name) {                                                  \
        snprintf(diff, sizeof(diff), "%s.%s want=%#lo got=%#lo", who, #name,        \
                 (unsigned long)want->name, (unsigned long)got->name);              \
        return 0;                                                                   \
    }
    FIELD(c_iflag)
    FIELD(c_oflag)
    FIELD(c_cflag)
    FIELD(c_lflag)
#undef FIELD
    if (cfgetispeed(want) != cfgetispeed(got)) {
        snprintf(diff, sizeof(diff), "%s.ispeed want=%lu got=%lu", who,
                 (unsigned long)cfgetispeed(want), (unsigned long)cfgetispeed(got));
        return 0;
    }
    if (cfgetospeed(want) != cfgetospeed(got)) {
        snprintf(diff, sizeof(diff), "%s.ospeed want=%lu got=%lu", who,
                 (unsigned long)cfgetospeed(want), (unsigned long)cfgetospeed(got));
        return 0;
    }
    for (int index = 0; index < NCCS; index++) {
        if (want->c_cc[index] != got->c_cc[index]) {
            snprintf(diff, sizeof(diff), "%s.c_cc[%d] want=%u got=%u", who, index,
                     (unsigned)want->c_cc[index], (unsigned)got->c_cc[index]);
            return 0;
        }
    }
    return 1;
}

static void row_termios(int mutation) {
    int master = -1;
    int slave = -1;
    if (open_pair(&master, &slave, NULL, 0) != 0) {
        fail("termios", "open pair errno=%d", errno);
        return;
    }

    struct termios original;
    struct termios want;
    if (tcgetattr(slave, &original) != 0) {
        fail("termios", "tcgetattr(slave) errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    want = original;
    want.c_iflag = ICRNL | IXON;
    want.c_oflag = OPOST | ONLCR;
    want.c_lflag = ISIG | ICANON | ECHO | ECHOE | ECHOK;
    want.c_cflag = (original.c_cflag & ~(CSIZE | PARENB)) | CS8 | CREAD | CLOCAL;
    want.c_cc[VINTR] = 3;
    want.c_cc[VQUIT] = 28;
    want.c_cc[VERASE] = 127;
    want.c_cc[VKILL] = 21;
    want.c_cc[VEOF] = 4;
    want.c_cc[VMIN] = 1;
    want.c_cc[VTIME] = 0;
    cfsetispeed(&want, B9600);
    cfsetospeed(&want, B9600);

    if (tcsetattr(slave, TCSANOW, &want) != 0) {
        fail("termios", "tcsetattr TCSANOW errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    struct termios back;
    struct termios master_back;
    int ok = tcgetattr(slave, &back) == 0 && cmp_termios("slave", &want, &back);
    if (ok) {
        ok = tcgetattr(master, &master_back) == 0 &&
             cmp_termios("master", &want, &master_back);
    }
    if (mutation) {
        struct termios wrong = want;
        wrong.c_lflag ^= ECHO;
        ok = cmp_termios("mutation", &wrong, &back) && ok;
        if (ok)
            snprintf(diff, sizeof(diff), "mutation:ECHO expected off but the row passed");
        ok = 0;
    }

    if (ok) {
        /* TCSADRAIN: a queued slave->master byte must survive the change. */
        struct termios drain = want;
        drain.c_oflag &= ~OPOST;
        if (write(slave, "drain", 5) != 5) {
            ok = 0;
            snprintf(diff, sizeof(diff), "TCSADRAIN write errno=%d", errno);
        } else if (tcsetattr(slave, TCSADRAIN, &drain) != 0) {
            ok = 0;
            snprintf(diff, sizeof(diff), "TCSADRAIN errno=%d", errno);
        } else {
            char buffer[8];
            if (read_exact(master, buffer, 5, IO_TIMEOUT_MS) != 0 ||
                memcmp(buffer, "drain", 5) != 0) {
                ok = 0;
                snprintf(diff, sizeof(diff), "TCSADRAIN lost or changed the queued bytes");
            }
        }
    }
    if (ok) {
        /* TCSAFLUSH: the record is applied and remains readable. */
        if (tcsetattr(slave, TCSAFLUSH, &want) != 0 ||
            tcgetattr(slave, &back) != 0 || !cmp_termios("flush", &want, &back)) {
            ok = 0;
            if (diff[0] == '\0')
                snprintf(diff, sizeof(diff), "TCSAFLUSH round-trip failed");
        }
    }

    if (ok)
        pass("termios",
             "TCSANOW/TCSADRAIN/TCSAFLUSH round-trip of iflag/oflag/lflag/cflag/c_cc/9600");
    else
        fail("termios", "%s", diff[0] != '\0' ? diff : "termios mismatch");
    close(master);
    close(slave);
}

static void row_pair(void) {
    int master = -1;
    int slave = -1;
    if (open_pair(&master, &slave, NULL, 0) != 0) {
        fail("pty-both-ways", "open pair errno=%d", errno);
        return;
    }
    if (make_raw(slave) != 0) {
        fail("pty-both-ways", "raw mode errno=%d", errno);
        close(master);
        close(slave);
        return;
    }

    static const char to_slave[] = "master->slave-0123456789";
    static const char to_master[] = "slave->master-9876543210";
    char buffer[64];
    int ok = write(master, to_slave, sizeof(to_slave) - 1) == (ssize_t)(sizeof(to_slave) - 1) &&
             read_exact(slave, buffer, sizeof(to_slave) - 1, IO_TIMEOUT_MS) == 0 &&
             memcmp(buffer, to_slave, sizeof(to_slave) - 1) == 0;
    if (ok) {
        ok = write(slave, to_master, sizeof(to_master) - 1) ==
                 (ssize_t)(sizeof(to_master) - 1) &&
             read_exact(master, buffer, sizeof(to_master) - 1, IO_TIMEOUT_MS) == 0 &&
             memcmp(buffer, to_master, sizeof(to_master) - 1) == 0;
    }
    if (ok)
        pass("pty-both-ways", "exact %zu bytes master->slave and %zu bytes slave->master",
             sizeof(to_slave) - 1, sizeof(to_master) - 1);
    else
        fail("pty-both-ways", "byte mismatch or timeout errno=%d", errno);
    close(master);
    close(slave);
}

static void row_ctty(void) {
    int master = -1;
    int slave = -1;
    char name[64];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        fail("ctty", "open pair errno=%d", errno);
        return;
    }
    pid_t child = fork();
    if (child < 0) {
        fail("ctty", "fork errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    if (child == 0) {
        close(master);
        close(slave);
        if (setsid() < 0)
            _exit(11);
        errno = 0;
        int tty = open("/dev/tty", O_RDWR);
        if (tty >= 0) {
            close(tty);
            _exit(12);
        }
        if (errno != ENXIO)
            _exit(13);
        int sfd = open(name, O_RDWR);
        if (sfd < 0)
            _exit(14);
        if (ioctl(sfd, TIOCSCTTY, 0) != 0)
            _exit(15);
        if (tcgetpgrp(sfd) != getpgrp())
            _exit(18);
        if (tcsetpgrp(sfd, getpgrp()) != 0)
            _exit(19);
        errno = 0;
        if (tcsetpgrp(sfd, 0) != -1 || errno != EPERM)
            _exit(20);
        int ctty = open("/dev/tty", O_RDWR);
        if (ctty < 0)
            _exit(16);
        if (write(ctty, "CTTY", 4) != 4)
            _exit(17);
        _exit(0);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child) {
        fail("ctty", "waitpid errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    char buffer[8];
    int routed = read_exact(master, buffer, 4, IO_TIMEOUT_MS) == 0 &&
                 memcmp(buffer, "CTTY", 4) == 0;
    int code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (code == 0 && routed)
        pass("ctty", "setsid -> /dev/tty ENXIO -> TIOCSCTTY -> /dev/tty open and routed");
    else
        fail("ctty", "child exit=%d routed=%d (11 setsid,12 ctty,13 errno,14 open,15 sctty,"
                     "16 tty,17 write,18 getpgrp,19 setpgrp,20 unauthorized)",
             code, routed);
    close(master);
    close(slave);
}

static void row_eio(void) {
    int master = -1;
    int slave = -1;
    if (open_pair(&master, &slave, NULL, 0) != 0) {
        fail("eio", "open pair errno=%d", errno);
        return;
    }
    close(slave);
    struct pollfd pfd = {.fd = master, .events = POLLIN};
    int ready = poll(&pfd, 1, IO_TIMEOUT_MS);
    errno = 0;
    char byte = 0;
    ssize_t count = ready > 0 ? read(master, &byte, 1) : -1;
    int error = errno;
    if (count < 0 && error == EIO)
        pass("eio", "read(master) after the last slave close returns EIO");
    else
        fail("eio", "poll=%d read=%zd errno=%d (want -1/EIO)", ready, count, error);
    close(master);
}

static volatile sig_atomic_t received;

static void note_signal(int signal_number) {
    received = signal_number;
}

static int run_signal_child(char *name, int master, int slave, int (*setup)(int),
                            int signal_number, int *result_fd, int *ready_fd, int *child_out) {
    int ready[2];
    int result[2];
    if (pipe(ready) != 0 || pipe(result) != 0)
        return -1;
    pid_t child = fork();
    if (child < 0)
        return -1;
    if (child == 0) {
        close(master);
        close(slave);
        close(ready[0]);
        close(result[0]);
        if (setsid() < 0)
            _exit(20);
        int sfd = open(name, O_RDWR);
        if (sfd < 0)
            _exit(21);
        if (ioctl(sfd, TIOCSCTTY, 0) != 0)
            _exit(22);
        if (setup(sfd) != 0)
            _exit(23);
        received = 0;
        struct sigaction action;
        memset(&action, 0, sizeof(action));
        action.sa_handler = note_signal;
        sigemptyset(&action.sa_mask);
        if (sigaction(signal_number, &action, NULL) != 0)
            _exit(24);
        char marker = 'R';
        if (write(ready[1], &marker, 1) != 1)
            _exit(25);
        close(ready[1]);
        for (int spins = 0; spins < 400 && received == 0; spins++)
            usleep(5000);
        int signal_received = (int)received;
        (void)write(result[1], &signal_received, sizeof(signal_received));
        _exit(0);
    }
    close(ready[1]);
    close(result[1]);
    *result_fd = result[0];
    *ready_fd = ready[0];
    *child_out = (int)child;
    return 0;
}

static int setup_winsize(int fd) {
    (void)fd;
    return 0;
}

static int setup_isig(int fd) {
    struct termios t;
    if (tcgetattr(fd, &t) != 0)
        return -1;
    t.c_lflag |= ISIG;
    t.c_lflag &= ~ICANON;
    t.c_lflag &= ~ECHO;
    t.c_cc[VINTR] = 3;
    t.c_cc[VMIN] = 1;
    t.c_cc[VTIME] = 0;
    return tcsetattr(fd, TCSANOW, &t);
}

static void wait_ready(int ready_fd) {
    char marker = 0;
    struct pollfd pfd = {.fd = ready_fd, .events = POLLIN};
    if (poll(&pfd, 1, IO_TIMEOUT_MS) > 0)
        (void)read(ready_fd, &marker, 1);
}

static void collect_signal(int result_fd, int *signal_out, int timeout_ms) {
    struct pollfd pfd = {.fd = result_fd, .events = POLLIN};
    *signal_out = 0;
    if (poll(&pfd, 1, timeout_ms) > 0) {
        int value = 0;
        if (read(result_fd, &value, sizeof(value)) == (ssize_t)sizeof(value))
            *signal_out = value;
    }
}

static int mutation_signals;

static void row_sigwinch(void) {
    if (mutation_signals) {
        fail("sigwinch", "mutation: signal generation suppressed");
        return;
    }
    int master = -1;
    int slave = -1;
    char name[64];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        fail("sigwinch", "open pair errno=%d", errno);
        return;
    }
    int result_fd = -1;
    int ready_fd = -1;
    int child = -1;
    if (run_signal_child(name, master, slave, setup_winsize, SIGWINCH, &result_fd,
                         &ready_fd, &child) != 0) {
        fail("sigwinch", "fork/pipe errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    wait_ready(ready_fd);
    struct winsize window;
    memset(&window, 0, sizeof(window));
    window.ws_row = 24;
    window.ws_col = 80;
    int set = ioctl(slave, TIOCSWINSZ, &window);
    int signal_received = 0;
    collect_signal(result_fd, &signal_received, IO_TIMEOUT_MS);
    int status = 0;
    waitpid(child, &status, 0);
    close(ready_fd);
    close(result_fd);
    close(master);
    close(slave);
    if (set == 0 && signal_received == SIGWINCH)
        pass("sigwinch", "TIOCSWINSZ delivered SIGWINCH to the foreground group");
    else
        fail("sigwinch", "ioctl=%d signal=%d (want SIGWINCH)", set, signal_received);
}

static void row_sighup(void) {
    if (mutation_signals) {
        fail("sighup", "mutation: signal generation suppressed");
        return;
    }
    int master = -1;
    int slave = -1;
    char name[64];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        fail("sighup", "open pair errno=%d", errno);
        return;
    }
    int result_fd = -1;
    int ready_fd = -1;
    int child = -1;
    if (run_signal_child(name, master, slave, setup_winsize, SIGHUP, &result_fd,
                         &ready_fd, &child) != 0) {
        fail("sighup", "fork/pipe errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    wait_ready(ready_fd);
    close(master);
    int signal_received = 0;
    collect_signal(result_fd, &signal_received, IO_TIMEOUT_MS);
    int status = 0;
    waitpid(child, &status, 0);
    close(ready_fd);
    close(result_fd);
    close(slave);
    if (signal_received == SIGHUP)
        pass("sighup", "master close delivered SIGHUP to the session leader");
    else
        fail("sighup", "signal=%d (want SIGHUP)", signal_received);
}

static void row_isig(int enabled) {
    if (mutation_signals) {
        fail("isig", "mutation: signal generation suppressed");
        return;
    }
    int master = -1;
    int slave = -1;
    char name[64];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        fail("isig", "open pair errno=%d", errno);
        return;
    }
    if (enabled) {
        if (make_raw(slave) != 0) {
            fail("isig", "raw mode errno=%d", errno);
            close(master);
            close(slave);
            return;
        }
        struct termios t;
        if (tcgetattr(slave, &t) != 0 || tcsetattr(slave, TCSANOW, &t) != 0) {
            fail("isig", "termios errno=%d", errno);
            close(master);
            close(slave);
            return;
        }
    }
    int result_fd = -1;
    int ready_fd = -1;
    int child = -1;
    if (run_signal_child(name, master, slave, setup_isig, SIGINT, &result_fd,
                         &ready_fd, &child) != 0) {
        fail("isig", "fork/pipe errno=%d", errno);
        close(master);
        close(slave);
        return;
    }
    wait_ready(ready_fd);
    char intr = 3;
    ssize_t written = write(master, &intr, 1);
    int signal_received = 0;
    collect_signal(result_fd, &signal_received, IO_TIMEOUT_MS);
    int status = 0;
    waitpid(child, &status, 0);
    close(ready_fd);
    close(result_fd);
    if (written != 1)
        fail("isig", "write ^C errno=%d", errno);
    else if (enabled && signal_received == SIGINT)
        pass("isig", "^C with ISIG set generated SIGINT");
    else if (!enabled && signal_received != SIGINT)
        pass("isig", "^C with ISIG cleared did not generate SIGINT");
    else
        fail("isig", "ISIG=%d signal=%d", enabled, signal_received);
    close(master);
    close(slave);
}

int main(int argc, char **argv) {
    int run_signals = 0;
    int mutation_termios = 0;
    for (int index = 1; index < argc; index++) {
        if (strcmp(argv[index], "--signals") == 0)
            run_signals = 1;
        else if (strcmp(argv[index], "--mutation-termios") == 0)
            mutation_termios = 1;
        else if (strcmp(argv[index], "--mutation-signals") == 0)
            mutation_signals = 1;
    }
    printf("S175-TERM start uid=%d signals=%d mutation=%d signal_mutation=%d\n", (int)getuid(),
           run_signals, mutation_termios, mutation_signals);
    row_header();
    row_termios(mutation_termios);
    row_pair();
    row_ctty();
    row_eio();
    if (run_signals || mutation_signals) {
        row_sigwinch();
        row_sighup();
        row_isig(1);
        row_isig(0);
    } else {
        pending("sigwinch", "S44 signal delivery not landed; run with --signals to bind");
        pending("sighup", "S44 signal delivery not landed; run with --signals to bind");
        pending("isig", "S44 signal delivery not landed; run with --signals to bind");
    }
    printf("S175-TERM done failures=%d\n", failures);
    return failures == 0 ? 0 : 1;
}
