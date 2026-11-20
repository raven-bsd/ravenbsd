// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K225: accepted socket across fork/exec. The parent accepts, the child dups the
// accepted fd and execs a static writer, and the host client must receive the
// exact banner and acknowledge it. The second connection is the no-exec control.

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

// The svcd service stdout is not the debug console; report through RAVEN DEBUG_WRITE (3).
static void debug_write(const char *bytes, size_t length) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t size asm("rsi") = length;
    __asm__ volatile("syscall" : "+r"(number) : "r"(pointer), "r"(size)
                     : "rcx", "r11", "memory");
}

static void say(const char *format, ...) {
    char buffer[512];
    va_list arguments;
    va_start(arguments, format);
    int length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(buffer)) {
            size = sizeof(buffer) - 1;
        }
        debug_write(buffer, size);
    }
}

#define PORT 23125
#define EXEC_BANNER "SSH-2.0-exec-pass\r\n"
#define NOEXEC_BANNER "SSH-2.0-noexec-pass\r\n"
#define EXEC_ACK "exec-ack"
#define NOEXEC_ACK "noexec-ack"

static int fail(const char *what) {
    say("tcp-accept-exec: fail %s errno=%d\n", what, errno);
    return 1;
}

static int read_exact(int fd, const char *wanted) {
    size_t want = strlen(wanted);
    size_t got = 0;
    while (got < want) {
        char buffer[64];
        ssize_t count = read(fd, buffer, want - got);
        if (count <= 0) {
            return -1;
        }
        if (memcmp(buffer, wanted + got, (size_t)count) != 0) {
            return -1;
        }
        got += (size_t)count;
    }
    return 0;
}

int main(void) {
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket");
    }
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    if (inet_pton(AF_INET, "10.0.2.15", &address.sin_addr) != 1) {
        return fail("inet_pton");
    }
    if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0) {
        return fail("bind");
    }
    if (listen(listener, 4) != 0) {
        return fail("listen");
    }
    say("tcp-accept-exec: listening 10.0.2.15:%d\n", PORT);

    for (int pass = 0; pass < 2; pass++) {
        struct sockaddr_in peer;
        socklen_t peer_len = sizeof(peer);
        int accepted = accept(listener, (struct sockaddr *)&peer, &peer_len);
        if (accepted < 0) {
            return fail("accept");
        }
        say("tcp-accept-exec: accepted fd=%d pass=%d peer=%s:%d\n", accepted, pass,
               inet_ntoa(peer.sin_addr), ntohs(peer.sin_port));

        if (pass == 0) {
            pid_t child = fork();
            if (child < 0) {
                return fail("fork");
            }
            if (child == 0) {
                if (dup2(accepted, 4) != 4) {
                    _exit(120);
                }
                close(accepted);
                close(listener);
                char *argv[] = {(char *)"/bin/tcp-accept-writer", (char *)EXEC_BANNER, NULL};
                char *envp[] = {NULL};
                execve("/bin/tcp-accept-writer", argv, envp);
                _exit(121);
            }
            int status = 0;
            if (waitpid(child, &status, 0) != child || status != 0) {
                say("tcp-accept-exec: child status=%d\n", status);
                return fail("waitpid");
            }
            say("tcp-accept-exec: exec child status=0\n");
            if (read_exact(accepted, EXEC_ACK) != 0) {
                return fail("exec ack");
            }
        } else {
            ssize_t written = write(accepted, NOEXEC_BANNER, strlen(NOEXEC_BANNER));
            if (written != (ssize_t)strlen(NOEXEC_BANNER)) {
                return fail("noexec write");
            }
            say("tcp-accept-exec: noexec wrote=%zd\n", written);
            if (read_exact(accepted, NOEXEC_ACK) != 0) {
                return fail("noexec ack");
            }
        }
        close(accepted);
    }

    close(listener);
    say("tcp-accept-exec: done\n");
    return 0;
}
