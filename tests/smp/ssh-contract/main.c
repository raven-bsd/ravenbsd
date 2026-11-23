// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K435: guest SSH kernel-contract conformance suite. It runs inside the RavenBSD image and
// prints one `CONTRACT <id> PASS|FAIL <detail>` line per row. The service exits non-zero on the
// first red row; the qtest fails on any FAIL. Rows:
//   K1 poll/select current level (no cache/replay)
//   K2 nonblocking read (EAGAIN/exact bytes/valid buffer)
//   K3 nonblocking write (EAGAIN/partial/success delivered on the same socket)
//   K4 dup and fork+exec identity/readiness
//   K5 PTY open/grant/unlock/ptsname, pair round-trip, termios defaults
//   K6 accept: accepted fd independent, listener still accepts
//   K7 pipe EOF: close write -> POLLIN|POLLHUP, read 0
//   K8 accepted+dup exec socket EOF: peer FIN -> readable, read 0, still writable
//      (NetBSD `sowritable`/FreeBSD `sowriteable` keep POLLOUT after the peer's FIN)
//   K8-shutdown: our own SHUT_WR -> the next write is EPIPE
//   K9 half-close: peer SHUT_WR -> readable/EOF, write allowed, then EPIPE + POLLHUP
//   K10 poll after EOF is level-correct (reports on the next call)

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#ifndef O_NONBLOCK
#define O_NONBLOCK 0x0004
#endif

extern int posix_openpt(int flags);
extern int grantpt(int fd);
extern int unlockpt(int fd);
extern char *ptsname(int fd);

#define K6_PORT 23097
#define KEOF_PORT 23098
#define KREADY_PORT 23096
#define K6_READY_MS 8000

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
static long raven_debug_write(const char *bytes, size_t len) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t length asm("rsi") = len;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(length)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)raven_debug_write(text, strlen(text));
}

static const char *row = "K0";

