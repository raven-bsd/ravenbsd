/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K489: a slave read blocked with VMIN=1 must not stall the pty server for another pair.
 *
 * Pair A: a child blocks in read(slave A) with no data.
 * Pair B: the parent writes one byte to master B and reads it from slave B; the wall time is
 * measured. On the single-threaded ptyd this stalls until the alarm fires.
 * Eight pairs: eight children block, the parent idles 5 s, then writes one byte to every master;
 * each child reports its read wall time.
 * A ninth pair is allocated and closed after the blocked set; ptyd must return to files=0 pairs=0.
 * A 3 s SIGALRM bounds the first red measurement so the case reports a wall instead of hanging. */

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#define BLOCKED 8

static int failures;

static long now_ms(void) {
    struct timespec ts;
    memset(&ts, 0, sizeof(ts));
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
        return -1;
    return (long)ts.tv_sec * 1000L + (long)(ts.tv_nsec / 1000000L);
}

static void alarm_handler(int signum) {
    (void)signum;
    const char message[] = "pty-concurrent: timeout blocked-wall=3000ms\n";
    (void)write(2, message, sizeof(message) - 1);
    _exit(2);
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

static int set_blocking_read(int slave) {
    struct termios term;
    memset(&term, 0, sizeof(term));
    term.c_iflag = 0;
    term.c_oflag = 0;
    term.c_cflag = CS8 | CREAD | B38400;
    term.c_lflag = 0;
    term.c_cc[VMIN] = 1;
    term.c_cc[VTIME] = 0;
    return tcsetattr(slave, TCSANOW, &term);
}

static int main_case(void) {
    int a_master = -1;
    int a_slave = -1;
    int b_master = -1;
    int b_slave = -1;
    char name[64];

    if (open_pair(&a_master, &a_slave, name, sizeof(name)) != 0 ||
        set_blocking_read(a_slave) != 0)
        return 1;
    pid_t a_child = fork();
    if (a_child < 0)
        return 1;
    if (a_child == 0) {
        close(a_master);
        unsigned char byte = 0;
        long start = now_ms();
        int count = (int)read(a_slave, &byte, 1);
        printf("pty-concurrent: A read=%d byte=%u wall_ms=%ld\n", count, (unsigned)byte,
               now_ms() - start);
        fflush(stdout);
        _exit(count == 1 && byte == 'A' ? 0 : 1);
    }
    close(a_slave);

    if (open_pair(&b_master, &b_slave, name, sizeof(name)) != 0 ||
        set_blocking_read(b_slave) != 0)
        return 1;
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = alarm_handler;
    (void)sigaction(SIGALRM, &action, NULL);
    (void)alarm(3);
    long start = now_ms();
    int wrote = (int)write(b_master, "B", 1);
    unsigned char byte = 0;
    int read_back = wrote == 1 ? (int)read(b_slave, &byte, 1) : -1;
    long wall = now_ms() - start;
    (void)alarm(0);
    printf("pty-concurrent: B write=%d read=%d byte=%u wall_ms=%ld\n", wrote, read_back,
           (unsigned)byte, wall);
    fflush(stdout);
    if (wrote != 1 || read_back != 1 || byte != 'B' || wall > 500) {
        printf("pty-concurrent: FAIL B wall_ms=%ld\n", wall);
        failures++;
    }
    if (write(a_master, "A", 1) != 1) {
        failures++;
    }
    int status = 0;
    if (waitpid(a_child, &status, 0) != a_child || !WIFEXITED(status) || WEXITSTATUS(status) != 0)
        failures++;
    close(b_slave);
    close(b_master);
    close(a_master);
    printf("pty-concurrent: pair A/B done\n");
    return 0;
}

static int many_case(void) {
    long *shared_start = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                              MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    if (shared_start == MAP_FAILED) {
        printf("pty-concurrent: FAIL mmap errno=%d\n", errno);
        return 1;
    }
    *shared_start = 0;
    int masters[BLOCKED];
    int slaves[BLOCKED];
    pid_t children[BLOCKED];
    char names[BLOCKED][64];
    memset(masters, -1, sizeof(masters));
    memset(slaves, -1, sizeof(slaves));
    for (int index = 0; index < BLOCKED; index++) {
        if (open_pair(&masters[index], &slaves[index], names[index], sizeof(names[index])) != 0 ||
            set_blocking_read(slaves[index]) != 0) {
            printf("pty-concurrent: FAIL open pair=%d errno=%d\n", index, errno);
            return 1;
        }
    }
    for (int index = 0; index < BLOCKED; index++) {
        pid_t child = fork();
        if (child < 0) {
            printf("pty-concurrent: FAIL fork pair=%d errno=%d\n", index, errno);
            return 1;
        }
        if (child == 0) {
            for (int other = 0; other < BLOCKED; other++) {
                if (other != index) {
                    close(masters[other]);
                    close(slaves[other]);
                }
            }
            close(masters[index]);
            long start = now_ms();
            unsigned char byte = 0;
            int count = (int)read(slaves[index], &byte, 1);
            long wall = now_ms() - start;
            long latency = (*shared_start != 0) ? now_ms() - *shared_start : -1;
            printf("pty-concurrent: pair=%d read=%d byte=%u wall_ms=%ld latency_ms=%ld\n",
                   index, count, (unsigned)byte, wall, latency);
            fflush(stdout);
            _exit(count == 1 && byte == (unsigned char)('a' + index) && latency >= 0 &&
                          latency <= 500
                      ? 0
                      : 1);
        }
        children[index] = child;
    }

    struct timespec idle;
    idle.tv_sec = 5;
    idle.tv_nsec = 0;
    (void)nanosleep(&idle, NULL);
    printf("pty-concurrent: idle_ms=5000 blocked=%d\n", BLOCKED);
    fflush(stdout);

    *shared_start = now_ms();
    for (int index = 0; index < BLOCKED; index++) {
        char byte = (char)('a' + index);
        if (write(masters[index], &byte, 1) != 1) {
            printf("pty-concurrent: FAIL write pair=%d errno=%d\n", index, errno);
            failures++;
        }
    }
    for (int index = 0; index < BLOCKED; index++) {
        int status = 0;
        if (waitpid(children[index], &status, 0) != children[index] || !WIFEXITED(status) ||
            WEXITSTATUS(status) != 0) {
            printf("pty-concurrent: FAIL pair=%d status=%d\n", index, status);
            failures++;
        }
        close(slaves[index]);
        close(masters[index]);
    }

    int ninth_master = -1;
    int ninth_slave = -1;
    char ninth_name[64];
    if (open_pair(&ninth_master, &ninth_slave, ninth_name, sizeof(ninth_name)) != 0) {
        printf("pty-concurrent: FAIL ninth errno=%d\n", errno);
        failures++;
    } else {
        close(ninth_slave);
        close(ninth_master);
        printf("pty-concurrent: ninth pair ok\n");
    }
    return 0;
}

int main(void) {
    printf("pty-concurrent: start uid=%d blocked=%d\n", (int)getuid(), BLOCKED);
    fflush(stdout);
    (void)main_case();
    (void)many_case();
    if (failures != 0) {
        printf("pty-concurrent: FAIL rows=%d\n", failures);
        return 1;
    }
    printf("pty-concurrent: ok\n");
    return 0;
}
