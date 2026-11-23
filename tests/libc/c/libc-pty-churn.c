/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K458: pty lifetime churn. Four concurrent processes each run 250 open/use/close cycles of
 * posix_openpt/grantpt/unlockpt/ptsname/open slave/tcsetattr/tcgetwinsize/close, after a
 * handshake that proves their slave paths are distinct while all four ptys are open. The parent
 * then holds 32 pairs at once, checks the paths are distinct, closes them, and proves the fd
 * numbers are reused. Any failure names the worker/cycle and the exact fd or path. The pty
 * server prints its own file/pair count on every transition to empty, so the qtest also proves
 * the server released every pair. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

#define WORKERS 4
/* K536 rev176: 4 workers x 250 = 1000 pty open/window/write/read/close cycles. The kernel's
 * s24 allocated/free frame line after each worker exit is the leak sample. */
#define CYCLES 250
#define CONCURRENT 32
#define NAME_MAX 64

static int fail(const char *what, int worker, int cycle, long got) {
    printf("pty-churn: FAIL worker=%d cycle=%d %s got=%ld errno=%d\n", worker, cycle,
           what, got, errno);
    return 1;
}

static int read_full(int fd, void *buffer, size_t length) {
    size_t total = 0;
    while (total < length) {
        ssize_t count = read(fd, (char *)buffer + total, length - total);
        if (count <= 0) {
            if (count < 0 && (errno == EINTR || errno == EAGAIN))
                continue;
            return -1;
        }
        total += (size_t)count;
    }
    return 0;
}

static int write_full(int fd, const void *buffer, size_t length) {
    size_t total = 0;
    while (total < length) {
        ssize_t count = write(fd, (const char *)buffer + total, length - total);
        if (count <= 0) {
            if (count < 0 && (errno == EINTR || errno == EAGAIN))
                continue;
            return -1;
        }
        total += (size_t)count;
    }
    return 0;
}

/* Open one pair and exercise the termios/winsize calls the SSH pty path uses. */
static int open_pair(int *master_out, int *slave_out, char *name, size_t name_len) {
    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0)
        return -1;
    if (grantpt(master) != 0 || unlockpt(master) != 0) {
        close(master);
        return -1;
    }
    char *path = ptsname(master);
    if (path == NULL || strlen(path) >= name_len) {
        close(master);
        return -1;
    }
    strcpy(name, path);
    int slave = open(name, O_RDWR);
    if (slave < 0) {
        close(master);
        return -1;
    }
    struct termios term;
    if (tcgetattr(slave, &term) != 0) {
        close(slave);
        close(master);
        return -1;
    }
    term.c_lflag &= (tcflag_t)~(ICANON | ECHO);
    term.c_iflag &= (tcflag_t)~(ICRNL | IXON);
    if (tcsetattr(slave, TCSANOW, &term) != 0) {
        close(slave);
        close(master);
        return -1;
    }
    struct winsize size;
    memset(&size, 0, sizeof(size));
    size.ws_row = 43;
    size.ws_col = 120;
    if (tcsetwinsize(slave, &size) != 0 || tcgetwinsize(slave, &size) != 0 ||
        size.ws_row != 43 || size.ws_col != 120) {
        close(slave);
        close(master);
        return -1;
    }
    /* Exercise both directions of the pty data path before closing the pair: the SSH session
     * writes commands to the slave and reads the master, and the shell writes the slave. */
    char byte = 'Z';
    char got = 0;
    struct pollfd pfd;
    pfd.fd = slave;
    pfd.events = POLLIN;
    pfd.revents = 0;
    if (write(master, &byte, 1) != 1 || poll(&pfd, 1, 1000) != 1 ||
        read(slave, &got, 1) != 1 || got != byte) {
        close(slave);
        close(master);
        return -1;
    }
    pfd.fd = master;
    pfd.revents = 0;
    if (write(slave, &byte, 1) != 1 || poll(&pfd, 1, 1000) != 1 ||
        read(master, &got, 1) != 1 || got != byte) {
        close(slave);
        close(master);
        return -1;
    }
    *master_out = master;
    *slave_out = slave;
    return 0;
}

static int worker(int index, int ready_fd, int go_fd) {
    int master = -1;
    int slave = -1;
    char first_name[NAME_MAX];
    memset(first_name, 0, sizeof(first_name));
    if (open_pair(&master, &slave, first_name, sizeof(first_name)) != 0) {
        uint32_t zero = 0;
        (void)write_full(ready_fd, &zero, sizeof(zero));
        return fail("handshake-open", index, 0, -1);
    }
    uint32_t length = (uint32_t)strlen(first_name);
    char message[sizeof(uint32_t) + NAME_MAX];
    memcpy(message, &length, sizeof(length));
    memcpy(message + sizeof(length), first_name, length);
    if (write_full(ready_fd, message, sizeof(length) + length) != 0)
        return fail("handshake-write", index, 0, -1);
    char release = 0;
    if (read_full(go_fd, &release, 1) != 0)
        return fail("handshake-release", index, 0, -1);
    close(slave);
    close(master);
    close(ready_fd);
    close(go_fd);

    int first_master = -1;
    int first_slave = -1;
    for (int cycle = 0; cycle < CYCLES; cycle++) {
        char name[NAME_MAX];
        memset(name, 0, sizeof(name));
        if (open_pair(&master, &slave, name, sizeof(name)) != 0)
            return fail("open", index, cycle, -1);
        if (cycle == 0) {
            first_master = master;
            first_slave = slave;
        } else {
            if (master != first_master)
                return fail("master-fd-reuse", index, cycle, master);
            if (slave != first_slave)
                return fail("slave-fd-reuse", index, cycle, slave);
        }
        if (strlen(name) < 9 || strncmp(name, "/dev/pts/", 9) != 0)
            return fail("slave-name", index, cycle, -1);
        close(slave);
        close(master);
    }
    printf("pty-churn: worker=%d cycles=%d first-master=%d first-slave=%d ok\n", index,
           CYCLES, first_master, first_slave);
    fflush(stdout);
    return 0;
}