static void result(int ok, const char *detail) {
    char line[240];
    int length = snprintf(line, sizeof(line), "CONTRACT %s %s %s\n", row, ok ? "PASS" : "FAIL",
                          detail);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int fail(const char *what) {
    char detail[160];
    (void)snprintf(detail, sizeof(detail), "%s errno=%d", what, errno);
    result(0, detail);
    say("CONTRACT SUMMARY FAIL\n");
    return 1;
}

static void pass(const char *detail) {
    result(1, detail);
}

static int poll_one(int fd, short events, int timeout_ms) {
    struct pollfd pfd;
    int ready;

    // The SSH poll path always uses a nonzero timeout. The libc non-scheme zero-timeout query
    // is a separate libc gap; 1 ms still measures the current level, not a future edge.
    if (timeout_ms == 0) {
        timeout_ms = 100;
    }
    pfd.fd = fd;
    pfd.events = events;
    pfd.revents = 0;
    errno = 0;
    do {
        ready = poll(&pfd, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    if (ready >= 0) {
        // Do not leak a stale errno from the scheme Call::Poll readiness query inside the shim.
        errno = 0;
    }
    return ready < 0 ? -1 : pfd.revents;
}

static int select_read(int fd, int timeout_ms) {
    fd_set read_set;
    struct timeval timeout;
    int ready;

    if (timeout_ms == 0) {
        timeout_ms = 100;
    }
    FD_ZERO(&read_set);
    FD_SET(fd, &read_set);
    timeout.tv_sec = timeout_ms / 1000;
    timeout.tv_usec = (timeout_ms % 1000) * 1000;
    do {
        ready = select(fd + 1, &read_set, NULL, NULL, &timeout);
    } while (ready < 0 && errno == EINTR);
    return ready;
}

static int read_exact(int fd, void *buffer, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t got = read(fd, (char *)buffer + done, length - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int read_exact_trace(int fd, void *buffer, size_t length, ssize_t *last, int *last_errno) {
    size_t done = 0;
    int retries = 0;

    *last = 0;
    *last_errno = 0;
    while (done < length) {
        ssize_t got;
        errno = 0;
        got = read(fd, (char *)buffer + done, length - done);
        *last = got;
        *last_errno = errno;
        if (got > 0) {
            done += (size_t)got;
            retries = 0;
            continue;
        }
        if (got == 0) {
            return -1;
        }
        // A scheme read may return EAGAIN after the readiness doorbell was consumed by an
        // earlier query.  Wait for the next readable transition instead of treating it as EOF.
        if (errno == EAGAIN && retries < 12) {
            int revents = poll_one(fd, POLLIN | POLLHUP, 5000);
            if (revents < 0) {
                *last_errno = errno;
                return -1;
            }
            retries++;
            continue;
        }
        return -1;
    }
    return 0;
}

static int check_poll(void) {
    int sv[2];
    char byte = 'A';
    int revents;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("socketpair");
    }
    revents = poll_one(sv[1], POLLOUT, 0);
    if (revents < 0 || (revents & POLLOUT) == 0) {
        return fail("empty write-ready");
    }
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents != 0) {
        return fail("empty read-ready");
    }
    if (select_read(sv[0], 0) != 0) {
        return fail("select empty");
    }
    if (write(sv[1], &byte, 1) != 1) {
        return fail("write");
    }
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("read-ready");
    }
    if (select_read(sv[0], 0) != 1) {
        return fail("select ready");
    }
    if (read(sv[0], &byte, 1) != 1) {
        return fail("drain");
    }
    // Level-triggered: the consumed readable edge must not replay and the write level must be
    // recomputed from the current ring state.
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents != 0) {
        return fail("read replay");
    }
    if (select_read(sv[0], 0) != 0) {
        return fail("select replay");
    }
    if (close(sv[1]) != 0) {
        return fail("close writer");
    }
    revents = poll_one(sv[0], POLLIN | POLLHUP, 0);
    if (revents < 0 || (revents & POLLHUP) == 0) {
        return fail("hup");
    }
    if (close(sv[0]) != 0) {
        return fail("close reader");
    }
    pass("level=current drain=no-replay hup=seen");
    return 0;
}

static int check_read(void) {
    int sv[2];
    unsigned char payload[100];
    unsigned char buffer[100];
    size_t index;
    ssize_t count;

    for (index = 0; index < sizeof(payload); index++) {
        payload[index] = (unsigned char)(index + 1);
    }
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, sv) != 0) {
        return fail("socketpair");
    }
    if (write(sv[1], payload, sizeof(payload)) != (ssize_t)sizeof(payload)) {
        return fail("write");
    }
    count = read(sv[0], buffer, 10);
    if (count != 10 || memcmp(buffer, payload, 10) != 0) {
        return fail("partial first");
    }
    count = read(sv[0], buffer, 10);
    if (count != 10 || memcmp(buffer, payload + 10, 10) != 0) {
        return fail("partial second");
    }
    if (poll_one(sv[0], POLLIN, 0) == 0) {
        return fail("remaining poll");
    }
    count = read(sv[0], buffer, 80);
    if (count != 80 || memcmp(buffer, payload + 20, 80) != 0) {
        return fail("remainder");
    }
    if (poll_one(sv[0], POLLIN, 0) != 0) {
        return fail("drained poll");
    }
    errno = 0;
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != -1 || errno != EAGAIN) {
        return fail("empty EAGAIN");
    }
    if (close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("close");
    }
    pass("partial=10/10/80 eagain=1 eof=0");
    return 0;
}

static unsigned char big_write[128 * 1024];

static int check_write(void) {
    int sv[2];
    ssize_t first;
    ssize_t second;
    ssize_t want;
    ssize_t got;
    unsigned char scratch[4096];
    unsigned char final[4096];

    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, sv) != 0) {
        return fail("socketpair");
    }
    memset(big_write, 0x5a, sizeof(big_write));
    first = write(sv[1], big_write, sizeof(big_write));
    if (first <= 0 || first >= (ssize_t)sizeof(big_write)) {
        return fail("partial");
    }
    errno = 0;
    second = write(sv[1], big_write, 4096);
    if (second != -1 || errno != EAGAIN) {
        return fail("full EAGAIN");
    }
    want = first;
    while (want > 0) {
        got = read(sv[0], scratch,
                   want < (ssize_t)sizeof(scratch) ? (size_t)want : sizeof(scratch));
        if (got <= 0) {
            return fail("drain");
        }
        want -= got;
    }
    if (write(sv[1], big_write, 4096) != 4096) {
        return fail("after drain");
    }
    if (read_exact(sv[0], final, sizeof(final)) != 0 ||
        memcmp(final, big_write, sizeof(final)) != 0) {
        return fail("delivery same socket");
    }
    if (close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("close");
    }
    pass("partial=positive eagain=1 delivered=4096");
    return 0;
}

static void k4_watchdog(void) {
    sleep(12);
    say("CONTRACT K4 FAIL child-exit-hang watchdog\n");
    say("CONTRACT SUMMARY FAIL\n");
    (void)kill(getppid(), SIGKILL);
    _exit(0);
}

