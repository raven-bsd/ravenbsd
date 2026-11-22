// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K408 in-guest isolation of the post-exec socket write path. The guest accepts one TCP
// connection and writes a four/six-byte marker through each variant in order:
//
//   1. same-process write                       -> "SAME"
//   2. write from a forked child                -> "FORK"
//   3. write from a fork+exec child (fd 7)      -> "EXEC"
//   4. write after a read on the accepted fd    -> "READ"
//   5. second write after the host acknowledges -> "SECOND"
//
// The host peer reads the markers, sends the READ/ACK2/DONE triggers, and the guest logs each
// step on the serial console. A variant that does not reach the host leaves its marker missing.

#include <arpa/inet.h>
#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <netinet/in.h>
#include <unistd.h>

#define HOST_IP "10.0.2.2"
#define PORT 23096
#define CHILD_PATH "/bin/socket-write-exec-child"
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

static ssize_t read_full(int fd, unsigned char *out, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t got = read(fd, out + done, length - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            return (ssize_t)done;
        }
        done += (size_t)got;
    }
    return (ssize_t)done;
}

static ssize_t write_all(int fd, const unsigned char *data, size_t length) {
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
    return (ssize_t)done;
}

static int write_marker(int fd, const char *marker) {
    size_t length = strlen(marker);
    ssize_t wrote = write_all(fd, (const unsigned char *)marker, length);

    if (wrote != (ssize_t)length) {
        say_value("socket-write-exec: write failed fd=", fd, "");
        say_value(" errno=", errno, "\n");
        return -1;
    }
    return 0;
}

static int wait_for_child(pid_t pid, const char *name) {
    int status = 0;
    pid_t done;

    do {
        done = waitpid(pid, &status, 0);
    } while (done < 0 && errno == EINTR);
    if (done != pid) {
        say_value("socket-write-exec: wait failed errno=", errno, "\n");
        return -1;
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        say_value("socket-write-exec: ", 0, "");
        say(name);
        say_value(" child status=", status, "\n");
        return -1;
    }
    return 0;
}

int main(int argc, char **argv) {
    struct sockaddr_in address;
    int connection;
    int attempt;
    int accept_mode;
    unsigned char trigger[8];

    accept_mode = argc > 1 && strcmp(argv[1], "--accept") == 0;
    connection = socket(AF_INET, SOCK_STREAM, 0);
    if (connection < 0) {
        say_value("socket-write-exec: socket errno=", errno, "\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    if (accept_mode) {
        // K195 shape: the guest owns the listener and the host connects over the tap.
        int listener = connection;
        int accepted;

        address.sin_addr.s_addr = htonl(0);
        if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
            say_value("socket-write-exec: bind errno=", errno, "\n");
            return 1;
        }
        if (listen(listener, 8) < 0) {
            say_value("socket-write-exec: listen errno=", errno, "\n");
            return 1;
        }
        say("socket-write-exec: listening port=23096\n");
        for (;;) {
            accepted = accept(listener, NULL, NULL);
            if (accepted >= 0) {
                break;
            }
            if (errno != EINTR) {
                say_value("socket-write-exec: accept errno=", errno, "\n");
                return 1;
            }
        }
        connection = accepted;
        say_value("socket-write-exec: accepted fd=", connection, "\n");
    } else {
        if (inet_pton(AF_INET, HOST_IP, &address.sin_addr) != 1) {
            say("socket-write-exec: bad host address\n");
            return 1;
        }
        for (attempt = 0; attempt < 100; attempt++) {
            if (connect(connection, (const struct sockaddr *)&address, sizeof(address)) == 0) {
                break;
            }
            usleep(100000);
        }
        if (attempt == 100) {
            say_value("socket-write-exec: connect errno=", errno, "\n");
            return 1;
        }
        say_value("socket-write-exec: connected fd=", connection, "\n");
    }

    // 1. Same-process write on the accepted socket.
    if (write_marker(connection, "SAME") != 0) {
        return 1;
    }
    say("socket-write-exec: same sent\n");

    // 2. Write from a forked child that inherits the fd.
    {
        pid_t pid = fork();

        if (pid < 0) {
            say_value("socket-write-exec: fork errno=", errno, "\n");
            return 1;
        }
        if (pid == 0) {
            _exit(write_marker(connection, "FORK") == 0 ? 0 : 1);
        }
        if (wait_for_child(pid, "fork") != 0) {
            return 1;
        }
    }
    say("socket-write-exec: fork sent\n");

    // 3. Write from a fork+exec child that receives the accepted socket as fd 7.
    {
        pid_t pid = fork();

        if (pid < 0) {
            say_value("socket-write-exec: exec fork errno=", errno, "\n");
            return 1;
        }
        if (pid == 0) {
            char descriptor[8];
            char *argv[3];

            if (dup2(connection, CHILD_FD) < 0) {
                _exit(2);
            }
            if (connection != CHILD_FD) {
                close(connection);
            }
            snprintf(descriptor, sizeof(descriptor), "%d", CHILD_FD);
            argv[0] = (char *)CHILD_PATH;
            argv[1] = descriptor;
            argv[2] = NULL;
            execv(CHILD_PATH, argv);
            _exit(3);
        }
        if (wait_for_child(pid, "exec") != 0) {
            return 1;
        }
    }
    say("socket-write-exec: exec sent\n");

    // 4. Read a trigger on the accepted socket, then write on the same fd.
    if (read_full(connection, trigger, 4) != 4 || memcmp(trigger, "READ", 4) != 0) {
        say("socket-write-exec: read trigger missing\n");
        return 1;
    }
    say("socket-write-exec: read request\n");
    if (write_marker(connection, "READ") != 0) {
        return 1;
    }
    say("socket-write-exec: read sent\n");

    // 5. Second write only after the host acknowledges the earlier markers.
    if (read_full(connection, trigger, 4) != 4 || memcmp(trigger, "ACK2", 4) != 0) {
        say("socket-write-exec: second trigger missing\n");
        return 1;
    }
    if (write_marker(connection, "SECOND") != 0) {
        return 1;
    }
    say("socket-write-exec: second sent\n");

    if (read_full(connection, trigger, 4) != 4 || memcmp(trigger, "DONE", 4) != 0) {
        say("socket-write-exec: done trigger missing\n");
        return 1;
    }
    say("socket-write-exec: host done\n");
    say("socket-write-exec: done\n");
    return 0;
}
