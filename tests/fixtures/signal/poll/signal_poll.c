// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S44 poll-interrupt fixture. A child installs a kernel C19 SIGUSR1 handler, tells the parent it
// is about to poll, and blocks in poll(2) on an empty pipe. The parent sends the signal. The only
// correct result is poll returning -1/EINTR and the handler having run; a lost kernel kick re-arms
// the wait and times out. Test-only; rows use DEBUG_WRITE so they reach the serial console.

#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define RAW_DEBUG_WRITE 0x03L
#define RAW_PROC_SIGNAL 0x4AL
#define POLL_TIMEOUT_MS 3000

static volatile sig_atomic_t handler_count;
static volatile sig_atomic_t handler_sig;

static long raw3(long number, long first, long second, long third) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(first), "S"(second), "d"(third)
                     : "rcx", "r11", "memory");
    return result;
}

static void say_text(const char *text) {
    if (text != NULL && *text != '\0') {
        (void)raw3(RAW_DEBUG_WRITE, (long)text, (long)strlen(text), 0);
    }
}

static void sayf(const char *format, ...) {
    char buffer[192];
    va_list arguments;
    int length;

    va_start(arguments, format);
    length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    if (length >= 0) {
        say_text(buffer);
    }
}

static int raw_proc_signal(int pid, int signo) {
    long result = raw3(RAW_PROC_SIGNAL, pid, signo, 0);
    if (result < 0) {
        errno = (int)-result;
        return -1;
    }
    return 0;
}

static void handler(int signo, siginfo_t *info, void *context) {
    (void)context;
    handler_count++;
    handler_sig = signo;
    if (info == NULL || info->si_signo != signo) {
        handler_sig = -1;
    }
}

static int fail(const char *row, long long value, int error) {
    sayf("signal-poll: fail %s value=%lld errno=%d\n", row, value, error);
    return 1;
}

static int child_main(int ready_fd, int poll_fd) {
    struct sigaction action;
    struct pollfd waiting;
    char ready = 'R';
    int result;
    int error;

    memset(&action, 0, sizeof(action));
    action.sa_sigaction = handler;
    sigemptyset(&action.sa_mask);
    action.sa_flags = SA_SIGINFO;
    if (sigaction(SIGUSR1, &action, NULL) != 0) {
        return fail("sigaction", 0, errno);
    }
    if (write(ready_fd, &ready, 1) != 1) {
        return fail("ready", 0, errno);
    }
    waiting.fd = poll_fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    errno = 0;
    result = poll(&waiting, 1, POLL_TIMEOUT_MS);
    error = errno;
    if (result != -1 || error != EINTR) {
        return fail("poll", result, error);
    }
    if (handler_count != 1 || handler_sig != SIGUSR1) {
        return fail("handler", (long long)handler_count, handler_sig);
    }
    say_text("signal-poll: poll EINTR\n");
    say_text("signal-poll: handler ok\n");
    return 0;
}

int main(void) {
    int ready_pipe[2];
    int data_pipe[2];
    char ready = 0;
    pid_t child;
    int status = 0;

    if (pipe(ready_pipe) != 0 || pipe(data_pipe) != 0) {
        return fail("pipe", 0, errno);
    }
    child = fork();
    if (child == 0) {
        (void)close(ready_pipe[0]);
        (void)close(data_pipe[1]);
        _exit(child_main(ready_pipe[1], data_pipe[0]));
    }
    if (child < 0) {
        return fail("fork", 0, errno);
    }
    (void)close(ready_pipe[1]);
    (void)close(data_pipe[0]);
    if (read(ready_pipe[0], &ready, 1) != 1 || ready != 'R') {
        return fail("ready-read", ready, errno);
    }
    /* Bounded enrolment delay: the child wrote ready just before poll(2), so give it time to
     * enter the kernel wait. A signal before the wait runs the handler early and poll times out,
     * which is a different ordering than this row measures. */
    {
        struct timespec delay = { 0, 100000000 };
        (void)nanosleep(&delay, NULL);
    }
    if (raw_proc_signal(child, SIGUSR1) != 0) {
        return fail("signal", 0, errno);
    }
    if (waitpid(child, &status, 0) != child) {
        return fail("wait", 0, errno);
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return fail("child", status, 0);
    }
    say_text("signal-poll: ok\n");
    return 0;
}