static int check_dup_exec(void) {
    int sv[2];
    int duplicate;
    char byte = 'D';
    pid_t child;
    pid_t watchdog;
    int status = 0;
    char fd_text[16];
    char *argv[3];
    char *envp[1] = {NULL};

    say("CONTRACT K4 CHILD stage=start\n");
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("dup socketpair");
    }
    duplicate = dup(sv[0]);
    if (duplicate < 0) {
        return fail("dup");
    }
    if (poll_one(sv[0], POLLIN, 0) != 0 || poll_one(duplicate, POLLIN, 0) != 0) {
        return fail("dup empty readiness");
    }
    if (write(sv[1], &byte, 1) != 1) {
        return fail("dup write");
    }
    // The duplicate and the original share one open file description, so the readable level is
    // visible through both and a drain through one clears it for both.
    if (poll_one(sv[0], POLLIN, 0) == 0 || poll_one(duplicate, POLLIN, 0) == 0) {
        return fail("dup shared readiness");
    }
    if (read(duplicate, &byte, 1) != 1) {
        return fail("dup read");
    }
    if (poll_one(sv[0], POLLIN, 0) != 0 || poll_one(duplicate, POLLIN, 0) != 0) {
        return fail("dup shared drain");
    }
    if (close(duplicate) != 0) {
        return fail("dup close duplicate");
    }

    watchdog = fork();
    if (watchdog == 0) {
        k4_watchdog();
    }

    // Use a fresh duplicate for the exec child, matching the rexec fdmap shape: the child gets
    // one stable descriptor that survives exec even if the original table also has holes.
    duplicate = dup(sv[0]);
    if (duplicate < 0) {
        return fail("exec dup");
    }
    say("CONTRACT K4 CHILD stage=fork\n");
    child = fork();
    say("CONTRACT K4 CHILD stage=forked\n");
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        say("CONTRACT K4 CHILD stage=child\n");
        (void)close(sv[0]);
        (void)close(sv[1]);
        // Keep only the inherited contract socket; the service's net/pty/event handles are not
        // part of K4 and would otherwise be torn down by the exec child.
        for (int fd = 0; fd < 64; fd++) {
            if (fd != duplicate) {
                (void)close(fd);
            }
        }
        if (snprintf(fd_text, sizeof(fd_text), "%d", duplicate) <= 0) {
            _exit(2);
        }
        argv[0] = (char *)"ssh-contract-child";
        argv[1] = fd_text;
        argv[2] = NULL;
        execve("/bin/ssh-contract-child", argv, envp);
        {
            char line[96];
            (void)snprintf(line, sizeof(line), "CONTRACT K4 CHILD exec fail errno=%d\n", errno);
            say(line);
        }
        _exit(3);
    }
    (void)close(duplicate);
    if (write(sv[1], "PING", 4) != 4) {
        return fail("exec parent write");
    }
    say("CONTRACT K4 CHILD stage=wait\n");
    {
        int exited = 0;
        int loop;
        for (loop = 0; loop < 8; loop++) {
            pid_t waited = waitpid(child, &status, WNOHANG);
            {
                char line[96];
                (void)snprintf(line, sizeof(line),
                               "CONTRACT K4 CHILD waitpid=%d status=%d errno=%d\n", (int)waited,
                               status, errno);
                say(line);
            }
            if (waited == child) {
                exited = 1;
                break;
            }
            if (waited < 0 && errno != EINTR) {
                break;
            }
            say("CONTRACT K4 CHILD stage=sleep\n");
            sleep(1);
        }
        if (!exited) {
            (void)kill(child, SIGKILL);
            (void)waitpid(child, &status, 0);
            result(0, "exec child stuck no PONG");
            say("CONTRACT SUMMARY FAIL\n");
            return 1;
        }
        if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            char detail[96];
            (void)snprintf(detail, sizeof(detail), "exec child status=%d", status);
            result(0, detail);
            say("CONTRACT SUMMARY FAIL\n");
            return 1;
        }
        {
            char pong[4];
            if (read_exact(sv[1], pong, sizeof(pong)) != 0 ||
                memcmp(pong, "PONG", sizeof(pong)) != 0) {
                result(0, "exec child PONG missing");
                say("CONTRACT SUMMARY FAIL\n");
                return 1;
            }
        }
    }
    if (close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("exec close");
    }
    if (watchdog > 0) {
        (void)kill(watchdog, SIGKILL);
    }
    pass("dup=shared exec=child-same-socket");
    return 0;
}

