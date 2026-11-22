// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K443: SIGCHLD delivery while blocked in ppoll, self-pipe write, blocking read and waitpid
// reaping. The binary is /init on the base profile and prints one CONTRACT row per check.
//
// The children sleep before they exit so the parent is already inside the blocking call when
// the signal is queued. The ppoll rows therefore distinguish "handler ran after the signal
// interrupted the wait" from "the pipe happened to be ready before the call".

#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

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

static void say_line(const char *id, int ok, const char *detail) {
    char line[240];
    int length = snprintf(line, sizeof(line), "CONTRACT %s %s %s\n", id, ok ? "PASS" : "FAIL",
                          detail);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int signal_pipe = -1;
static volatile sig_atomic_t signal_count;

static void sigchld_handler(int signal_number) {
    (void)signal_number;
    int saved = errno;
    signal_count++;
    if (signal_pipe >= 0) {
        char byte = 1;
        ssize_t ignored = write(signal_pipe, &byte, 1);
        (void)ignored;
    }
    errno = saved;
}

static int install_sigchld(int restart) {
    struct sigaction action;

    memset(&action, 0, sizeof(action));
    action.sa_handler = sigchld_handler;
    sigemptyset(&action.sa_mask);
    action.sa_flags = restart ? SA_RESTART : 0;
    return sigaction(SIGCHLD, &action, NULL);
}

static void sleep_ms(long milliseconds) {
    struct timespec delay;
    delay.tv_sec = milliseconds / 1000;
    delay.tv_nsec = (milliseconds % 1000) * 1000000L;
    while (nanosleep(&delay, &delay) != 0 && errno == EINTR) {
    }
}

static int fork_exit_after(int code, long delay_ms) {
    pid_t child = fork();
    if (child == 0) {
        sleep_ms(delay_ms);
        _exit(code);
    }
    return child < 0 ? -1 : (int)child;
}

// One ppoll on the idle socket entry and the self-pipe. Returns the raw result; on a signal
// the caller sees -1/EINTR and the handler has already written the self-pipe byte.
static int ppoll_once(int pipe_read, int timeout_ms, int *saved_errno) {
    struct pollfd fds[2];
    struct timespec timeout;

    fds[0].fd = -1; // the idle socket is represented by a negative fd
    fds[0].events = 0;
    fds[0].revents = 0;
    fds[1].fd = pipe_read;
    fds[1].events = POLLIN;
    fds[1].revents = 0;
    timeout.tv_sec = timeout_ms / 1000;
    timeout.tv_nsec = (timeout_ms % 1000) * 1000000L;
    errno = 0;
    int ready = ppoll(fds, 2, &timeout, NULL);
    *saved_errno = errno;
    return ready;
}

static ssize_t read_self_pipe(int pipe_read, char *byte) {
    ssize_t count;
    do {
        count = read(pipe_read, byte, 1);
    } while (count < 0 && errno == EINTR);
    return count;
}

// A second SIGCHLD can interrupt the waitpid call itself; POSIX says retry.
static pid_t wait_child(pid_t pid, int *status) {
    pid_t result;
    do {
        result = waitpid(pid, status, 0);
    } while (result < 0 && errno == EINTR);
    return result;
}

static int row_one(int restart, const char *id) {
    int pipe_fds[2];
    int child;
    int status = 0;
    int ok = 1;
    int first_errno = 0;
    int ready;
    char detail[200];
    char byte = 0;
    ssize_t count = -1;

    signal_count = 0;
    if (pipe(pipe_fds) != 0) {
        say_line(id, 0, "pipe failed");
        return 1;
    }
    signal_pipe = pipe_fds[1];
    if (install_sigchld(restart) != 0) {
        say_line(id, 0, "sigaction failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    child = fork_exit_after(7, 500);
    if (child < 0) {
        say_line(id, 0, "fork failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    ready = ppoll_once(pipe_fds[0], 5000, &first_errno);
    if (ready != -1 || first_errno != EINTR) {
        snprintf(detail, sizeof(detail), "ppoll=%d errno=%d not an interrupting signal", ready,
                 first_errno);
        ok = 0;
    } else {
        count = read_self_pipe(pipe_fds[0], &byte);
        if (count != 1 || byte != 1) {
            snprintf(detail, sizeof(detail), "self-pipe read=%ld errno=%d", (long)count, errno);
            ok = 0;
        } else if (wait_child(child, &status) != child || !WIFEXITED(status) ||
                   WEXITSTATUS(status) != 7) {
            snprintf(detail, sizeof(detail), "waitpid status=%d", status);
            ok = 0;
        } else {
            snprintf(detail, sizeof(detail),
                     "ppoll=EINTR read=1 waitpid=7 restart=%d signals=%d", restart,
                     (int)signal_count);
        }
    }
    close(pipe_fds[0]);
    close(pipe_fds[1]);
    if (child >= 0 && !WIFEXITED(status)) {
        (void)kill(child, SIGKILL);
        (void)waitpid(child, &status, WNOHANG);
    }
    say_line(id, ok, detail);
    return ok ? 0 : 1;
}

static int row_blocking_read(void) {
    int pipe_fds[2];
    int child;
    int status = 0;
    int ok = 1;
    char detail[200];
    ssize_t count = -1;
    int first_errno = 0;
    char byte = 0;

    signal_count = 0;
    if (pipe(pipe_fds) != 0) {
        say_line("K13", 0, "pipe failed");
        return 1;
    }
    signal_pipe = pipe_fds[1];
    if (install_sigchld(1) != 0) {
        say_line("K13", 0, "sigaction failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    child = fork_exit_after(7, 500);
    if (child < 0) {
        say_line("K13", 0, "fork failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    // The pipe is empty here. A blocking read must sleep in the kernel/libc wait, not return
    // EAGAIN; the SIGCHLD handler writes the byte and the read either restarts (SA_RESTART) or
    // is retried once by this loop.
    errno = 0;
    count = read(pipe_fds[0], &byte, 1);
    first_errno = errno;
    while (count < 0 && errno == EINTR) {
        errno = 0;
        count = read(pipe_fds[0], &byte, 1);
    }
    if (first_errno == EAGAIN) {
        snprintf(detail, sizeof(detail), "blocking read=EAGAIN");
        ok = 0;
    } else if (count != 1 || byte != 1) {
        snprintf(detail, sizeof(detail), "blocking read=%ld errno=%d first=%d", (long)count,
                 errno, first_errno);
        ok = 0;
    } else if (wait_child(child, &status) != child || !WIFEXITED(status) ||
               WEXITSTATUS(status) != 7) {
        snprintf(detail, sizeof(detail), "waitpid status=%d", status);
        ok = 0;
    } else {
        snprintf(detail, sizeof(detail),
                 "blocking read=1 first_errno=%d waitpid=7 signals=%d", first_errno,
                 (int)signal_count);
    }
    close(pipe_fds[0]);
    close(pipe_fds[1]);
    if (child >= 0 && !WIFEXITED(status)) {
        (void)kill(child, SIGKILL);
        (void)waitpid(child, &status, WNOHANG);
    }
    say_line("K13", ok, detail);
    return ok ? 0 : 1;
}

static int row_two_children(void) {
    int pipe_fds[2];
    int first;
    int second;
    int status = 0;
    int reaped = 0;
    int ok = 1;
    int read_count = 0;
    int got_first = 0;
    int got_second = 0;
    int first_errno = 0;
    int wait_errno = 0;
    int ready;
    char detail[240];
    char bytes[4];

    signal_count = 0;
    if (pipe(pipe_fds) != 0) {
        say_line("K14", 0, "pipe failed");
        return 1;
    }
    signal_pipe = pipe_fds[1];
    if (install_sigchld(1) != 0) {
        say_line("K14", 0, "sigaction failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    first = fork_exit_after(7, 500);
    second = fork_exit_after(8, 600);
    if (first < 0 || second < 0) {
        say_line("K14", 0, "fork failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    ready = ppoll_once(pipe_fds[0], 5000, &first_errno);
    if (ready != -1 || first_errno != EINTR) {
        snprintf(detail, sizeof(detail), "ppoll=%d errno=%d not an interrupting signal", ready,
                 first_errno);
        ok = 0;
    } else {
        // One byte proves the wake; the waitpid loop must still reap both children. Two
        // children must produce two SIGCHLD records and never more than one per child.
        read_count = (int)read_self_pipe(pipe_fds[0], bytes);
        while (reaped < 2) {
            pid_t pid = waitpid(-1, &status, 0);
            if (pid < 0) {
                wait_errno = errno;
                if (errno == EINTR) {
                    continue;
                }
                break;
            }
            if (pid == first && WIFEXITED(status) && WEXITSTATUS(status) == 7) {
                got_first = 1;
            }
            if (pid == second && WIFEXITED(status) && WEXITSTATUS(status) == 8) {
                got_second = 1;
            }
            reaped++;
        }
        // A waitpid that consumed the exit before the kick returned can leave the second
        // SIGCHLD entry pending; let it reach the handler before declaring it lost.
        for (int settle = 0; settle < 100 && signal_count < 2; settle++) {
            sleep_ms(10);
        }
        if (read_count < 1 || reaped != 2 || !got_first || !got_second ||
            signal_count != 2) {
            snprintf(detail, sizeof(detail),
                     "read=%d reaped=%d first=%d second=%d signals=%d wait_errno=%d",
                     read_count, reaped, got_first, got_second, (int)signal_count, wait_errno);
            ok = 0;
        } else {
            snprintf(detail, sizeof(detail),
                     "ppoll=EINTR read=%d reaped=2 statuses=7/8 signals=2", read_count);
        }
    }
    close(pipe_fds[0]);
    close(pipe_fds[1]);
    say_line("K14", ok, detail);
    return ok ? 0 : 1;
}

int main(void) {
    int failures = 0;

    failures += row_one(0, "K11");
    failures += row_one(1, "K12");
    failures += row_blocking_read();
    failures += row_two_children();
    if (failures != 0) {
        say("CONTRACT SUMMARY FAIL\n");
        return 1;
    }
    say("CONTRACT SUMMARY PASS 4/4\n");
    return 0;
}
