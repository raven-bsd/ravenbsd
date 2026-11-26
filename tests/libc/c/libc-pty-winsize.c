/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K455: the pty window-size contract. OpenSSH sends the client's geometry in pty-req
 * (session.c pty_set_size/TIOCSWINSZ), and programs read it with TIOCGWINSZ or the POSIX.1-2024
 * tcgetwinsize/tcsetwinsize. The pty must store one struct winsize per pair, return it unchanged
 * on both slave and master, and report zero for a fresh pair. A non-tty fd must answer ENOTTY,
 * never ENOSYS. Every mismatch prints the exact field diff and exits 1. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

static int failures;

/* K536: one SIGWINCH per actual size change, delivered to the foreground group only. The libc
 * on this base has no asynchronous handler dispatch (PG-7 is absent; see
 * libc/ravenlibc/crates/raven-libc-signal/README.md), so the child consumes the queued signal
 * through the kernel seam (PROC_SIGRECV) and immediately reads the size, which is the action a
 * SIGWINCH handler performs. */
struct winch_report {
    int count;
    int row;
    int col;
};

static volatile sig_atomic_t winch_count;

/* The libc `syscall()` symbol is a one-argument stub and libc `kill()` returns ENOSYS (the
 * procd-kill capability PG-44 is absent). The fixture therefore calls the two documented raw
 * kernel numbers directly: PROC_SIGNAL 0x4A and PROC_SIGRECV 0x4B. */
static long raw_syscall2(long number, long first, long second) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(first), "S"(second)
                     : "rcx", "r11", "memory");
    return result;
}

static int winch_child(int slave, int master, int report, int make_group) {
    unsigned long info[3];
    int pgrp;
    int pid;
    char ready = 'R';

    close(master);
    if (make_group && setpgid(0, 0) != 0)
        return 1;
    pgrp = (int)getpgrp();
    pid = (int)getpid();
    if (write(report, &pgrp, sizeof(pgrp)) != (ssize_t)sizeof(pgrp))
        return 1;
    if (write(report, &pid, sizeof(pid)) != (ssize_t)sizeof(pid))
        return 1;
    if (write(report, &ready, 1) != 1)
        return 1;
    for (;;) {
        struct winsize size;
        struct winch_report value;
        long signo;
        memset(info, 0, sizeof(info));
        signo = raw_syscall2(0x4B, (long)info, 0);
        if (signo == SIGWINCH) {
            memset(&size, 0, sizeof(size));
            (void)tcgetwinsize(slave, &size);
            winch_count++;
            value.count = (int)winch_count;
            value.row = size.ws_row;
            value.col = size.ws_col;
            if (write(report, &value, sizeof(value)) != (ssize_t)sizeof(value))
                return 1;
        } else if (signo == SIGUSR2) {
            break;
        } else if (signo < 0 && signo != -EINTR) {
            break;
        }
    }
    return 0;
}

static pid_t spawn_winch_child(int slave, int master, int make_group, int *report) {
    int report_pipe[2];
    pid_t pid;
    if (pipe(report_pipe) != 0)
        return -1;
    pid = fork();
    if (pid < 0)
        return -1;
    if (pid == 0) {
        close(report_pipe[0]);
        _exit(winch_child(slave, master, report_pipe[1], make_group));
    }
    close(report_pipe[1]);
    *report = report_pipe[0];
    return pid;
}

static int child_ready(pid_t pid, int report, int *pgrp, int *child_pid) {
    char ready = 0;
    if (pid < 0)
        return 0;
    if (read(report, pgrp, sizeof(*pgrp)) != (ssize_t)sizeof(*pgrp))
        return 0;
    if (read(report, child_pid, sizeof(*child_pid)) != (ssize_t)sizeof(*child_pid))
        return 0;
    if (read(report, &ready, 1) != 1 || ready != 'R')
        return 0;
    return 1;
}

/* 1 = a report arrived, 0 = timeout, -1 = error. */
static int child_report(int report, struct winch_report *value, int timeout_ms) {
    struct pollfd waiting;
    int ready;
    waiting.fd = report;
    waiting.events = POLLIN;
    waiting.revents = 0;
    ready = poll(&waiting, 1, timeout_ms);
    if (ready == 0)
        return 0;
    if (ready < 0)
        return -1;
    return read(report, value, sizeof(*value)) == (ssize_t)sizeof(*value) ? 1 : -1;
}

static void child_stop(int child_pid) {
    (void)raw_syscall2(0x4A, child_pid, SIGUSR2);
}

static void drain_parent_signals(void) {
    unsigned long info[3];
    for (;;) {
        memset(info, 0, sizeof(info));
        if (raw_syscall2(0x4B, (long)info, 1) < 0)
            break;
    }
}

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