int main(void) {
    printf("pty-churn: start uid=%d workers=%d cycles=%d concurrent=%d\n", (int)getuid(),
           WORKERS, CYCLES, CONCURRENT);
    fflush(stdout);

    int ready[2];
    int go[2];
    if (pipe(ready) != 0 || pipe(go) != 0) {
        printf("pty-churn: FAIL pipe errno=%d\n", errno);
        return 1;
    }

    pid_t pids[WORKERS];
    for (int index = 0; index < WORKERS; index++) {
        pid_t pid = fork();
        if (pid < 0) {
            printf("pty-churn: FAIL fork errno=%d\n", errno);
            return 1;
        }
        if (pid == 0) {
            close(ready[0]);
            close(go[1]);
            int result = worker(index, ready[1], go[0]);
            fflush(stdout);
            _exit(result);
        }
        pids[index] = pid;
    }
    close(ready[1]);
    close(go[0]);

    char paths[WORKERS][NAME_MAX];
    memset(paths, 0, sizeof(paths));
    for (int index = 0; index < WORKERS; index++) {
        uint32_t length = 0;
        if (read_full(ready[0], &length, sizeof(length)) != 0) {
            printf("pty-churn: FAIL handshake-read index=%d errno=%d\n", index, errno);
            return 1;
        }
        if (length == 0 || length >= NAME_MAX) {
            printf("pty-churn: FAIL handshake-length index=%d length=%u\n", index,
                   (unsigned)length);
            return 1;
        }
        if (read_full(ready[0], paths[index], length) != 0) {
            printf("pty-churn: FAIL handshake-name index=%d errno=%d\n", index, errno);
            return 1;
        }
        paths[index][length] = 0;
        printf("pty-churn: handshake index=%d path=%s\n", index, paths[index]);
    }
    for (int left = 0; left < WORKERS; left++) {
        for (int right = left + 1; right < WORKERS; right++) {
            if (strcmp(paths[left], paths[right]) == 0) {
                printf("pty-churn: FAIL duplicate-path=%s left=%d right=%d\n",
                       paths[left], left, right);
                return 1;
            }
        }
    }
    printf("pty-churn: handshake distinct=%d\n", WORKERS);

    char release[WORKERS];
    memset(release, 0, sizeof(release));
    if (write_full(go[1], release, sizeof(release)) != 0) {
        printf("pty-churn: FAIL handshake-go errno=%d\n", errno);
        return 1;
    }
    close(ready[0]);
    close(go[1]);

    for (int index = 0; index < WORKERS; index++) {
        int status = 0;
        if (waitpid(pids[index], &status, 0) != pids[index]) {
            printf("pty-churn: FAIL waitpid index=%d errno=%d\n", index, errno);
            return 1;
        }
        if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            printf("pty-churn: FAIL worker-status index=%d status=%d\n", index, status);
            return 1;
        }
    }
    printf("pty-churn: workers=4 status=ok\n");

    int masters[CONCURRENT];
    int slaves[CONCURRENT];
    char names[CONCURRENT][NAME_MAX];
    memset(names, 0, sizeof(names));
    for (int index = 0; index < CONCURRENT; index++) {
        if (open_pair(&masters[index], &slaves[index], names[index],
                      sizeof(names[index])) != 0) {
            printf("pty-churn: FAIL concurrent-open index=%d errno=%d\n", index, errno);
            return 1;
        }
    }
    for (int left = 0; left < CONCURRENT; left++) {
        for (int right = left + 1; right < CONCURRENT; right++) {
            if (strcmp(names[left], names[right]) == 0) {
                printf("pty-churn: FAIL concurrent-duplicate=%s left=%d right=%d\n",
                       names[left], left, right);
                return 1;
            }
        }
    }
    printf("pty-churn: concurrent=%d distinct=ok\n", CONCURRENT);
    for (int index = 0; index < CONCURRENT; index++) {
        close(slaves[index]);
        close(masters[index]);
    }

    int first_master = -1;
    int first_slave = -1;
    char probe[NAME_MAX];
    memset(probe, 0, sizeof(probe));
    if (open_pair(&first_master, &first_slave, probe, sizeof(probe)) != 0) {
        printf("pty-churn: FAIL final-open errno=%d\n", errno);
        return 1;
    }
    close(first_slave);
    close(first_master);

    int reused_master = -1;
    int reused_slave = -1;
    if (open_pair(&reused_master, &reused_slave, probe, sizeof(probe)) != 0) {
        printf("pty-churn: FAIL reuse-open errno=%d\n", errno);
        return 1;
    }
    if (reused_master != first_master || reused_slave != first_slave) {
        printf("pty-churn: FAIL fd-reuse master=%d->%d slave=%d->%d\n", first_master,
               reused_master, first_slave, reused_slave);
        return 1;
    }
    printf("pty-churn: fd-reuse master=%d slave=%d ok\n", reused_master, reused_slave);
    close(reused_slave);
    close(reused_master);

    printf("pty-churn: pass cycles=%d concurrent=%d\n", WORKERS * CYCLES, CONCURRENT);
    return 0;
}
