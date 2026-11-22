/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K443: SIGCHLD delivery while the parent is blocked in ppoll. The handler is the OpenSSH
 * self-pipe pattern: it writes one byte and returns. The parent must observe the byte, reap the
 * exact child status, see one handler per child, and a blocking read on the already-written
 * self-pipe must return the byte, not EAGAIN. */

#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int sigchld_pipe[2];
static volatile sig_atomic_t sigchld_count;

static void sigchld_handler(int signo) {
    int saved = errno;
    unsigned char byte = 1;
    (void)signo;
    (void)write(sigchld_pipe[1], &byte, 1);
    sigchld_count = sigchld_count + 1;
    errno = saved;
}

static int fail(const char *what) {
    printf("sigchld-ppoll: FAIL %s errno=%d\n", what, errno);
    return 1;
}

static int install_sigchld(int flags) {
    struct sigaction action;

    if (pipe(sigchld_pipe) != 0) {
        return fail("pipe");
    }
    memset(&action, 0, sizeof(action));
    action.sa_handler = sigchld_handler;
    sigemptyset(&action.sa_mask);
    action.sa_flags = flags;
    if (sigaction(SIGCHLD, &action, NULL) != 0) {
        return fail("sigaction");
    }
    sigchld_count = 0;
    return 0;
}

static int wait_sigchld_byte(int timeout_ms, int *ready_out) {
    struct pollfd pfd;
    struct timespec timeout;
    int ready;

    pfd.fd = sigchld_pipe[0];
    pfd.events = POLLIN;
    pfd.revents = 0;
    timeout.tv_sec = timeout_ms / 1000;
    timeout.tv_nsec = (long)(timeout_ms % 1000) * 1000000L;
    do {
        ready = ppoll(&pfd, 1, &timeout, NULL);
    } while (ready < 0 && errno == EINTR);
    *ready_out = ready;
    return ready > 0 && (pfd.revents & POLLIN) != 0 ? 0 : -1;
}

static int check_ppoll(int restart) {
    pid_t child;
    int status;
    int ready = 0;
    unsigned char byte = 0;

    if (install_sigchld(restart ? SA_RESTART : 0) != 0) {
        return 1;
    }
    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        _exit(7);
    }
    if (wait_sigchld_byte(5000, &ready) != 0) {
        return fail("ppoll");
    }
    if (read(sigchld_pipe[0], &byte, 1) != 1) {
        return fail("self-pipe read");
    }
    if (waitpid(child, &status, 0) != child) {
        return fail("waitpid");
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 7) {
        return fail("status");
    }
    if (sigchld_count != 1) {
        return fail("count");
    }
    (void)close(sigchld_pipe[0]);
    (void)close(sigchld_pipe[1]);
    printf("sigchld-ppoll: %s PASS handler=1 read=1 wait=7\n",
           restart ? "SA_RESTART" : "no-SA_RESTART");
    return 0;
}

static int check_blocking_read(void) {
    pid_t child;
    int status;
    int ready = 0;
    unsigned char byte = 0;

    if (install_sigchld(0) != 0) {
        return 1;
    }
    child = fork();
    if (child < 0) {
        return fail("read fork");
    }
    if (child == 0) {
        _exit(7);
    }
    if (wait_sigchld_byte(5000, &ready) != 0) {
        return fail("read ready");
    }
    if (read(sigchld_pipe[0], &byte, 1) != 1) {
        return fail("blocking read");
    }
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 7) {
        return fail("read wait");
    }
    (void)close(sigchld_pipe[0]);
    (void)close(sigchld_pipe[1]);
    printf("sigchld-ppoll: BLOCKING PASS read=1\n");
    return 0;
}

static int check_two_children(void) {
    pid_t first;
    pid_t second;
    int status;
    int ready = 0;
    unsigned char bytes[2] = {0, 0};
    size_t got = 0;

    if (install_sigchld(0) != 0) {
        return 1;
    }
    first = fork();
    if (first < 0) {
        return fail("two fork1");
    }
    if (first == 0) {
        _exit(3);
    }
    second = fork();
    if (second < 0) {
        return fail("two fork2");
    }
    if (second == 0) {
        _exit(4);
    }
    (void)poll(NULL, 0, 200);
    for (int attempt = 0; attempt < 50 && got < 2; attempt++) {
        if (wait_sigchld_byte(100, &ready) == 0) {
            ssize_t count = read(sigchld_pipe[0], bytes + got, 2 - got);
            if (count > 0) {
                got += (size_t)count;
            }
        }
    }
    if (got != 2) {
        return fail("two bytes");
    }
    if (sigchld_count != 2) {
        return fail("two count");
    }
    if (waitpid(first, &status, 0) != first || !WIFEXITED(status) || WEXITSTATUS(status) != 3) {
        return fail("two wait1");
    }
    if (waitpid(second, &status, 0) != second || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 4) {
        return fail("two wait2");
    }
    (void)close(sigchld_pipe[0]);
    (void)close(sigchld_pipe[1]);
    printf("sigchld-ppoll: TWO PASS handler=2 statuses=3,4\n");
    return 0;
}

int main(void) {
    printf("sigchld-ppoll: start sig=%d\n", SIGCHLD);
    if (check_ppoll(0) != 0 || check_ppoll(SA_RESTART) != 0 ||
        check_blocking_read() != 0 || check_two_children() != 0) {
        printf("sigchld-ppoll: FAIL\n");
        return 1;
    }
    printf("sigchld-ppoll: ok\n");
    _exit(0);
}
