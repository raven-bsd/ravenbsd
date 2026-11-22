/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K473: the exact server-side pty sequence sshd runs, without sshd or the network.
 *
 * Step 1-5 are the libutil openpty order one call at a time: posix_openpt, grantpt, unlockpt,
 * ptsname, open(slave). Steps 6-7 apply the K444 pty-req termios record and the K455 window size.
 * Step 8 fork+execs this same binary on the slave (stdin/stdout/stderr) and the child prints
 * ttyname(0) and tcgetwinsize(0). Step 9 reads that through the master and checks /dev/pts/N and
 * 43x120. Step 10 writes an input byte, checks the ECHO on the master and the child's read of the
 * completed line. Step 11 waits, closes, and the qtest asserts ptyd files=0 pairs=0.
 *
 * Every step prints `pty-ssh-shape: step=N call=... rv=...`, so a failure names the syscall. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

static int fail(const char *what, long rv) {
    printf("pty-ssh-shape: FAIL call=%s rv=%ld errno=%d\n", what, rv, errno);
    fflush(stdout);
    return 1;
}

static int read_until(int fd, char *buffer, size_t capacity, const char *needle) {
    size_t total = 0;
    buffer[0] = 0;
    for (int spins = 0; spins < 200; spins++) {
        struct pollfd ready;
        ready.fd = fd;
        ready.events = POLLIN;
        ready.revents = 0;
        int polled = poll(&ready, 1, 100);
        if (polled < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (polled == 0)
            continue;
        ssize_t count = read(fd, buffer + total, capacity - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            buffer[total] = 0;
            if (strstr(buffer, needle) != NULL)
                return (int)total;
            if (total + 1 >= capacity)
                return -1;
            continue;
        }
        if (count == 0)
            return -1;
        if (errno != EAGAIN && errno != EINTR)
            return -1;
    }
    return -1;
}

static int read_append_until(int fd, char *buffer, size_t capacity, const char *needle) {
    size_t total = strlen(buffer);
    for (int spins = 0; spins < 200; spins++) {
        struct pollfd ready;
        ready.fd = fd;
        ready.events = POLLIN;
        ready.revents = 0;
        int polled = poll(&ready, 1, 100);
        if (polled < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (polled == 0)
            continue;
        ssize_t count = read(fd, buffer + total, capacity - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            buffer[total] = 0;
            if (strstr(buffer, needle) != NULL)
                return (int)total;
            if (total + 1 >= capacity)
                return -1;
            continue;
        }
        if (count == 0)
            return -1;
        if (errno != EAGAIN && errno != EINTR)
            return -1;
    }
    return -1;
}

static int child_main(void) {
    char *name = ttyname(0);
    struct winsize winsize;
    memset(&winsize, 0, sizeof(winsize));
    if (name == NULL)
        return fail("ttyname", -1);
    if (tcgetwinsize(0, &winsize) != 0)
        return fail("tcgetwinsize", -1);
    printf("pty-ssh-shape: child tty=%s rows=%u cols=%u\n", name,
           (unsigned)winsize.ws_row, (unsigned)winsize.ws_col);
    fflush(stdout);

    char input[64];
    ssize_t count;
    for (;;) {
        count = read(0, input, sizeof(input));
        if (count >= 0 || (errno != EAGAIN && errno != EINTR))
            break;
        struct pollfd ready;
        ready.fd = 0;
        ready.events = POLLIN;
        ready.revents = 0;
        (void)poll(&ready, 1, 100);
    }
    printf("pty-ssh-shape: child read=%d\n", (int)count);
    if (count > 0)
        (void)write(1, input, (size_t)count);
    printf("pty-ssh-shape: child done\n");
    fflush(stdout);
    return count > 0 ? 0 : 1;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "pty-ssh-shape-child") == 0)
        return child_main();

    printf("pty-ssh-shape: start uid=%d\n", (int)getuid());
    fflush(stdout);

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    printf("pty-ssh-shape: step=1 call=posix_openpt rv=%d errno=%d\n", master, errno);
    fflush(stdout);
    if (master < 0)
        return 1;

    int granted = grantpt(master);
    printf("pty-ssh-shape: step=2 call=grantpt rv=%d errno=%d\n", granted, errno);
    fflush(stdout);
    if (granted != 0)
        return 1;

    int unlocked = unlockpt(master);
    printf("pty-ssh-shape: step=3 call=unlockpt rv=%d errno=%d\n", unlocked, errno);
    fflush(stdout);
    if (unlocked != 0)
        return 1;

    char *path = ptsname(master);
    printf("pty-ssh-shape: step=4 call=ptsname rv=%d path=%s errno=%d\n",
           path == NULL ? -1 : 0, path == NULL ? "(null)" : path, errno);
    fflush(stdout);
    if (path == NULL)
        return 1;

    int slave = open(path, O_RDWR);
    printf("pty-ssh-shape: step=5 call=open_slave rv=%d path=%s errno=%d\n", slave, path,
           errno);
    fflush(stdout);
    if (slave < 0)
        return 1;

    struct termios term;
    memset(&term, 0, sizeof(term));
    term.c_iflag = IGNBRK | ICRNL | IXON;
    term.c_oflag = OPOST | ONLCR;
    term.c_cflag = CS8 | CREAD | B9600;
    term.c_lflag = ISIG | ICANON | ECHO | IEXTEN;
    term.c_cc[VINTR] = 3;
    term.c_cc[VQUIT] = 28;
    term.c_cc[VERASE] = 127;
    term.c_cc[VKILL] = 21;
    term.c_cc[VEOF] = 4;
    term.c_cc[VMIN] = 1;
    term.c_cc[VTIME] = 0;
    term.c_cc[VSTART] = 17;
    term.c_cc[VSTOP] = 19;
    term.c_cc[VSUSP] = 26;
    cfsetispeed(&term, B38400);
    cfsetospeed(&term, B38400);
    int set = tcsetattr(slave, TCSANOW, &term);
    printf("pty-ssh-shape: step=6 call=tcsetattr rv=%d errno=%d\n", set, errno);
    fflush(stdout);
    if (set != 0)
        return 1;

    struct winsize size;
    memset(&size, 0, sizeof(size));
    size.ws_row = 43;
    size.ws_col = 120;
    int sized = tcsetwinsize(slave, &size);
    printf("pty-ssh-shape: step=7 call=tcsetwinsize rv=%d rows=43 cols=120 errno=%d\n", sized,
           errno);
    fflush(stdout);
    if (sized != 0)
        return 1;

    // A forked session child is this binary's own equivalent of sshd's exec: it inherits the
    // opened slave as stdin/stdout/stderr and runs the child path below. The libc-test initfs
    // does not stage a separate `/bin/tty`; the pty sequence under test is the inheritance.
    pid_t child = fork();
    if (child == 0) {
        (void)dup2(slave, 0);
        (void)dup2(slave, 1);
        (void)dup2(slave, 2);
        close(master);
        char *child_argv[] = { "libc-test", "pty-ssh-shape-child", NULL };
        char *child_env[] = { NULL };
        (void)execve("/bin/libc-test", child_argv, child_env);
        _exit(127);
    }
    printf("pty-ssh-shape: step=8 call=fork_exec rv=%d pid=%d errno=%d\n",
           child < 0 ? -1 : 0, (int)child, errno);
    fflush(stdout);
    if (child < 0)
        return 1;

    (void)fcntl(master, F_SETFL, O_NONBLOCK);

    char output[2048];
    if (read_until(master, output, sizeof(output), "child tty=") < 0)
        return fail("read_child_tty", -1);
    printf("pty-ssh-shape: step=9 call=read_master rv=%d\n", (int)strlen(output));
    fflush(stdout);
    char child_tty[64];
    unsigned child_rows = 0;
    unsigned child_cols = 0;
    memset(child_tty, 0, sizeof(child_tty));
    char *child_line = strstr(output, "child tty=");
    if (child_line != NULL)
        (void)sscanf(child_line, "child tty=%63s rows=%u cols=%u", child_tty, &child_rows,
                     &child_cols);
    printf("pty-ssh-shape: step=9 call=child_shape tty=%s rows=%u cols=%u\n", child_tty,
           child_rows, child_cols);
    fflush(stdout);
    if (strncmp(child_tty, "/dev/pts/", 9) != 0 || child_rows != 43 || child_cols != 120) {
        printf("pty-ssh-shape: FAIL child-shape\n");
        return 1;
    }

    struct winsize master_size;
    memset(&master_size, 0, sizeof(master_size));
    int master_sized = tcgetwinsize(master, &master_size);
    printf("pty-ssh-shape: step=9 call=tcgetwinsize_master rv=%d rows=%u cols=%u\n",
           master_sized, (unsigned)master_size.ws_row, (unsigned)master_size.ws_col);
    fflush(stdout);
    if (master_sized != 0 || master_size.ws_row != 43 || master_size.ws_col != 120)
        return 1;

    if (write(master, "Z", 1) != 1)
        return fail("write_echo", -1);
    if (read_until(master, output, sizeof(output), "Z") < 0)
        return fail("read_echo", -1);
    printf("pty-ssh-shape: step=10 call=read_echo rv=%d\n", (int)strlen(output));
    fflush(stdout);

    if (write(master, "\n", 1) != 1)
        return fail("write_newline", -1);
    if (read_append_until(master, output, sizeof(output), "child done") < 0)
        return fail("read_child_done", -1);
    printf("pty-ssh-shape: step=10 call=read_line rv=%d\n", (int)strlen(output));
    fflush(stdout);
    if (strstr(output, "child read=2") == NULL) {
        printf("pty-ssh-shape: FAIL child-read\n");
        return 1;
    }
    printf("pty-ssh-shape: step=10 call=child_read rv=2\n");
    printf("pty-ssh-shape: step=10 call=child_done rv=0\n");
    fflush(stdout);

    int status = -1;
    pid_t waited = waitpid(child, &status, 0);
    printf("pty-ssh-shape: step=11 call=waitpid rv=%d status=%d\n", (int)waited, status);
    fflush(stdout);
    if (waited != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return 1;

    int closed_slave = close(slave);
    int closed_master = close(master);
    printf("pty-ssh-shape: step=12 call=close rv=%d/%d\n", closed_slave, closed_master);
    fflush(stdout);
    if (closed_slave != 0 || closed_master != 0)
        return 1;

    printf("pty-ssh-shape: ok\n");
    return 0;
}
