// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K232 target fixture: an accepted TCP socket must stay level-ready for a second packet that is
// already queued after the first read, and `ppoll` must wake for a later packet. The client is a
// loopback socket in the same process, so the test has no external host race and still uses the
// real netd accepted-socket/event path.

#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

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

static int fail(const char *what, int error) {
    char line[160];
    int length = snprintf(line, sizeof(line), "net-two-reads: %s errno=%d\n", what, error);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("net-two-reads: fail\n");
    return 1;
}

static int read_exact(int fd, char *wanted, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, wanted + done, length - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int read_packet(int fd, const char *expected, size_t length) {
    char buffer[32];
    if (length > sizeof(buffer)) {
        return -1;
    }
    if (read_exact(fd, buffer, length) != 0) {
        return -1;
    }
    return memcmp(buffer, expected, length) == 0 ? 0 : -1;
}

static int poll_one(int fd, int timeout_ms) {
    struct pollfd waiting;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready;
    do {
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    if (ready < 0) {
        return -1;
    }
    return ready;
}

static int ppoll_one(int fd, int timeout_ms) {
    struct pollfd waiting;
    struct timespec timeout;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    timeout.tv_sec = timeout_ms / 1000;
    timeout.tv_nsec = (long)(timeout_ms % 1000) * 1000000L;
    int ready;
    do {
        ready = ppoll(&waiting, 1, &timeout, NULL);
    } while (ready < 0 && errno == EINTR);
    if (ready < 0) {
        return -1;
    }
    return ready;
}

#define PORT 23082

static int run_same_process(void) {
    struct sockaddr_in address;
    struct sockaddr_in peer;
    socklen_t peer_length = sizeof(peer);
    int listener;
    int client;
    int connection;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket", errno);
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0x7f000001);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("bind", errno);
    }
    if (listen(listener, 4) < 0) {
        return fail("listen", errno);
    }
    say("net-two-reads: listening\n");

    client = socket(AF_INET, SOCK_STREAM, 0);
    if (client < 0) {
        return fail("client socket", errno);
    }
    if (connect(client, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("connect", errno);
    }
    if (poll_one(listener, 5000) <= 0) {
        return fail("poll listener", errno);
    }
    connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
    if (connection < 0) {
        return fail("accept", errno);
    }
    say("net-two-reads: accepted\n");

    // The event is attached at accept and this segment carries two packets. The first poll/read
    // drains "alpha"; "bravo" stays in the socket queue with no new transition.
    if (write(client, "alphabravo", 10) != 10) {
        return fail("write first", errno);
    }
    say("net-two-reads: wrote first\n");
    if (poll_one(connection, 5000) <= 0) {
        return fail("poll first", errno);
    }
    if (read_packet(connection, "alpha", 5) != 0) {
        return fail("read first", errno);
    }
    say("net-two-reads: first ok\n");

    // The second packet is already queued. A stored poll verdict is not a readiness fact, so
    // read it directly; the blocking read waits on the live kernel queue if needed.
    if (read_packet(connection, "bravo", 5) != 0) {
        return fail("read second", errno);
    }
    say("net-two-reads: second ok\n");

    // No busy loop: no new data is queued until "charlie".
    int idle = poll_one(connection, 100);
    if (idle != 0) {
        return fail("poll busy", idle);
    }
    say("net-two-reads: idle ok\n");

    if (write(client, "charlie", 7) != 7) {
        return fail("write third", errno);
    }
    if (ppoll_one(connection, 5000) <= 0) {
        return fail("ppoll third", errno);
    }
    if (read_packet(connection, "charlie", 7) != 0) {
        return fail("read third", errno);
    }
    say("net-two-reads: third ok\n");

    (void)close(connection);
    (void)close(client);
    (void)close(listener);
    say("net-two-reads: ok\n");
    return 0;
}

static int run_exec_parent(void) {
    struct sockaddr_in address;
    struct sockaddr_in peer;
    socklen_t peer_length = sizeof(peer);
    int listener;
    int client;
    int connection;
    int status = 0;
    pid_t child;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("exec socket", errno);
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0x7f000001);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("exec bind", errno);
    }
    if (listen(listener, 4) < 0) {
        return fail("exec listen", errno);
    }
    say("net-two-reads-exec: listening\n");
    client = socket(AF_INET, SOCK_STREAM, 0);
    if (client < 0) {
        return fail("exec client socket", errno);
    }
    if (connect(client, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("exec connect", errno);
    }
    if (poll_one(listener, 5000) <= 0) {
        return fail("exec poll listener", errno);
    }
    connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
    if (connection < 0) {
        return fail("exec accept", errno);
    }
    say("net-two-reads-exec: accepted\n");

    // Two packets are queued before the fork. The parent reads only the first; the second stays
    // in the socket queue, and the Ofd event/replay state must survive the child's execve.
    if (write(client, "alphabravo", 10) != 10) {
        return fail("exec write first", errno);
    }
    if (poll_one(connection, 5000) <= 0) {
        return fail("exec poll first", errno);
    }
    if (read_packet(connection, "alpha", 5) != 0) {
        return fail("exec read first", errno);
    }
    say("net-two-reads-exec: parent first ok\n");

    child = fork();
    if (child < 0) {
        return fail("exec fork", errno);
    }
    if (child == 0) {
        int inherited = dup(connection);
        if (inherited < 0) {
            _exit(120);
        }
        (void)close(connection);
        (void)close(client);
        (void)close(listener);
        char fd_text[16];
        int length = snprintf(fd_text, sizeof(fd_text), "%d", inherited);
        if (length <= 0) {
            _exit(122);
        }
        char *args[] = {(char *)"/bin/net-two-reads-reader", fd_text, NULL};
        char *envp[] = {NULL};
        execve("/bin/net-two-reads-reader", args, envp);
        _exit(121);
    }
    (void)close(connection);
    (void)close(listener);
    // Poll on the parent's own descriptor while the child starts polling on its inherited one.
    // A shared event queue would answer EBUSY; the platform must split it after fork.
    if (poll_one(client, 100) < 0) {
        return fail("exec parent poll", errno);
    }
    sleep(1);
    if (write(client, "charlie", 7) != 7) {
        return fail("exec write third", errno);
    }
    if (waitpid(child, &status, 0) != child || status != 0) {
        char line[96];
        int length = snprintf(line, sizeof(line), "net-two-reads-exec: child status=%d\n", status);
        if (length > 0) {
            (void)raven_debug_write(line, (size_t)length);
        }
        say("net-two-reads-exec: fail\n");
        return 1;
    }
    if (read_packet(client, "two-reads-ack", 13) != 0) {
        return fail("exec read ack", errno);
    }
    (void)close(client);
    say("net-two-reads-exec: parent ok\n");
    say("net-two-reads-exec: ok\n");
    return 0;
}

