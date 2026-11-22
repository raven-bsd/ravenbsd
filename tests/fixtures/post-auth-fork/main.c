// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K247 post-auth fork fixture: a listener accepts a loopback connection and forks. The parent
// only writes the framed request; it does not poll/read the accepted socket. The child inherits
// the accepted fd/event and must poll, read the request and write the framed response.

#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#ifdef POST_AUTH_FORK_HOST
// Host baseline mode: ordinary glibc write, so the same phases can be traced on Linux.
static void trace_write(const char *bytes, size_t len) {
    ssize_t written = write(STDOUT_FILENO, bytes, len);
    (void)written;
}
#else
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

static void trace_write(const char *bytes, size_t len) {
    (void)raven_debug_write(bytes, len);
}
#endif

static void say(const char *text) {
    trace_write(text, strlen(text));
}

static void say_value(const char *prefix, long value, const char *suffix) {
    char line[192];
    int length = snprintf(line, sizeof(line), "%s%ld%s", prefix, value, suffix);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        trace_write(line, size);
    }
}

static void trace_poll(const char *phase, int fd, int ready, int revents) {
    char line[192];
    int length = snprintf(
        line,
        sizeof(line),
        "post-auth-fork: %s poll fd=%d ready=%d revents=%d\n",
        phase,
        fd,
        ready,
        revents);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        trace_write(line, size);
    }
}

static int fail(const char *what, int error) {
    char line[160];
    int length = snprintf(line, sizeof(line), "post-auth-fork: %s errno=%d\n", what, error);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)trace_write(line, size);
    }
    say("post-auth-fork: fail\n");
    return 1;
}

static int read_exact(int fd, char *buffer, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, buffer + done, length - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int poll_readable(const char *phase, int fd, int timeout_ms) {
    struct pollfd waiting;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready;
    do {
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    trace_poll(phase, fd, ready, (int)waiting.revents);
    return ready;
}

#define PORT 23083
#define REQUEST "POSTAUTH 5 hello"
#define RESPONSE "POSTOK 2 ok"

static int run_child(int connection) {
    if (poll_readable("child", connection, 5000) <= 0) {
        return fail("child poll", errno);
    }
    say("post-auth-fork: child poll ok\n");
    char request[sizeof(REQUEST) - 1];
    if (read_exact(connection, request, sizeof(request)) != 0 ||
        memcmp(request, REQUEST, sizeof(request)) != 0) {
        return fail("child read", errno);
    }
    say_value("post-auth-fork: child read bytes=", (long)sizeof(request), "\n");
    say("post-auth-fork: child read ok\n");
    if (write(connection, RESPONSE, sizeof(RESPONSE) - 1) != (ssize_t)(sizeof(RESPONSE) - 1)) {
        return fail("child write", errno);
    }
    say_value(
        "post-auth-fork: child write bytes=",
        (long)(sizeof(RESPONSE) - 1),
        "\n");
    say("post-auth-fork: child write ok\n");
    return 0;
}

int main(void) {
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
        return fail("socket", errno);
    }
    int reuse = 1;
    (void)setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
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
    say("post-auth-fork: listening\n");

    client = socket(AF_INET, SOCK_STREAM, 0);
    if (client < 0) {
        return fail("client socket", errno);
    }
    if (connect(client, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("connect", errno);
    }
    say_value("post-auth-fork: client fd=", (long)client, "\n");
    if (poll_readable("listener", listener, 5000) <= 0) {
        return fail("poll listener", errno);
    }
    connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
    if (connection < 0) {
        return fail("accept", errno);
    }
    say("post-auth-fork: accepted\n");
    say_value("post-auth-fork: accepted fd=", (long)connection, "\n");

    child = fork();
    if (child < 0) {
        return fail("fork", errno);
    }
    if (child == 0) {
        say_value("post-auth-fork: child pid=", (long)getpid(), "\n");
        say_value(
            "post-auth-fork: child connection fd=",
            (long)connection,
            "\n");
        (void)close(listener);
        (void)close(client);
        int result = run_child(connection);
        (void)close(connection);
        say_value("post-auth-fork: child exit=", (long)result, "\n");
        _exit(result);
    }

    // The parent does not poll or read the accepted socket after the fork. It only sends the
    // request after a delay, then reaps the child and reads the response on its own client end.
    say_value("post-auth-fork: parent pid=", (long)getpid(), "\n");
    (void)close(connection);
    say_value(
        "post-auth-fork: parent closed accepted fd=",
        (long)connection,
        "\n");
    (void)close(listener);
    sleep(1);
    if (write(client, REQUEST, sizeof(REQUEST) - 1) != (ssize_t)(sizeof(REQUEST) - 1)) {
        return fail("parent write", errno);
    }
    say_value(
        "post-auth-fork: parent request bytes=",
        (long)(sizeof(REQUEST) - 1),
        "\n");
    say("post-auth-fork: parent request sent\n");
    if (waitpid(child, &status, 0) != child || status != 0) {
        say_value("post-auth-fork: child status=", (long)status, "\n");
        char line[96];
        int length = snprintf(line, sizeof(line), "post-auth-fork: child status=%d\n", status);
        if (length > 0) {
            (void)trace_write(line, (size_t)length);
        }
        say("post-auth-fork: fail\n");
        return 1;
    }
    say_value("post-auth-fork: child status=", (long)status, "\n");
    char response[sizeof(RESPONSE) - 1];
    if (read_exact(client, response, sizeof(response)) != 0 ||
        memcmp(response, RESPONSE, sizeof(response)) != 0) {
        return fail("parent response", errno);
    }
    say_value(
        "post-auth-fork: parent response bytes=",
        (long)sizeof(response),
        "\n");
    say("post-auth-fork: parent response ok\n");
    (void)close(client);
    say("post-auth-fork: ok\n");
    return 0;
}