static int check_pty(void) {
    int master;
    int slave;
    struct termios settings;
    char name[128];
    char buffer[32];
    ssize_t count;
    int revents;

    master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0) {
        return fail("posix_openpt");
    }
    if (grantpt(master) != 0) {
        return fail("grantpt");
    }
    if (unlockpt(master) != 0) {
        return fail("unlockpt");
    }
    if (ptsname == NULL) {
        errno = ENOSYS;
        return fail("ptsname symbol missing");
    }
    if (ptsname(master) == NULL) {
        return fail("ptsname");
    }
    (void)snprintf(name, sizeof(name), "%s", ptsname(master));
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        return fail("open slave");
    }
    if (!isatty(master) || !isatty(slave)) {
        return fail("isatty");
    }
    if (tcgetattr(slave, &settings) != 0) {
        return fail("tcgetattr");
    }
    if ((settings.c_lflag & ICANON) == 0 || (settings.c_lflag & ECHO) == 0) {
        return fail("termios defaults");
    }
    if (write(master, "hello\n", 6) != 6) {
        return fail("master write");
    }
    revents = poll_one(slave, POLLIN, 2000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("slave poll");
    }
    count = read(slave, buffer, sizeof(buffer));
    if (count < 5 || memcmp(buffer, "hello", 5) != 0) {
        return fail("slave roundtrip");
    }
    // Drain the master-side echo queued by the line discipline before the slave output test.
    if (poll_one(master, POLLIN, 2000) > 0) {
        (void)read(master, buffer, sizeof(buffer));
    }
    if (write(slave, "world\n", 6) != 6) {
        return fail("slave write");
    }
    revents = poll_one(master, POLLIN, 2000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("master poll");
    }
    count = read(master, buffer, sizeof(buffer));
    if (count < 5 || memcmp(buffer, "world", 5) != 0) {
        return fail("master roundtrip");
    }
    if (close(slave) != 0 || close(master) != 0) {
        return fail("pty close");
    }
    pass("pair=open termios=icanon-echo roundtrip=both");
    return 0;
}

static int accept_one(int listener, const char *payload, const char *ack) {
    int fd;
    char buffer[32];
    struct sockaddr_in peer;
    socklen_t peer_len = sizeof(peer);
    size_t length = strlen(payload);

    fd = accept(listener, NULL, NULL);
    if (fd < 0) {
        return -1;
    }
    // K548: the accepted fd must carry the host peer's address, not the UNKNOWN/zero peer the
    // pre-fix inheritance path returned. The hostfwd client arrives as the slirp gateway
    // 10.0.2.2 with a live port; 0.0.0.0:0 or a getpeername error is the failure.
    memset(&peer, 0, sizeof(peer));
    if (getpeername(fd, (struct sockaddr *)&peer, &peer_len) != 0) {
        (void)close(fd);
        return -1;
    }
    if (peer_len != sizeof(peer) || peer.sin_family != AF_INET ||
        peer.sin_addr.s_addr != inet_addr("10.0.2.2") || peer.sin_port == 0) {
        (void)close(fd);
        errno = EPROTO;
        return -1;
    }
    if (read_exact(fd, buffer, length) != 0 || memcmp(buffer, payload, length) != 0) {
        (void)close(fd);
        errno = EPROTO;
        return -1;
    }
    if (write(fd, payload, length) != (ssize_t)length) {
        (void)close(fd);
        return -1;
    }
    if (read_exact(fd, buffer, strlen(ack)) != 0 ||
        memcmp(buffer, ack, strlen(ack)) != 0) {
        (void)close(fd);
        errno = EPROTO;
        return -1;
    }
    (void)close(fd);
    return 0;
}

static int check_listen_only(void) {
    int listener;
    struct sockaddr_in address;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("listen-only socket");
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(23099);
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0) {
        (void)close(listener);
        return fail("listen-only bind");
    }
    errno = 0;
    if (listen(listener, 2) != 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "bind ok, listen errno=%d", errno);
        (void)close(listener);
        return fail(detail);
    }
    (void)close(listener);
    pass("bind ok listen ok");
    return 0;
}

static long long monotonic_ms(void) {
    struct timespec now;

    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return -1;
    }
    return (long long)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}

