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
    int length = snprintf(line, sizeof(line), "post-auth-fork: %s errno=%d\n", what, error);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
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

static int poll_readable(int fd, int timeout_ms) {
    struct pollfd waiting;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready;
    do {
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    return ready;
}

#define PORT 23083
#define REQUEST "POSTAUTH 5 hello"
#define RESPONSE "POSTOK 2 ok"

static int run_child(int connection) {
    if (poll_readable(connection, 5000) <= 0) {
        return fail("child poll", errno);
    }
    say("post-auth-fork: child poll ok\n");
    char request[sizeof(REQUEST) - 1];
    if (read_exact(connection, request, sizeof(request)) != 0 ||
        memcmp(request, REQUEST, sizeof(request)) != 0) {
        return fail("child read", errno);
    }
    say("post-auth-fork: child read ok\n");
    if (write(connection, RESPONSE, sizeof(RESPONSE) - 1) != (ssize_t)(sizeof(RESPONSE) - 1)) {
        return fail("child write", errno);
    }
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
    if (poll_readable(listener, 5000) <= 0) {
        return fail("poll listener", errno);
    }
    connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
    if (connection < 0) {
        return fail("accept", errno);
    }
    say("post-auth-fork: accepted\n");

    child = fork();
    if (child < 0) {
        return fail("fork", errno);
    }
    if (child == 0) {
        (void)close(listener);
        (void)close(client);
        int result = run_child(connection);
        (void)close(connection);
        _exit(result);
    }

    // The parent does not poll or read the accepted socket after the fork. It only sends the
    // request after a delay, then reaps the child and reads the response on its own client end.
    (void)close(connection);
    (void)close(listener);
    sleep(1);
    if (write(client, REQUEST, sizeof(REQUEST) - 1) != (ssize_t)(sizeof(REQUEST) - 1)) {
        return fail("parent write", errno);
    }
    say("post-auth-fork: parent request sent\n");
    if (waitpid(child, &status, 0) != child || status != 0) {
        char line[96];
        int length = snprintf(line, sizeof(line), "post-auth-fork: child status=%d\n", status);
        if (length > 0) {
            (void)raven_debug_write(line, (size_t)length);
        }
        say("post-auth-fork: fail\n");
        return 1;
    }
    char response[sizeof(RESPONSE) - 1];
    if (read_exact(client, response, sizeof(response)) != 0 ||
        memcmp(response, RESPONSE, sizeof(response)) != 0) {
        return fail("parent response", errno);
    }
    say("post-auth-fork: parent response ok\n");
    (void)close(client);
    say("post-auth-fork: ok\n");
    return 0;
}
