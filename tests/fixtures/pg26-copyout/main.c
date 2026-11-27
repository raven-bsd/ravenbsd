/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173c PG-26 copyout destination assertion.
 *
 * The mapped control is a valid writable `struct pollfd[1]` with fd=-1 and events=0; with no
 * registration the wait is empty, so `ppoll` returns 0. The read-only case maps the same array,
 * makes the page read-only with mprotect (the input is still readable, `revents` is not
 * writable) and requires -1/EFAULT. The concurrent case blocks on a pipe read end with a 1 s
 * timeout while a thread unmaps the destination page after 50 ms, then requires -1/EFAULT once
 * the wait times out. Both boundaries run in forked children so the parent proves the child
 * exited normally and was never killed by a page fault.
 *
 * The guest is /init and has no stdio fds; progress goes through the kernel debug console
 * (DEBUG_WRITE, 0x03), the same path the pg26-pointer fixture uses. */

#include <errno.h>
#include <poll.h>
#include <pthread.h>
#include <stddef.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define PAGE 4096
#define UNMAP_DELAY_NS (50L * 1000L * 1000L)
#define WAIT_TIMEOUT_S 1
#define WAIT_MIN_MS 900
#define WAIT_MAX_MS 5000

struct unmap_state {
    char *page;
    volatile int release;
    volatile int done;
    long long at_ms;
};

static long debug_write(const char *bytes, size_t length) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t len asm("rsi") = length;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(len)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)debug_write(text, strlen(text));
}

static void say_uint(unsigned long value) {
    char digits[20];
    size_t used = 0;

    do {
        digits[used++] = (char)('0' + (value % 10));
        value /= 10;
    } while (value != 0);
    while (used != 0) {
        --used;
        (void)debug_write(&digits[used], 1);
    }
}

static int fail(const char *what) {
    say("pg26-copyout: fail ");
    say(what);
    say("\n");
    return 1;
}

static int fail_child(const char *what, int value) {
    say("pg26-copyout: fail ");
    say(what);
    say("=");
    say_uint((unsigned long)(long)value);
    say("\n");
    return 1;
}

static long long now_ms(void) {
    struct timespec ts;

    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return -1;
    }
    return (long long)ts.tv_sec * 1000LL + (long long)ts.tv_nsec / 1000000LL;
}

static void *unmap_after_release(void *argument) {
    struct unmap_state *state = argument;
    struct timespec pause = {0, 1000L * 1000L};
    struct timespec delay = {0, UNMAP_DELAY_NS};

    while (!state->release) {
        (void)nanosleep(&pause, NULL);
    }
    (void)nanosleep(&delay, NULL);
    if (munmap(state->page, PAGE) == 0) {
        state->at_ms = now_ms();
        state->done = 1;
    } else {
        state->done = 2;
    }
    return NULL;
}

/* Mapped, writable destination with an ignored record: the wait is empty and returns 0. */
static int mapped_control(void) {
    struct pollfd control;
    int result;

    control.fd = -1;
    control.events = 0;
    control.revents = 0;
    errno = 0;
    result = ppoll(&control, 1, NULL, NULL);
    if (result != 0 || errno != 0) {
        say("pg26-copyout: fail mapped result=");
        say_uint((unsigned long)(long)result);
        say(" errno=");
        say_uint((unsigned long)errno);
        say("\n");
        return 1;
    }
    say("pg26-copyout: mapped ok\n");
    return 0;
}

static int read_only_child(void) {
    char *page;
    struct pollfd *fds;
    int result;

    page = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        return 10;
    }
    fds = (struct pollfd *)page;
    fds->fd = -1;
    fds->events = 0;
    fds->revents = 0;
    if (mprotect(page, PAGE, PROT_READ) != 0) {
        return 11;
    }
    errno = 0;
    result = ppoll(fds, 1, NULL, NULL);
    if (result != -1) {
        return 12;
    }
    if (errno != EFAULT) {
        return 13;
    }
    return 0;
}

static int unmapped_child(void) {
    struct pollfd *fds;
    struct unmap_state state;
    struct timespec timeout = {WAIT_TIMEOUT_S, 0};
    pthread_t thread;
    char *page;
    long long started;
    long long finished;
    int pipe_fds[2];
    int result;

    if (pipe(pipe_fds) != 0) {
        return 18;
    }
    page = mmap(NULL, PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        return 10;
    }
    fds = (struct pollfd *)page;
    fds->fd = pipe_fds[0];
    fds->events = POLLIN;
    fds->revents = 0;

    state.page = page;
    state.release = 0;
    state.done = 0;
    state.at_ms = 0;
    if (pthread_create(&thread, NULL, unmap_after_release, &state) != 0) {
        return 16;
    }
    /* Release the unmapper: it sleeps 50 ms, then unmaps while ppoll waits on the pipe. */
    state.release = 1;
    started = now_ms();
    errno = 0;
    result = ppoll(fds, 1, &timeout, NULL);
    finished = now_ms();
    if (pthread_join(thread, NULL) != 0) {
        return 17;
    }
    if (result != -1) {
        return 12;
    }
    if (errno != EFAULT) {
        return 13;
    }
    if (state.done != 1) {
        return 14;
    }
    if (started < 0 || state.at_ms < started) {
        return 15;
    }
    if (finished <= state.at_ms) {
        return 20;
    }
    if (finished - started < WAIT_MIN_MS || finished - started > WAIT_MAX_MS) {
        return 21;
    }
    return 0;
}

static int run_child(const char *label, int (*body)(void)) {
    int status = 0;
    pid_t child;

    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        _exit(body());
    }
    if (waitpid(child, &status, 0) < 0) {
        return fail("waitpid");
    }
    if (!WIFEXITED(status)) {
        return fail_child("child-signal", WTERMSIG(status));
    }
    if (WEXITSTATUS(status) != 0) {
        return fail_child("child-exit", WEXITSTATUS(status));
    }
    say(label);
    return 0;
}

int main(void) {
    if (mapped_control() != 0) {
        return 1;
    }
    if (run_child("pg26-copyout: read-only EFAULT\n", read_only_child) != 0) {
        return 1;
    }
    if (run_child("pg26-copyout: concurrent-unmap EFAULT\n", unmapped_child) != 0) {
        return 1;
    }
    return 0;
}
