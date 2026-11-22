/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K489: ptyd concurrency. Pair A blocks in a slave read (VMIN=1, no data); pair B's master
 * write must complete its slave round-trip while A is still blocked. The parent bounds the red
 * run with a 500 ms poll. The test then blocks eight pairs at once and writes each master; every
 * slave must receive its byte within 500 ms. A ninth pair allocates and closes while the eight
 * are live. VMIN=0/VTIME=1 must still return 0 after ~100 ms. The ptyd table must return to
 * files=0 pairs=0; a 5 s idle window and a post-idle probe give the ptyd CPU samples. */

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <poll.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>
#include <sys/wait.h>

#define BOUND_MS 500
#define PAIRS 8
#define NAME_MAX 64
#define VTIME_TENTHS 0 /* true blocking VMIN=1; the parent bounds the red run with poll */
#define READ_ARM_MS 100

struct result {
    int index;
    int count;
    int wall_ms;
    unsigned char byte;
};

static long now_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
        return -1;
    return (long)ts.tv_sec * 1000 + (long)ts.tv_nsec / 1000000;
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

static int wait_result(int fd, struct result *result, int timeout_ms) {
    struct pollfd pfd;
    pfd.fd = fd;
    pfd.events = POLLIN;
    pfd.revents = 0;
    int ready = poll(&pfd, 1, timeout_ms);
    if (ready <= 0)
        return -1;
    return read_full(fd, result, sizeof(*result));
}

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
    int slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        close(master);
        return -1;
    }
    *master_out = master;
    *slave_out = slave;
    return 0;
}

static int set_read_mode(int slave, int vmin, int vtime) {
    struct termios term;
    if (tcgetattr(slave, &term) != 0)
        return -1;
    term.c_lflag &= (tcflag_t)~(ECHO | ICANON);
    term.c_cc[VMIN] = (cc_t)vmin;
    term.c_cc[VTIME] = (cc_t)vtime;
    return tcsetattr(slave, TCSANOW, &term);
}

/* Child: set the slave read mode, signal readiness, block in read, report the observed byte. */
static void reader_child(int slave, int master, int ready_fd, int result_fd, int index) {
    close(master);
    if (set_read_mode(slave, 1, VTIME_TENTHS) != 0) {
        struct result r = { index, -1, -1, 0 };
        (void)write_full(result_fd, &r, sizeof(r));
        _exit(1);
    }
    char ready = 'R';
    if (write_full(ready_fd, &ready, 1) != 0)
        _exit(1);
    long start = now_ms();
    unsigned char byte = 0;
    ssize_t count = read(slave, &byte, 1);
    long wall = now_ms() - start;
    struct result r = { index, (int)count, (int)wall, byte };
    (void)write_full(result_fd, &r, sizeof(r));
    close(slave);
    _exit(0);
}