// The host may connect before the guest stack is up; such a connection can outlive the host's
// retry and be accepted here after the host has closed it. Require an explicit host ACK on the
// same connection so a dead READY never satisfies the handshake. The whole exchange is bounded
// by K6_READY_MS of guest time.
static int signal_k6_ready(void) {
    int listener;
    struct sockaddr_in address;
    long long deadline;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return -1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(KREADY_PORT);
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0 ||
        listen(listener, 2) != 0) {
        (void)close(listener);
        return -1;
    }
    deadline = monotonic_ms();
    if (deadline < 0) {
        (void)close(listener);
        return -1;
    }
    deadline += K6_READY_MS;
    for (;;) {
        long long now = monotonic_ms();
        long long remaining;
        int conn;
        int revents;
        char ack;

        if (now < 0) {
            (void)close(listener);
            return -1;
        }
        remaining = deadline - now;
        if (remaining <= 0) {
            (void)close(listener);
            return -1;
        }
        revents = poll_one(listener, POLLIN, (int)remaining);
        if (revents < 0 || (revents & POLLIN) == 0) {
            (void)close(listener);
            return -1;
        }
        conn = accept(listener, NULL, NULL);
        if (conn < 0) {
            (void)close(listener);
            return -1;
        }
        if (send(conn, "READY\n", 6, 0) == 6 &&
            poll_one(conn, POLLIN, 1000) > 0 &&
            read(conn, &ack, 1) == 1 && ack == 'A') {
            (void)close(conn);
            (void)close(listener);
            return 0;
        }
        (void)close(conn);
    }
}

static int check_accept(void) {
    int listener;
    struct sockaddr_in address;
    int revents;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket");
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(K6_PORT);
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0) {
        return fail("bind");
    }
    if (listen(listener, 2) != 0) {
        return fail("listen");
    }
    // The K6 listener is already bound.  Tell the host peer it may open the first connection;
    // connecting before this marker is what makes the first poll race under load.
    if (signal_k6_ready() != 0) {
        return fail("k6 ready");
    }
    revents = poll_one(listener, POLLIN, 5000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "listener poll revents=%d errno=%d",
                       revents, errno);
        return fail(detail);
    }
    if (accept_one(listener, "ACCEPT1", "K6ACK1") != 0) {
        return fail("accept first");
    }
    // The listener must still accept while the first accepted descriptor has been used.
    revents = poll_one(listener, POLLIN, 5000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("listener second poll");
    }
    if (accept_one(listener, "ACCEPT2", "K6ACK2") != 0) {
        return fail("accept second");
    }
    if (close(listener) != 0) {
        return fail("listener close");
    }
    pass("accepted=independent listener=still-accepts");
    return 0;
}

static int eof_listener = -1;

static int eof_listener_open(void) {
    int listener;
    struct sockaddr_in address;
    int reuse = 1;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return -1;
    }
    (void)setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(KEOF_PORT);
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0 ||
        listen(listener, 2) != 0) {
        (void)close(listener);
        return -1;
    }
    return listener;
}

static int accept_eof(void) {
    int conn;

    if (eof_listener < 0) {
        errno = EBADF;
        return -1;
    }
    if (poll_one(eof_listener, POLLIN, 5000) <= 0) {
        errno = ETIMEDOUT;
        return -1;
    }
    conn = accept(eof_listener, NULL, NULL);
    return conn;
}

static int check_eof_pipe(void) {
    int pv[2];
    char byte = 0;
    int revents;

    if (pipe(pv) != 0) {
        return fail("eof pipe");
    }
    if (close(pv[1]) != 0) {
        return fail("eof pipe close write");
    }
    revents = poll_one(pv[0], POLLIN | POLLHUP, 2000);
    if (revents < 0 || (revents & (POLLIN | POLLHUP)) != (POLLIN | POLLHUP)) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "pipe eof revents=0x%x", (unsigned)revents);
        return fail(detail);
    }
    if (read(pv[0], &byte, 1) != 0) {
        return fail("pipe eof read");
    }
    (void)close(pv[0]);
    pass("pipe revents=POLLIN|POLLHUP read=0");
    return 0;
}

