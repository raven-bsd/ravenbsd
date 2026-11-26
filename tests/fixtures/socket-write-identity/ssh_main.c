// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K431 S5 identity fixture: the SSH shape. The guest owns an accepted TCP connection on the tap,
// duplicates it onto fd 7 / fd 1 / fd 2, then fork+execs a child. The child writes a sentinel on
// each descriptor; the host peer must receive the exact bytes on the same connection, in order.
// The child's fd 1/2 sentinels must not appear on the guest console. After the child exits, the
// parent writes again on its still-open accepted fd to prove the peer stayed open.

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define PORT 23097
#define CHILD_PATH "/bin/socket-write-identity-ssh-child"
#define CHILD_FD 7

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
    char line[192];
    int length = snprintf(line, sizeof(line), "%s%ld%s", prefix, value, suffix);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int write_all(int fd, const char *data, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t wrote = write(fd, data + done, length - done);
        if (wrote < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)wrote;
    }
    return 0;
}

static int read_exact(int fd, char *data, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, data + done, length - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

int main(void) {
    struct sockaddr_in address;
    struct stat accepted_stat;
    int listener;
    int connection;
    pid_t child;
    int status = 0;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        say_value("socket-write-identity: socket errno=", errno, "\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        say_value("socket-write-identity: bind errno=", errno, "\n");
        return 1;
    }
    if (listen(listener, 8) < 0) {
        say_value("socket-write-identity: listen errno=", errno, "\n");
        return 1;
    }
    say("socket-write-identity: listening port=23097\n");

    connection = accept(listener, NULL, NULL);
    if (connection < 0) {
        say_value("socket-write-identity: accept errno=", errno, "\n");
        return 1;
    }
    say_value("socket-write-identity: accepted fd=", connection, "\n");
    if (fstat(connection, &accepted_stat) < 0) {
        say_value("socket-write-identity: accepted fstat errno=", errno, "\n");
        return 1;
    }
    if (!S_ISSOCK(accepted_stat.st_mode) || accepted_stat.st_ino == 0) {
        say_value("socket-write-identity: accepted ino=", (long)accepted_stat.st_ino,
                  " not a socket\n");
        return 1;
    }
    say_value("socket-write-identity: accepted ino=", (long)accepted_stat.st_ino, "\n");

    // K434 rev 18 shell shape: exec /bin/rsh with the socket on fd 1/2 and no RSH_FD* variables.
    // Without the libc stdio-handle publication the shell falls back to tty:console and the
    // sentinels reach the serial console instead of this host connection.
    child = fork();
    if (child < 0) {
        say_value("socket-write-identity: shell fork errno=", errno, "\n");
        return 1;
    }
    if (child == 0) {
        char *arguments[4];

        close(listener);
        if (dup2(connection, 0) < 0 || dup2(connection, 1) < 0 ||
            dup2(connection, 2) < 0) {
            say_value("socket-write-identity: shell dup2 errno=", errno, "\n");
            _exit(2);
        }
        if (connection != 0 && connection != 1 && connection != 2) {
            close(connection);
        }
        arguments[0] = "/bin/rsh";
        arguments[1] = "-c";
        arguments[2] = "echo ID1-STDOUT; echo ID2-STDERR 1>&2";
        arguments[3] = NULL;
        execv("/bin/rsh", arguments);
        say_value("socket-write-identity: shell exec errno=", errno, "\n");
        _exit(3);
    }
    if (waitpid(child, &status, 0) != child) {
        say_value("socket-write-identity: shell waitpid errno=", errno, "\n");
        return 1;
    }
    say_value("socket-write-identity: shell status=",
              WIFEXITED(status) ? WEXITSTATUS(status) : 255, "\n");
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return 1;
    }

    child = fork();
    if (child < 0) {
        say_value("socket-write-identity: fork errno=", errno, "\n");
        return 1;
    }
    if (child == 0) {
        char descriptor[8];
        char *arguments[3];

        close(listener);
        if (dup2(connection, CHILD_FD) < 0) {
            say_value("socket-write-identity: dup2 fd7 errno=", errno, "\n");
            _exit(2);
        }
        if (dup2(CHILD_FD, 0) < 0 || dup2(CHILD_FD, 1) < 0 ||
            dup2(CHILD_FD, 2) < 0) {
            say_value("socket-write-identity: dup2 stdio errno=", errno, "\n");
            _exit(3);
        }
        if (connection != CHILD_FD && connection != 0 && connection != 1 &&
            connection != 2) {
            close(connection);
        }
        snprintf(descriptor, sizeof(descriptor), "%d", CHILD_FD);
        arguments[0] = (char *)CHILD_PATH;
        arguments[1] = descriptor;
        arguments[2] = NULL;
        execv(CHILD_PATH, arguments);
        say_value("socket-write-identity: exec errno=", errno, "\n");
        _exit(4);
    }

    if (waitpid(child, &status, 0) != child) {
        say_value("socket-write-identity: waitpid errno=", errno, "\n");
        return 1;
    }
    say_value("socket-write-identity: child status=",
              WIFEXITED(status) ? WEXITSTATUS(status) : 255, "\n");
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return 1;
    }

    if (write_all(connection, "ID-PARENT", 9) != 0) {
        say_value("socket-write-identity: parent write errno=", errno, "\n");
        return 1;
    }
    say("socket-write-identity: parent sent\n");

    {
        char done[4];
        if (read_exact(connection, done, sizeof(done)) != 0 ||
            memcmp(done, "DONE", sizeof(done)) != 0) {
            say("socket-write-identity: host done missing\n");
            return 1;
        }
    }
    say("socket-write-identity: done\n");
    close(connection);
    close(listener);
    return 0;
}