static int run_reader(int argc, char **argv) {
    int fd = 4;
    int idle;

    if (argc > 1) {
        fd = atoi(argv[1]);
        if (fd < 0) {
            return fail("reader fd", errno);
        }
    }

    // A packet queued before exec is read directly: a stored poll verdict is not a readiness
    // fact. The blocking read waits/returns from the live kernel queue.
    if (read_packet(fd, "bravo", 5) != 0) {
        return fail("reader read second", errno);
    }
    say("net-two-reads-exec: child second ok\n");

    idle = poll_one(fd, 100);
    if (idle != 0) {
        return fail("reader poll busy", idle);
    }
    // Deliberately read without polling while the parent still delays "charlie". A blocking
    // socket read must wait on the doorbell and return data, never EAGAIN.
    char first = 0;
    if (read(fd, &first, 1) != 1) {
        return fail("reader blocking read", errno);
    }
    if (first != 'c') {
        return fail("reader blocking byte", errno);
    }
    if (read_packet(fd, "harlie", 6) != 0) {
        return fail("reader read third", errno);
    }
    say("net-two-reads-exec: child third ok\n");
    if (write(fd, "two-reads-ack", 13) != 13) {
        return fail("reader write ack", errno);
    }
    say("net-two-reads-exec: child ok\n");
    return 0;
}

int main(int argc, char **argv) {
    (void)argc;
    const char *name = argv[0] == NULL ? "" : argv[0];
    const char *slash = strrchr(name, '/');
    if (slash != NULL) {
        name = slash + 1;
    }
    if (strstr(name, "-reader") != NULL) {
        return run_reader(argc, argv);
    }
    if (strstr(name, "-exec") != NULL) {
        return run_exec_parent();
    }
    return run_same_process();
}