static int pair_ab(void) {
    int am = -1, as = -1, bm = -1, bs = -1;
    char name[NAME_MAX];
    if (open_pair(&am, &as, name, sizeof(name)) != 0 ||
        open_pair(&bm, &bs, name, sizeof(name)) != 0) {
        printf("pty-concurrent: FAIL pair-ab open errno=%d\n", errno);
        return 1;
    }
    int aready[2], aresult[2], bready[2], bresult[2];
    if (pipe(aready) != 0 || pipe(aresult) != 0 || pipe(bready) != 0 || pipe(bresult) != 0) {
        printf("pty-concurrent: FAIL pair-ab pipe errno=%d\n", errno);
        return 1;
    }
    pid_t a = fork();
    if (a == 0) {
        close(aready[0]);
        close(aresult[0]);
        reader_child(as, am, aready[1], aresult[1], 0);
    }
    pid_t b = fork();
    if (b == 0) {
        close(bready[0]);
        close(bresult[0]);
        reader_child(bs, bm, bready[1], bresult[1], 1);
    }
    close(as);
    close(bs);
    close(aready[1]);
    close(aresult[1]);
    close(bready[1]);
    close(bresult[1]);

    char ready = 0;
    if (read_full(aready[0], &ready, 1) != 0 || read_full(bready[0], &ready, 1) != 0) {
        printf("pty-concurrent: FAIL pair-ab ready errno=%d\n", errno);
        return 1;
    }
    struct timespec pause = { 0, READ_ARM_MS * 1000000L };
    (void)nanosleep(&pause, NULL);
    long start = now_ms();
    unsigned char b_byte = 'B';
    if (write_full(bm, &b_byte, 1) != 0) {
        printf("pty-concurrent: FAIL pair-b master-write errno=%d\n", errno);
        return 1;
    }
    struct result br;
    int got = wait_result(bresult[0], &br, BOUND_MS);
    long wall = now_ms() - start;
    int status = 0;
    if (got != 0) {
        printf("pty-concurrent: FAIL pair-b wall-ms=%ld bound=%d result=%d\n", wall, BOUND_MS,
               got);
        return 1;
    }
    unsigned char a_byte = 'A';
    if (write_full(am, &a_byte, 1) != 0) {
        printf("pty-concurrent: FAIL pair-a unblock errno=%d\n", errno);
        return 1;
    }
    struct result ar;
    if (read_full(aresult[0], &ar, sizeof(ar)) != 0 ||
        waitpid(a, &status, 0) != a ||
        waitpid(b, &status, 0) != b || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("pty-concurrent: FAIL pair-ab child-status errno=%d\n", errno);
        return 1;
    }
    close(am);
    close(bm);
    close(aready[0]);
    close(aresult[0]);
    close(bready[0]);
    close(bresult[0]);
    if (br.count != 1 || br.byte != 'B' || wall > BOUND_MS || wall < 0 ||
        ar.count != 1 || ar.byte != 'A') {
        printf("pty-concurrent: FAIL pair-b wall-ms=%ld bound=%d count=%d byte=%u\n", wall,
               BOUND_MS, br.count, (unsigned)br.byte);
        return 1;
    }
    printf("pty-concurrent: pair-a wall-ms=%d count=%d\n", ar.wall_ms, ar.count);
    printf("pty-concurrent: pair-b wall-ms=%ld bound=%d byte=%u\n", wall, BOUND_MS,
           (unsigned)br.byte);
    return 0;
}

static int pairs_eight(void) {
    int master[PAIRS], slave[PAIRS];
    char names[PAIRS][NAME_MAX];
    int ready[2], result[2];
    if (pipe(ready) != 0 || pipe(result) != 0) {
        printf("pty-concurrent: FAIL pairs pipe errno=%d\n", errno);
        return 1;
    }
    pid_t pids[PAIRS];
    for (int i = 0; i < PAIRS; i++) {
        if (open_pair(&master[i], &slave[i], names[i], sizeof(names[i])) != 0) {
            printf("pty-concurrent: FAIL pairs open index=%d errno=%d\n", i, errno);
            return 1;
        }
        pid_t pid = fork();
        if (pid == 0) {
            close(ready[0]);
            close(result[0]);
            reader_child(slave[i], master[i], ready[1], result[1], i);
        }
        pids[i] = pid;
        close(slave[i]);
    }
    close(ready[1]);
    close(result[1]);
    char byte;
    for (int i = 0; i < PAIRS; i++) {
        if (read_full(ready[0], &byte, 1) != 0) {
            printf("pty-concurrent: FAIL pairs ready index=%d errno=%d\n", i, errno);
            return 1;
        }
    }
    struct timespec pause = { 0, READ_ARM_MS * 1000000L };
    (void)nanosleep(&pause, NULL);
    int worst = 0;
    for (int i = 0; i < PAIRS; i++) {
        long start = now_ms();
        unsigned char want = (unsigned char)('a' + i);
        if (write_full(master[i], &want, 1) != 0) {
            printf("pty-concurrent: FAIL pairs write index=%d errno=%d\n", i, errno);
            return 1;
        }
        struct result r;
        if (wait_result(result[0], &r, BOUND_MS) != 0) {
            printf("pty-concurrent: FAIL pairs result index=%d timeout=%d\n", i, BOUND_MS);
            return 1;
        }
        long wall = now_ms() - start;
        if (wall > worst)
            worst = (int)wall;
        if (r.index != i || r.count != 1 || r.byte != want || wall > BOUND_MS) {
            printf("pty-concurrent: FAIL pairs index=%d child=%d count=%d byte=%u wall-ms=%ld bound=%d\n",
                   i, r.index, r.count, (unsigned)r.byte, wall, BOUND_MS);
            return 1;
        }
        printf("pty-concurrent: pair index=%d wall-ms=%ld\n", i, wall);
    }
    for (int i = 0; i < PAIRS; i++) {
        int status = 0;
        if (waitpid(pids[i], &status, 0) != pids[i] || !WIFEXITED(status) ||
            WEXITSTATUS(status) != 0) {
            printf("pty-concurrent: FAIL pairs child index=%d status=%d\n", i, status);
            return 1;
        }
        close(master[i]);
    }
    close(ready[0]);
    close(result[0]);
    printf("pty-concurrent: pairs=%d worst-ms=%d bound=%d\n", PAIRS, worst, BOUND_MS);
    return 0;
}