static int check_eof_socket(void) {
    int conn;
    int status = 0;
    pid_t child;
    char fd_text[16];
    char *argv[4];
    char byte = 0;
    int revents;

    eof_listener = eof_listener_open();
    if (eof_listener < 0) {
        return fail("eof listener");
    }
    conn = accept_eof();
    if (conn < 0) {
        return fail("eof accept");
    }
    if (send(conn, "EOF\n", 4, 0) != 4) {
        return fail("eof greeting");
    }
    child = fork();
    if (child < 0) {
        return fail("eof fork");
    }
    if (child == 0) {
        (void)close(eof_listener);
        if (dup2(conn, 7) < 0) {
            _exit(2);
        }
        for (int fd = 0; fd < 64; fd++) {
            if (fd != 7) {
                (void)close(fd);
            }
        }
        if (snprintf(fd_text, sizeof(fd_text), "%d", 7) <= 0) {
            _exit(3);
        }
        argv[0] = (char *)"ssh-contract-child";
        argv[1] = fd_text;
        argv[2] = (char *)"eof";
        argv[3] = NULL;
        execve("/bin/ssh-contract-child", argv, (char *const[]){NULL});
        _exit(4);
    }
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "eof child status=%d", status);
        return fail(detail);
    }
    revents = poll_one(conn, POLLIN | POLLHUP, 2000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "eof parent revents=0x%x", (unsigned)revents);
        return fail(detail);
    }
    if (read(conn, &byte, 1) != 0) {
        return fail("eof parent read");
    }
    // NetBSD `sowritable`/FreeBSD `sowriteable` keep POLLOUT after the peer's FIN: EOF is a
    // receive-side condition and the local send side is still open until our own SHUT_WR.
    revents = poll_one(conn, POLLOUT, 500);
    if (revents < 0 || (revents & POLLOUT) == 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "eof socket not writable revents=0x%x",
                       (unsigned)revents);
        return fail(detail);
    }
    (void)close(conn);
    pass("accepted+dup exec read=0 eof-writable");
    return 0;
}

static int check_own_shutdown(void) {
    int conn;
    ssize_t wrote;
    int revents;

    conn = accept_eof();
    if (conn < 0) {
        return fail("shutdown accept");
    }
    if (send(conn, "SHUT\n", 5, 0) != 5) {
        return fail("shutdown greeting");
    }
    if (shutdown(conn, SHUT_WR) != 0) {
        return fail("own shutdown write");
    }
    // The donor predicates keep POLLOUT set after SS_CANTSENDMORE so a blocked writer wakes and
    // observes the error; the functional "unwritable" assertion is the EPIPE on the next write.
    revents = poll_one(conn, POLLOUT, 500);
    errno = 0;
    wrote = write(conn, "X", 1);
    if (wrote >= 0 || errno != EPIPE) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail),
                       "own shutdown write errno=%d poll=0x%x", errno, (unsigned)revents);
        return fail(detail);
    }
    (void)close(conn);
    pass("own-shutdown write=EPIPE");
    return 0;
}

static int check_half_close(void) {
    int conn;
    char data[4];
    int revents;
    ssize_t wrote;

    conn = accept_eof();
    if (conn < 0) {
        return fail("half accept");
    }
    {
        int one = 1;
        // The K9 peer half-closes and then closes with RST; the contract is about HUP/EPIPE, not
        // Nagle. sshd sets TCP_NODELAY on its sessions, so the accepted contract socket does too.
        if (setsockopt(conn, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one)) != 0) {
            return fail("half nodelay");
        }
    }
    if (send(conn, "HALF\n", 5, 0) != 5) {
        return fail("half greeting");
    }
    revents = poll_one(conn, POLLIN | POLLHUP, 5000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        char detail[96];
        (void)snprintf(detail, sizeof(detail), "half pre-read poll revents=0x%x errno=%d",
                       (unsigned)revents, errno);
        return fail(detail);
    }
    {
        ssize_t last = 0;
        int read_errno = 0;
        if (read_exact_trace(conn, data, sizeof(data), &last, &read_errno) != 0 ||
            memcmp(data, "DATA", 4) != 0) {
            char detail[96];
            errno = read_errno;
            (void)snprintf(detail, sizeof(detail), "half data got=%zd read_errno=%d",
                           last, read_errno);
            return fail(detail);
        }
    }
    revents = poll_one(conn, POLLIN, 2000);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("half readable");
    }
    if (read(conn, data, 1) != 0) {
        return fail("half eof read");
    }
    revents = poll_one(conn, POLLOUT, 2000);
    if (revents < 0 || (revents & POLLOUT) == 0) {
        return fail("half writable");
    }
    wrote = write(conn, "HALFACK", 7);
    if (wrote != 7) {
        return fail("half ack write");
    }
    // The host closes after HALFACK. A first write after its close may be accepted locally and
    // trigger the peer's RST; the next level/readiness query is the HUP, and the next write is
    // the EPIPE the contract requires.
    (void)poll(NULL, 0, 50);
    wrote = write(conn, "X", 1);
    {
        ssize_t probe = wrote;
        int probe_errno = wrote < 0 ? errno : 0;
        revents = poll_one(conn, POLLIN | POLLHUP, 3000);
        for (int attempt = 0; attempt < 50 && (revents & POLLHUP) == 0; attempt++) {
            (void)poll(NULL, 0, 20);
            revents = poll_one(conn, POLLIN | POLLHUP, 100);
        }
        if (revents < 0 || (revents & POLLHUP) == 0) {
            int so_error = 0;
            socklen_t so_len = sizeof(so_error);
            int so_rc = getsockopt(conn, SOL_SOCKET, SO_ERROR, &so_error, &so_len);
            char detail[192];
            (void)snprintf(detail, sizeof(detail),
                           "half hup revents=0x%x errno=%d x_write=%zd x_errno=%d "
                           "so_rc=%d so_error=%d",
                           (unsigned)revents, errno, probe, probe_errno, so_rc, so_error);
            return fail(detail);
        }
    }
    wrote = write(conn, "X", 1);
    if (wrote >= 0 || errno != EPIPE) {
        return fail("half write EPIPE");
    }
    (void)close(conn);
    (void)close(eof_listener);
    eof_listener = -1;
    pass("half-close eof=0 writable then EPIPE|POLLHUP");
    return 0;
}

