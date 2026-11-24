/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K513: a blocked slave read on one pair must not delay another pair or the pty server.
 *
 * Pair A blocks in read(VMIN=1); pair B writes and reads one byte with a wall bound.
 * Eight pairs block for a 5 s idle window; each then receives its byte. The 16-pair phase uses a
 * 1 s idle and a stated 1000 ms bound. The per-pair latencies are printed; each child enforces
 * its phase bound. A ninth pair allocates and closes; ptyd returns to files=0 pairs=0.
 * The case runs under KVM because a bound over eight guest processes measures the TCG scheduler
 * as much as ptyd (K489); the server completion is what this ticket bounds. */

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#define MAX_PAIRS 16
#define PAIRS8 8
#define PAIRS16 16
#define BOUND8_MS 500
#define BOUND16_MS 1000

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

/* A caught SIGCHLD is not SA_RESTART: POSIX lets waitpid return -1/EINTR (FreeBSD
 * `kern_wait6` returns the `msleep(... PCATCH)` error). The child status stays queued,
 * so retry the same wait. */
static pid_t wait_child(pid_t child, int *status) {
    pid_t waited;
    while ((waited = waitpid(child, status, 0)) < 0 && errno == EINTR)
        ;
    return waited;
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
    if (wrote != 1 || read_back != 1 || byte != 'B' || wall > BOUND8_MS) {
        printf("pty-concurrent: FAIL B wall_ms=%ld\n", wall);
        failures++;
    }
    if (write(a_master, "A", 1) != 1) {
        failures++;
    }
    int status = 0;
    if (wait_child(a_child, &status) != a_child || !WIFEXITED(status) || WEXITSTATUS(status) != 0)
        failures++;
    close(b_slave);
    close(b_master);
    close(a_master);
    printf("pty-concurrent: pair A/B done\n");
    return 0;
}

/* Open `pairs` blocked reads, idle, write one byte to every master, release every reader and
 * enforce `bound_ms` from release to read completion. The per-pair lines are the evidence. */
static int run_phase(int pairs, int bound_ms, int idle_secs) {
    int masters[MAX_PAIRS];
    int slaves[MAX_PAIRS];
    int go[MAX_PAIRS][2];
    int ready[MAX_PAIRS][2];
    pid_t children[MAX_PAIRS];
    char names[MAX_PAIRS][64];
    int phase_failures = 0;

    if (pairs < 1 || pairs > MAX_PAIRS)
        return 1;
    memset(masters, -1, sizeof(masters));
    memset(slaves, -1, sizeof(slaves));
    memset(go, -1, sizeof(go));
    memset(ready, -1, sizeof(ready));
    memset(children, 0, sizeof(children));

    for (int index = 0; index < pairs; index++) {
        if (open_pair(&masters[index], &slaves[index], names[index], sizeof(names[index])) != 0 ||
            set_blocking_read(slaves[index]) != 0) {
            printf("pty-concurrent: FAIL open pair=%d errno=%d\n", index, errno);
            failures++;
            return 1;
        }
        if (pipe(go[index]) != 0 || pipe(ready[index]) != 0) {
            printf("pty-concurrent: FAIL pipe pair=%d errno=%d\n", index, errno);
            failures++;
            return 1;
        }
    }

    for (int index = 0; index < pairs; index++) {
        pid_t child = fork();
        if (child < 0) {
            printf("pty-concurrent: FAIL fork pair=%d errno=%d\n", index, errno);
            failures++;
            return 1;
        }
        if (child == 0) {
            for (int other = 0; other < pairs; other++) {
                if (other != index) {
                    close(go[other][0]);
                    close(go[other][1]);
                    close(ready[other][0]);
                    close(ready[other][1]);
                    close(masters[other]);
                    close(slaves[other]);
                }
            }
            close(masters[index]);
            close(go[index][1]);
            close(ready[index][0]);
            char ready_byte = 1;
            (void)write(ready[index][1], &ready_byte, 1);
            close(ready[index][1]);
            // The parent sends the phase start timestamp just before it writes the masters;
            // the measured latency therefore covers the server write/completion path, not the
            // idle and not only the child's post-release scheduling.
            long start = 0;
            (void)read(go[index][0], &start, sizeof(start));
            close(go[index][0]);
            unsigned char byte = 0;
            int count = (int)read(slaves[index], &byte, 1);
            long latency = now_ms() - start;
            printf("pty-concurrent: pairs=%d pair=%d read=%d byte=%u latency_ms=%ld\n", pairs,
                   index, count, (unsigned)byte, latency);
            fflush(stdout);
            _exit(count == 1 && byte == (unsigned char)('a' + index) && latency <= bound_ms ? 0
                                                                                        : 1);
        }
        children[index] = child;
    }

    for (int index = 0; index < pairs; index++)
        close(go[index][0]);
    for (int index = 0; index < pairs; index++) {
        char ready_byte = 0;
        (void)read(ready[index][0], &ready_byte, 1);
        close(ready[index][0]);
        close(ready[index][1]);
    }

    struct timespec idle;
    idle.tv_sec = idle_secs;
    idle.tv_nsec = 0;
    (void)nanosleep(&idle, NULL);
    printf("pty-concurrent: idle_ms=%d pairs=%d\n", idle_secs * 1000, pairs);
    fflush(stdout);

    long start_ms = now_ms();
    for (int index = 0; index < pairs; index++) {
        (void)write(go[index][1], &start_ms, sizeof(start_ms));
        close(go[index][1]);
    }
    for (int index = 0; index < pairs; index++) {
        char byte = (char)('a' + index);
        if (write(masters[index], &byte, 1) != 1) {
            printf("pty-concurrent: FAIL write pair=%d errno=%d\n", index, errno);
            phase_failures++;
        }
    }
    for (int index = 0; index < pairs; index++) {
        int status = 0;
        if (wait_child(children[index], &status) != children[index] || !WIFEXITED(status) ||
            WEXITSTATUS(status) != 0) {
            printf("pty-concurrent: FAIL pair=%d status=%d\n", index, status);
            phase_failures++;
        }
        close(slaves[index]);
        close(masters[index]);
    }

    failures += phase_failures;
    printf("pty-concurrent: pairs=%d bound_ms=%d status=%s\n", pairs, bound_ms,
           phase_failures == 0 ? "ok" : "fail");
    return phase_failures == 0 ? 0 : 1;
}

static void ninth_pair(void) {
    int master = -1;
    int slave = -1;
    char name[64];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        printf("pty-concurrent: FAIL ninth errno=%d\n", errno);
        failures++;
        return;
    }
    close(slave);
    close(master);
    printf("pty-concurrent: ninth pair ok\n");
}

int main(void) {
    printf("pty-concurrent: start uid=%d blocked=%d\n", (int)getuid(), PAIRS8);
    fflush(stdout);
    (void)main_case();
    (void)run_phase(PAIRS8, BOUND8_MS, 5);
    (void)run_phase(PAIRS16, BOUND16_MS, 1);
    ninth_pair();
    if (failures != 0) {
        printf("pty-concurrent: FAIL rows=%d\n", failures);
        return 1;
    }
    printf("pty-concurrent: ok\n");
    return 0;
}