static int ninth_pair(void) {
    int master = -1, slave = -1;
    char name[NAME_MAX];
    if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
        printf("pty-concurrent: FAIL ninth open errno=%d\n", errno);
        return 1;
    }
    close(slave);
    close(master);
    printf("pty-concurrent: ninth alloc-close ok\n");
    return 0;
}

static int vtime_rows(void) {
    for (int index = 0; index < 2; index++) {
        int master = -1, slave = -1;
        char name[NAME_MAX];
        if (open_pair(&master, &slave, name, sizeof(name)) != 0) {
            printf("pty-concurrent: FAIL vtime open index=%d errno=%d\n", index, errno);
            return 1;
        }
        if (set_read_mode(slave, 0, 1) != 0) {
            printf("pty-concurrent: FAIL vtime mode index=%d errno=%d\n", index, errno);
            return 1;
        }
        long start = now_ms();
        unsigned char byte = 0;
        ssize_t count = read(slave, &byte, 1);
        long wall = now_ms() - start;
        if (count != 0 || wall < 50 || wall > 300) {
            printf("pty-concurrent: FAIL vtime index=%d count=%zd wall-ms=%ld\n", index, count,
                   wall);
            return 1;
        }
        printf("pty-concurrent: vtime index=%d count=0 wall-ms=%ld\n", index, wall);
        close(slave);
        close(master);
    }
    return 0;
}

int main(void) {
    printf("pty-concurrent: start uid=%d pairs=%d bound-ms=%d\n", (int)getuid(), PAIRS,
           BOUND_MS);
    fflush(stdout);
    if (pair_ab() != 0)
        return 1;
    if (pairs_eight() != 0)
        return 1;
    if (ninth_pair() != 0)
        return 1;
    if (vtime_rows() != 0)
        return 1;
    printf("pty-concurrent: idle-start\n");
    fflush(stdout);
    struct timespec idle = { 5, 0 };
    (void)nanosleep(&idle, NULL);
    printf("pty-concurrent: idle-end\n");
    int post_master = -1;
    int post_slave = -1;
    char post_name[NAME_MAX];
    if (open_pair(&post_master, &post_slave, post_name, sizeof(post_name)) != 0) {
        printf("pty-concurrent: FAIL post-idle open errno=%d\n", errno);
        return 1;
    }
    close(post_slave);
    close(post_master);
    printf("pty-concurrent: post-idle probe ok\n");
    printf("pty-concurrent: pass pairs=%d bound-ms=%d\n", PAIRS, BOUND_MS);
    fflush(stdout);
    return 0;
}