static int check_eof_level(void) {
    int pv[2];
    char byte = 0;
    int first;
    int second;
    int after;

    if (pipe(pv) != 0) {
        return fail("level pipe");
    }
    if (close(pv[1]) != 0) {
        return fail("level close write");
    }
    first = poll_one(pv[0], POLLIN | POLLHUP, 2000);
    second = poll_one(pv[0], POLLIN | POLLHUP, 2000);
    if (first < 0 || second < 0 ||
        (first & (POLLIN | POLLHUP)) != (POLLIN | POLLHUP) ||
        (second & (POLLIN | POLLHUP)) != (POLLIN | POLLHUP)) {
        return fail("level repeat");
    }
    if (read(pv[0], &byte, 1) != 0) {
        return fail("level read");
    }
    after = poll_one(pv[0], POLLIN | POLLHUP, 2000);
    if (after < 0 || (after & POLLHUP) == 0) {
        return fail("level after read");
    }
    (void)close(pv[0]);
    pass("pipe eof two polls + hup after read");
    return 0;
}

#define DIAG_LOOPS 50

static void diag_load(char *out, size_t size) {
    int fd;
    ssize_t got;

    (void)snprintf(out, size, "n/a");
    fd = open("/proc/loadavg", O_RDONLY);
    if (fd < 0) {
        return;
    }
    got = read(fd, out, size - 1);
    (void)close(fd);
    if (got <= 0) {
        (void)snprintf(out, size, "n/a");
        return;
    }
    out[got] = '\0';
    for (ssize_t index = 0; index < got; index++) {
        if (out[index] == '\n') {
            out[index] = '\0';
            break;
        }
    }
}

static void diag_log(const char *tag, int iter, const char *step, int fd, int events,
                     int revents, int error) {
    char load[64];
    char line[320];

    diag_load(load, sizeof(load));
    (void)snprintf(line, sizeof(line),
                   "DIAG %s FAIL iter=%d step=%s fd=%d events=0x%x revents=0x%x errno=%d load=%s\n",
                   tag, iter, step, fd, (unsigned)events, (unsigned)revents, error, load);
    say(line);
}

