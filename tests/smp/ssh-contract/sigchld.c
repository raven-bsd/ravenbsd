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
#include <pthread.h>
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
static int sibling_pid = -1;

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

// K15: a SIGCHLD queued while blocked must stay queued until the mask opens. The old kernel
// pop made the signal disappear inside the entry path; the row sees signal_count == 0.
static int row_masked_sigchld(const char *id) {
    int pipe_fds[2];
    int child;
    int status = 0;
    int ok = 1;
    int before = 0;
    int blocked = 0;
    int still_blocked = 0;
    int settle;
    ssize_t count = -1;
    char detail[240];
    char byte = 0;
    sigset_t set;
    sigset_t current;

    signal_count = 0;
    if (pipe(pipe_fds) != 0) {
        say_line(id, 0, "pipe failed");
        return 1;
    }
    signal_pipe = pipe_fds[1];
    if (install_sigchld(1) != 0) {
        say_line(id, 0, "sigaction failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    sigemptyset(&set);
    sigaddset(&set, SIGCHLD);
    if (sigprocmask(SIG_BLOCK, &set, NULL) != 0) {
        say_line(id, 0, "block failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    current = set;
    if (sigprocmask(SIG_BLOCK, NULL, &current) != 0) {
        say_line(id, 0, "mask read failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    blocked = sigismember(&current, SIGCHLD);
    child = fork_exit_after(7, 100);
    if (child < 0) {
        say_line(id, 0, "fork failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    // Let the child exit and the signal be queued while the handler is blocked. Several short
    // sleeps force syscall returns, which is where the old entry path discarded the signal.
    for (settle = 0; settle < 40; settle++) {
        sleep_ms(10);
    }
    current = set;
    if (sigprocmask(SIG_BLOCK, NULL, &current) == 0) {
        still_blocked = sigismember(&current, SIGCHLD);
    }
    before = (int)signal_count;
    if (sigprocmask(SIG_UNBLOCK, &set, NULL) != 0) {
        snprintf(detail, sizeof(detail), "unblock failed errno=%d", errno);
        ok = 0;
    } else {
        for (settle = 0; settle < 200 && signal_count < 1; settle++) {
            sleep_ms(5);
        }
        if (signal_count == 1) {
            count = read_self_pipe(pipe_fds[0], &byte);
        }
        if (before != 0) {
            snprintf(detail, sizeof(detail), "delivered while blocked signals=%d", before);
            ok = 0;
        } else if (signal_count != 1 || count != 1 || byte != 1) {
            snprintf(detail, sizeof(detail),
                     "lost masked signal blocked=%d still=%d signals=%d read=%ld", blocked,
                     still_blocked, (int)signal_count, (long)count);
            ok = 0;
        } else if (wait_child(child, &status) != child || !WIFEXITED(status) ||
                   WEXITSTATUS(status) != 7) {
            snprintf(detail, sizeof(detail), "waitpid status=%d", status);
            ok = 0;
        } else {
            snprintf(detail, sizeof(detail), "blocked=0 unblocked=1 waitpid=7 signals=1");
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

// K16: two SIGCHLD records queued while blocked must both survive the unblock.
static int row_two_masked(void) {
    int pipe_fds[2];
    int first;
    int second;
    int status = 0;
    int reaped = 0;
    int got_first = 0;
    int got_second = 0;
    int ok = 1;
    int settle;
    ssize_t count = -1;
    char detail[240];
    char bytes[4];
    sigset_t set;

    signal_count = 0;
    if (pipe(pipe_fds) != 0) {
        say_line("K16", 0, "pipe failed");
        return 1;
    }
    signal_pipe = pipe_fds[1];
    if (install_sigchld(1) != 0) {
        say_line("K16", 0, "sigaction failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    sigemptyset(&set);
    sigaddset(&set, SIGCHLD);
    if (sigprocmask(SIG_BLOCK, &set, NULL) != 0) {
        say_line("K16", 0, "block failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    first = fork_exit_after(7, 100);
    second = fork_exit_after(8, 150);
    if (first < 0 || second < 0) {
        say_line("K16", 0, "fork failed");
        close(pipe_fds[0]);
        close(pipe_fds[1]);
        return 1;
    }
    for (settle = 0; settle < 50; settle++) {
        sleep_ms(10);
    }
    if (sigprocmask(SIG_UNBLOCK, &set, NULL) != 0) {
        snprintf(detail, sizeof(detail), "unblock failed errno=%d", errno);
        ok = 0;
    } else {
        for (settle = 0; settle < 300 && signal_count < 2; settle++) {
            sleep_ms(5);
        }
        if (signal_count > 0) {
            count = read_self_pipe(pipe_fds[0], bytes);
        }
        while (reaped < 2) {
            pid_t pid = waitpid(-1, &status, 0);
            if (pid < 0) {
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
        // `read_self_pipe` reads one byte by design; the two handler runs are proven by
        // `signal_count == 2` above the reap.
        if (signal_count != 2 || count < 1 || reaped != 2 || !got_first || !got_second) {
            snprintf(detail, sizeof(detail),
                     "signals=%d read=%ld reaped=%d first=%d second=%d", (int)signal_count,
                     (long)count, reaped, got_first, got_second);
            ok = 0;
        } else {
            snprintf(detail, sizeof(detail), "blocked=2 unblocked=2 signals=2 statuses=7/8");
        }
    }
    close(pipe_fds[0]);
    close(pipe_fds[1]);
    say_line("K16", ok, detail);
    return ok ? 0 : 1;
}

// K17: sigqueue must carry si_value, si_code == SI_QUEUE and the sender pid.
static volatile sig_atomic_t info_count;
static volatile sig_atomic_t info_code;
static volatile sig_atomic_t info_pid;
static volatile int info_value;

static void sigusr1_info(int signal_number, siginfo_t *info, void *context) {
    (void)context;
    int saved = errno;
    info_count++;
    info_code = info->si_code;
    info_pid = info->si_pid;
    info_value = info->si_value.sival_int;
    errno = saved;
    (void)signal_number;
}

static int row_sigqueue_info(void) {
    struct sigaction action;
    union sigval value;
    int rc;
    int settle;
    int ok = 1;
    char detail[240];
    pid_t self = getpid();

    info_count = 0;
    info_code = 0;
    info_pid = 0;
    info_value = 0;
    memset(&action, 0, sizeof(action));
    action.sa_sigaction = sigusr1_info;
    sigemptyset(&action.sa_mask);
    action.sa_flags = SA_SIGINFO;
    if (sigaction(SIGUSR1, &action, NULL) != 0) {
        say_line("K17", 0, "sigaction failed");
        return 1;
    }
    value.sival_int = 0x5a5a;
    errno = 0;
    rc = sigqueue(self, SIGUSR1, value);
    if (rc != 0) {
        snprintf(detail, sizeof(detail), "sigqueue=%d errno=%d", rc, errno);
        ok = 0;
    } else {
        for (settle = 0; settle < 200 && info_count < 1; settle++) {
            sleep_ms(5);
        }
        if (info_count != 1 || info_value != 0x5a5a || info_code != SI_QUEUE ||
            info_pid != (int)self) {
            snprintf(detail, sizeof(detail), "count=%d value=0x%x code=%d pid=%d self=%d",
                     (int)info_count, info_value, (int)info_code, (int)info_pid, (int)self);
            ok = 0;
        } else {
            snprintf(detail, sizeof(detail), "value=0x%x code=SI_QUEUE pid=%d", info_value,
                     (int)info_pid);
        }
    }
    say_line("K17", ok, detail);
    return ok ? 0 : 1;
}

// K18: a secondary thread gets its own context area and its own entry registration; a signal
// blocked, raised and unblocked there must run on that thread.
static volatile sig_atomic_t thread_signals;
static volatile unsigned long handler_thread;
static volatile unsigned long target_thread;

static void thread_sigusr1(int signal_number) {
    int saved = errno;
    thread_signals++;
    handler_thread = (unsigned long)pthread_self();
    errno = saved;
    (void)signal_number;
}

static void *thread_signal_body(void *argument) {
    sigset_t set;
    int settle;

    (void)argument;
    sigemptyset(&set);
    sigaddset(&set, SIGUSR1);
    if (pthread_sigmask(SIG_BLOCK, &set, NULL) != 0) {
        return (void *)1;
    }
    target_thread = (unsigned long)pthread_self();
    if (raise(SIGUSR1) != 0) {
        return (void *)1;
    }
    sleep_ms(20);
    if (pthread_sigmask(SIG_UNBLOCK, &set, NULL) != 0) {
        return (void *)1;
    }
    for (settle = 0; settle < 200 && thread_signals == 0; settle++) {
        sleep_ms(5);
    }
    return thread_signals == 0 ? (void *)1 : (void *)0;
}

static int row_secondary_thread(void) {
    struct sigaction action;
    pthread_t thread;
    void *result = (void *)1;
    int ok = 1;
    char detail[240];

    thread_signals = 0;
    handler_thread = 0;
    target_thread = 0;
    memset(&action, 0, sizeof(action));
    action.sa_handler = thread_sigusr1;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sigaction(SIGUSR1, &action, NULL) != 0) {
        say_line("K18", 0, "sigaction failed");
        return 1;
    }
    if (pthread_create(&thread, NULL, thread_signal_body, NULL) != 0) {
        say_line("K18", 0, "pthread_create failed");
        return 1;
    }
    if (pthread_join(thread, &result) != 0) {
        snprintf(detail, sizeof(detail), "pthread_join failed");
        ok = 0;
    } else if (result != (void *)0) {
        snprintf(detail, sizeof(detail), "thread body did not see the signal");
        ok = 0;
    } else if (thread_signals != 1 || handler_thread == 0 || handler_thread != target_thread) {
        snprintf(detail, sizeof(detail), "signals=%d handler=%lu target=%lu",
                 (int)thread_signals, handler_thread, target_thread);
        ok = 0;
    } else {
        snprintf(detail, sizeof(detail), "signals=1 delivered-on-target-thread");
    }
    say_line("K18", ok, detail);
    return ok ? 0 : 1;
}

static int row_kill_child(void) {
    int status = 0;
    int result = -1;
    int kill_errno = 0;
    int reaped = 0;
    int polls;
    char detail[220];
    pid_t child = fork();

    if (child < 0) {
        say_line("K19", 0, "fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            pause();
        }
    }
    sleep_ms(50);
    errno = 0;
    result = kill(child, SIGTERM);
    kill_errno = errno;
    if (result != 0) {
        snprintf(detail, sizeof(detail), "kill failed errno=%d", kill_errno);
        (void)kill(child, SIGKILL);
        (void)waitpid(child, &status, 0);
        say_line("K19", 0, detail);
        return 1;
    }
    for (polls = 0; polls < 200; polls++) {
        reaped = (int)waitpid(child, &status, WNOHANG);
        if (reaped == (int)child || (reaped < 0 && errno != EINTR)) {
            break;
        }
        sleep_ms(10);
    }
    if (reaped != (int)child || !WIFSIGNALED(status) || WTERMSIG(status) != SIGTERM) {
        snprintf(detail, sizeof(detail), "waitpid=%d status=%d signaled=%d sig=%d", reaped, status,
                 WIFSIGNALED(status), WTERMSIG(status));
        (void)kill(child, SIGKILL);
        (void)waitpid(child, &status, 0);
        say_line("K19", 0, detail);
        return 1;
    }
    snprintf(detail, sizeof(detail), "kill=SIGTERM waitpid=%d status=%d sig=%d", reaped, status,
             WTERMSIG(status));
    say_line("K19", 1, detail);
    return 0;
}

static int row_sibling_kill(void) {
    int status = 0;
    int killer_status = 0;
    int denied_status = 0;
    int reaped = 0;
    int polls;
    int result = -1;
    int kill_errno = 0;
    char detail[240];
    pid_t sibling = fork();

    if (sibling < 0) {
        say_line("K20", 0, "sibling fork failed");
        return 1;
    }
    if (sibling == 0) {
        for (;;) {
            pause();
        }
    }
    sibling_pid = (int)sibling;

    pid_t killer = fork();
    if (killer < 0) {
        (void)kill(sibling, SIGKILL);
        (void)waitpid(sibling, &status, 0);
        say_line("K20", 0, "killer fork failed");
        return 1;
    }
    if (killer == 0) {
        sleep_ms(50);
        errno = 0;
        result = kill(sibling_pid, SIGTERM);
        _exit(result == 0 ? 0 : 2);
    }

    // The killer is a sibling of the target, not its child. The parent reaps the target and
    // requires the signal status.
    for (polls = 0; polls < 200; polls++) {
        reaped = (int)waitpid(sibling, &status, WNOHANG);
        if (reaped == (int)sibling || (reaped < 0 && errno != EINTR)) {
            break;
        }
        sleep_ms(10);
    }
    if (reaped != (int)sibling || !WIFSIGNALED(status) || WTERMSIG(status) != SIGTERM) {
        snprintf(detail, sizeof(detail), "sibling waitpid=%d status=%d signaled=%d sig=%d", reaped,
                 status, WIFSIGNALED(status), WTERMSIG(status));
        (void)kill(sibling, SIGKILL);
        (void)waitpid(sibling, &status, 0);
        (void)kill(killer, SIGKILL);
        (void)waitpid(killer, &killer_status, 0);
        say_line("K20", 0, detail);
        return 1;
    }
    if (wait_child(killer, &killer_status) != killer || !WIFEXITED(killer_status) ||
        WEXITSTATUS(killer_status) != 0) {
        snprintf(detail, sizeof(detail), "killer waitpid=%d status=%d", (int)killer,
                 killer_status);
        say_line("K20", 0, detail);
        return 1;
    }

    // A different-uid child must be refused: the child becomes uid 1 and kills the root parent.
    pid_t denied = fork();
    if (denied == 0) {
        if (setuid(1) != 0) {
            _exit(3);
        }
        errno = 0;
        result = kill(getppid(), SIGTERM);
        kill_errno = errno;
        _exit(result != 0 && kill_errno == EPERM ? 0 : 4);
    }
    if (denied < 0 || wait_child(denied, &denied_status) != denied ||
        !WIFEXITED(denied_status) || WEXITSTATUS(denied_status) != 0) {
        snprintf(detail, sizeof(detail), "denied waitpid=%d status=%d", (int)denied, denied_status);
        say_line("K20", 0, detail);
        return 1;
    }
    snprintf(detail, sizeof(detail), "sibling=SIGTERM status=%d eperm=1", status);
    say_line("K20", 1, detail);
    return 0;
}

int main(void) {
    int failures = 0;

    failures += row_one(0, "K11");
    failures += row_one(1, "K12");
    failures += row_blocking_read();
    failures += row_two_children();
    failures += row_masked_sigchld("K15");
    failures += row_two_masked();
    failures += row_sigqueue_info();
    failures += row_secondary_thread();
    failures += row_kill_child();
    failures += row_sibling_kill();
    if (failures != 0) {
        say("CONTRACT SUMMARY FAIL\n");
        return 1;
    }
    say("CONTRACT SUMMARY PASS 10/10\n");
    return 0;
}