/* K536 rev166: when the only slave endpoint exits, the master must see EOF
 * (NetBSD tty_pty.c ptsclose -> ttyclose, ptcclose carrier drop; FreeBSD
 * ttydisc_close flushes the discipline). A master read after the child exits
 * must not block: a hung pty keeps the SSH session channel open. */
static void child_exit_eof(void) {
    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0) {
        printf("pty-winsize: close child-exit open FAIL errno=%d\n", errno);
        failures++;
        return;
    }
    if (grantpt(master) != 0 || unlockpt(master) != 0) {
        printf("pty-winsize: close child-exit grant FAIL errno=%d\n", errno);
        failures++;
        close(master);
        return;
    }
    char *name = ptsname(master);
    if (name == NULL) {
        printf("pty-winsize: close child-exit ptsname FAIL errno=%d\n", errno);
        failures++;
        close(master);
        return;
    }
    int parent_slave = open(name, O_RDWR);
    if (parent_slave < 0) {
        printf("pty-winsize: close child-exit slave FAIL errno=%d\n", errno);
        failures++;
        close(master);
        return;
    }
    pid_t child = fork();
    if (child < 0) {
        printf("pty-winsize: close child-exit fork FAIL errno=%d\n", errno);
        failures++;
        close(parent_slave);
        close(master);
        return;
    }
    if (child == 0) {
        close(master);
        int slave = open(name, O_RDWR);
        if (slave < 0)
            _exit(1);
        char byte = 'Z';
        if (write(slave, &byte, 1) != 1)
            _exit(1);
        close(slave);
        _exit(0);
    }
    close(parent_slave);
    struct pollfd waiting;
    waiting.fd = master;
    waiting.events = POLLIN;
    waiting.revents = 0;
    char byte = 0;
    int count = 0;
    int got = 0;
    for (int attempt = 0; attempt < 20 && !got; attempt++) {
        waiting.revents = 0;
        int ready = poll(&waiting, 1, 100);
        if (ready < 0)
            break;
        if (ready == 0)
            continue;
        if (ready == 0)
            continue;
        count = (int)read(master, &byte, 1);
        if (count == 1 && byte == 'Z') {
            got = 1;
        } else if (count < 0 && errno == EAGAIN) {
            continue;
        } else {
            break;
        }
    }
    printf("pty-winsize: close child-exit read=%d byte=%u\n", got ? 1 : count,
           (unsigned)byte);
    if (!got) {
        failures++;
        (void)raw_syscall2(0x4A, child, SIGKILL);
        close(master);
        return;
    }
    int eof = 0;
    for (int attempt = 0; attempt < 20 && !eof; attempt++) {
        waiting.revents = 0;
        int ready = poll(&waiting, 1, 100);
        if (ready < 0)
            break;
        if (ready == 0)
            continue;
        count = (int)read(master, &byte, 1);
        if (count == 0) {
            eof = 1;
        } else if (count < 0 && errno != EAGAIN) {
            break;
        }
    }
    printf("pty-winsize: close child-exit eof=%d\n", eof);
    if (!eof) {
        printf("pty-winsize: close child-exit FAIL eof=hang\n");
        failures++;
        (void)raw_syscall2(0x4A, child, SIGKILL);
    }
    close(master);
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

    /* K536: one SIGWINCH per actual size change, foreground group only. The parent makes itself
     * the group leader so tcsetpgrp's POSIX same-session check can use its own pid; the
     * foreground child inherits that group, and the background child leaves it. */
    if (setpgid(0, 0) != 0) {
        printf("pty-winsize: sigwinch setpgid FAIL errno=%d\n", errno);
        return 1;
    }
    if (tcsetpgrp(master, getpgrp()) != 0) {
        printf("pty-winsize: sigwinch set-fg FAIL errno=%d\n", errno);
        return 1;
    }
    int fg_rep = -1;
    pid_t fg_pid = spawn_winch_child(slave, master, 0, &fg_rep);
    int fg_pgrp = 0;
    int fg_child = 0;
    if (!child_ready(fg_pid, fg_rep, &fg_pgrp, &fg_child)) {
        printf("pty-winsize: sigwinch fg FAIL errno=%d\n", errno);
        return 1;
    }
    printf("pty-winsize: sigwinch fg pgrp=%d\n", fg_pgrp);

    struct winsize change1;
    memset(&change1, 0, sizeof(change1));
    change1.ws_row = 43;
    change1.ws_col = 120;
    if (tcsetwinsize(master, &change1) != 0) {
        printf("pty-winsize: sigwinch change-1-set FAIL errno=%d\n", errno);
        return 1;
    }
    struct winch_report r1;
    if (child_report(fg_rep, &r1, 2000) != 1) {
        printf("pty-winsize: sigwinch change-1 FAIL errno=%d\n", errno);
        return 1;
    }
    printf("pty-winsize: sigwinch change-1 count=%d row=%d col=%d\n", r1.count,
           r1.row, r1.col);
    if (r1.count != 1 || r1.row != 43 || r1.col != 120)
        failures++;
    drain_parent_signals();

    if (tcsetwinsize(slave, &change1) != 0) {
        printf("pty-winsize: sigwinch noop-set FAIL errno=%d\n", errno);
        return 1;
    }
    struct winch_report r2;
    int noop = child_report(fg_rep, &r2, 300);
    printf("pty-winsize: sigwinch noop count=%d\n", noop == 1 ? r2.count : r1.count);
    if (noop != 0)
        failures++;
    drain_parent_signals();

    struct winsize change2;
    memset(&change2, 0, sizeof(change2));
    change2.ws_row = 24;
    change2.ws_col = 80;
    change2.ws_xpixel = 640;
    change2.ws_ypixel = 480;
    /* The slave path must signal too; change-1 and change-3 use the master. */
    if (tcsetwinsize(slave, &change2) != 0) {
        printf("pty-winsize: sigwinch change-2-set FAIL errno=%d\n", errno);
        return 1;
    }
    struct winch_report r3;
    if (child_report(fg_rep, &r3, 2000) != 1) {
        printf("pty-winsize: sigwinch change-2 FAIL errno=%d\n", errno);
        return 1;
    }
    printf("pty-winsize: sigwinch change-2 count=%d row=%d col=%d\n", r3.count,
           r3.row, r3.col);
    if (r3.count != 2 || r3.row != 24 || r3.col != 80)
        failures++;
    drain_parent_signals();

    int bg_rep = -1;
    pid_t bg_pid = spawn_winch_child(slave, master, 1, &bg_rep);
    int bg_pgrp = 0;
    int bg_child = 0;
    if (!child_ready(bg_pid, bg_rep, &bg_pgrp, &bg_child)) {
        printf("pty-winsize: sigwinch bg FAIL errno=%d\n", errno);
        return 1;
    }
    printf("pty-winsize: sigwinch bg pgrp=%d\n", bg_pgrp);
    struct winsize change3;
    memset(&change3, 0, sizeof(change3));
    change3.ws_row = 30;
    change3.ws_col = 100;
    if (tcsetwinsize(master, &change3) != 0) {
        printf("pty-winsize: sigwinch change-3-set FAIL errno=%d\n", errno);
        return 1;
    }
    struct winch_report r4;
    if (child_report(fg_rep, &r4, 2000) != 1) {
        printf("pty-winsize: sigwinch change-3 FAIL errno=%d\n", errno);
        return 1;
    }
    printf("pty-winsize: sigwinch change-3 count=%d row=%d col=%d\n", r4.count,
           r4.row, r4.col);
    if (r4.count != 3 || r4.row != 30 || r4.col != 100)
        failures++;
    drain_parent_signals();
    struct winch_report rb;
    int bg = child_report(bg_rep, &rb, 300);
    printf("pty-winsize: sigwinch bg count=%d\n", bg == 1 ? rb.count : 0);
    if (bg != 0)
        failures++;
    child_stop(fg_child);
    child_stop(bg_child);
    int status = 0;
    if (waitpid(fg_pid, &status, 0) != fg_pid || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0)
        failures++;
    if (waitpid(bg_pid, &status, 0) != bg_pid || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0)
        failures++;
    close(fg_rep);
    close(bg_rep);

    /* No SIGWINCH after the pair closes, even with the parent as the foreground group. */
    drain_parent_signals();
    if (setpgid(0, 0) != 0)
        failures++;
    if (tcsetpgrp(master, getpgrp()) != 0)
        failures++;
    close(slave);
    close(master);
    struct winsize closed_size;
    memset(&closed_size, 0, sizeof(closed_size));
    closed_size.ws_row = 10;
    closed_size.ws_col = 20;
    errno = 0;
    int closed = tcsetwinsize(master, &closed_size);
    int closed_errno = errno;
    unsigned long info[3];
    memset(info, 0, sizeof(info));
    long queued = raw_syscall2(0x4B, (long)info, 1);
    printf("pty-winsize: sigwinch closed errno=%d signal=%ld\n", closed_errno,
           queued);
    if (closed != -1 || closed_errno != EBADF || queued != -EAGAIN)
        failures++;

    child_exit_eof();

    if (failures != 0) {
        printf("pty-winsize: FAIL fields=%d\n", failures);
        return 1;
    }
    printf("pty-winsize: ok\n");
    return 0;
}