static int diag_k1(void) {
    for (int iter = 1; iter <= DIAG_LOOPS; iter++) {
        int sv[2];
        char byte = 'A';
        int revents;
        int selected;

        if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
            diag_log("K1", iter, "socketpair", -1, 0, 0, errno);
            return 1;
        }
        revents = poll_one(sv[1], POLLOUT, 0);
        if (revents < 0 || (revents & POLLOUT) == 0) {
            diag_log("K1", iter, "write-ready", sv[1], POLLOUT, revents, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        revents = poll_one(sv[0], POLLIN, 0);
        if (revents != 0) {
            diag_log("K1", iter, "empty-read-ready", sv[0], POLLIN, revents, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        selected = select_read(sv[0], 0);
        if (selected != 0) {
            diag_log("K1", iter, "empty-select", sv[0], POLLIN, selected, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        if (write(sv[1], &byte, 1) != 1) {
            diag_log("K1", iter, "write", sv[1], POLLOUT, 0, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        revents = poll_one(sv[0], POLLIN, 2000);
        if (revents < 0 || (revents & POLLIN) == 0) {
            diag_log("K1", iter, "read-ready", sv[0], POLLIN, revents, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        selected = select_read(sv[0], 2000);
        if (selected <= 0) {
            diag_log("K1", iter, "select-ready", sv[0], POLLIN, selected, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        if (read(sv[0], &byte, 1) != 1) {
            diag_log("K1", iter, "read", sv[0], POLLIN, revents, errno);
            (void)close(sv[0]);
            (void)close(sv[1]);
            return 1;
        }
        (void)close(sv[0]);
        (void)close(sv[1]);
    }
    say("DIAG K1 PASS loops=50\n");
    return 0;
}

static int diag_k6(void) {
    struct sockaddr_in local;
    int listener;
    int revents;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        diag_log("K6", 0, "socket", -1, 0, 0, errno);
        return 1;
    }
    memset(&local, 0, sizeof(local));
    local.sin_family = AF_INET;
    local.sin_addr.s_addr = htonl(INADDR_ANY);
    local.sin_port = htons(K6_PORT);
    if (bind(listener, (struct sockaddr *)&local, sizeof(local)) != 0) {
        diag_log("K6", 0, "bind", listener, 0, 0, errno);
        (void)close(listener);
        return 1;
    }
    if (listen(listener, 2) != 0) {
        diag_log("K6", 0, "listen", listener, 0, 0, errno);
        (void)close(listener);
        return 1;
    }

    for (int iter = 1; iter <= DIAG_LOOPS; iter++) {
        int conn;
        char data[4];

        revents = poll_one(listener, POLLIN, 5000);
        if (revents < 0 || (revents & POLLIN) == 0) {
            diag_log("K6", iter, "listener-poll", listener, POLLIN, revents, errno);
            (void)close(listener);
            return 1;
        }
        conn = accept(listener, NULL, NULL);
        if (conn < 0) {
            diag_log("K6", iter, "accept", listener, POLLIN, revents, errno);
            (void)close(listener);
            return 1;
        }
        if (read_exact(conn, data, sizeof(data)) != 0 ||
            memcmp(data, "PING", 4) != 0) {
            diag_log("K6", iter, "read", conn, POLLIN, revents, errno);
            (void)close(conn);
            (void)close(listener);
            return 1;
        }
        if (write(conn, data, sizeof(data)) != (ssize_t)sizeof(data)) {
            diag_log("K6", iter, "write", conn, POLLOUT, 0, errno);
            (void)close(conn);
            (void)close(listener);
            return 1;
        }
        (void)close(conn);
    }
    (void)close(listener);
    say("DIAG K6 PASS loops=50\n");
    return 0;
}

static int diag_main(void) {
    say("DIAG START\n");
    if (diag_k1() != 0) {
        return 1;
    }
    if (diag_k6() != 0) {
        return 1;
    }
    say("DIAG SUMMARY PASS\n");
    return 0;
}

int main(int argc, char **argv) {
#ifdef SSH_CONTRACT_DIAG_BINARY
    (void)argc;
    (void)argv;
    signal(SIGPIPE, SIG_IGN);
    return diag_main();
#else
    signal(SIGPIPE, SIG_IGN);
    if (argc > 1 && strcmp(argv[1], "diag") == 0) {
        return diag_main();
    }
    row = "K1";
    if (check_poll() != 0) {
        return 1;
    }
    row = "K2";
    if (check_read() != 0) {
        return 1;
    }
    row = "K3";
    if (check_write() != 0) {
        return 1;
    }
    row = "K4";
    if (check_dup_exec() != 0) {
        return 1;
    }
    row = "K5";
    if (check_pty() != 0) {
        return 1;
    }
    row = "K6-listen";
    if (check_listen_only() != 0) {
        return 1;
    }
    row = "K6";
    if (check_accept() != 0) {
        return 1;
    }
    row = "K7";
    if (check_eof_pipe() != 0) {
        return 1;
    }
    row = "K8";
    if (check_eof_socket() != 0) {
        return 1;
    }
    row = "K8-shutdown";
    if (check_own_shutdown() != 0) {
        return 1;
    }
    row = "K9";
    if (check_half_close() != 0) {
        return 1;
    }
    row = "K10";
    if (check_eof_level() != 0) {
        return 1;
    }
    say("CONTRACT SUMMARY PASS 10/10\n");
    return 0;
#endif
}
