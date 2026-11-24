// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K231 target fixture, parent half: the listener of an `sshd` that hands each accepted
// connection to a freshly exec'd session process. It accepts through `poll`, forks, passes the
// accepted socket to `/bin/socket-inet-exec-child` as descriptor 7, waits for it, and then
// closes its own descriptor. The child (child.c) does the protocol; the parent proves that the
// accepted socket's doorbell is released by `close` (K231: it used to fail with EBADF).

#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#define PORT 23081
#define CHILD "/bin/socket-inet-exec-child"
#define CHILD_FD 7
#define CHILD_STALE 3

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

static void say_value(const char *prefix, long value, const char *suffix) {
    char line[160];
    int length = snprintf(line, sizeof(line), "%s%ld%s", prefix, value, suffix);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int fail(const char *what) {
    char line[160];
    int length = snprintf(line, sizeof(line), "socket-inet-exec: fail %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    return 1;
}

static void say_peer(int connection, const struct sockaddr_in *peer) {
    char line[192];
    uint32_t address = ntohl(peer->sin_addr.s_addr);
    int length = snprintf(
        line,
        sizeof(line),
        "socket-inet-exec: accepted fd=%d peer=%u.%u.%u.%u:%u\n",
        connection,
        (unsigned)((address >> 24) & 0xffu),
        (unsigned)((address >> 16) & 0xffu),
        (unsigned)((address >> 8) & 0xffu),
        (unsigned)(address & 0xffu),
        (unsigned)ntohs(peer->sin_port));
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

int main(void) {
    struct sockaddr_in address;
    struct sockaddr_in peer;
    socklen_t peer_length;
    int listener;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket");
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("bind");
    }
    if (listen(listener, 8) < 0) {
        return fail("listen");
    }
    say_value("socket-inet-exec: listening port=", PORT, "\n");

    // A host retry that overlapped the listener start can leave a connection nobody is behind;
    // the child reports those as stale and the next session gets a fresh child.
    for (int session = 0; session < 8; session++) {
        struct pollfd waiting;
        int ready = 0;
        waiting.fd = listener;
        waiting.events = POLLIN;
        for (int attempt = 0; attempt < 120; attempt++) {
            waiting.revents = 0;
            ready = poll(&waiting, 1, 1000);
            if (ready > 0) {
                break;
            }
            if (ready < 0 && errno != EINTR) {
                return fail("poll listener");
            }
        }
        if (ready <= 0 || (waiting.revents & POLLIN) == 0) {
            return fail("poll timeout");
        }

        peer_length = sizeof(peer);
        int connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
        if (connection < 0) {
            return fail("accept");
        }
        say_peer(connection, &peer);

        pid_t child = fork();
        if (child < 0) {
            return fail("fork");
        }
        if (child == 0) {
            if (connection != CHILD_FD && dup2(connection, CHILD_FD) != CHILD_FD) {
                _exit(120);
            }
            if (connection != CHILD_FD) {
                (void)close(connection);
            }
            (void)close(listener);
            char *argv[] = {(char *)CHILD, NULL};
            char *envp[] = {NULL};
            execve(CHILD, argv, envp);
            _exit(121);
        }

        int status = 0;
        if (waitpid(child, &status, 0) != child) {
            return fail("waitpid");
        }
        int code = WIFEXITED(status) ? WEXITSTATUS(status) : 255;
        say_value("socket-inet-exec: child status=", code, "\n");

        // The accepted socket carries a readiness doorbell; releasing it must not fail.
        if (close(connection) < 0) {
            return fail("close connection");
        }
        if (code == 0) {
            if (close(listener) < 0) {
                return fail("close listener");
            }
            say("socket-inet-exec: ok\n");
            return 0;
        }
        if (code == CHILD_STALE) {
            say("socket-inet-exec: stale connection\n");
            continue;
        }
        return fail("child");
    }
    return fail("no complete session");
}
