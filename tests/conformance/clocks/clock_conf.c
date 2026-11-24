// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S176 clock and sleep conformance rows. The program is test-only: it pins the POSIX/FreeBSD
// clock contract on the release image and never changes product behaviour.

#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t got_signal;

static void on_signal(int signal_number) {
    got_signal = signal_number;
}

static long long ns_of(const struct timespec *value) {
    return (long long)value->tv_sec * 1000000000LL + (long long)value->tv_nsec;
}

static long long elapsed_ns(const struct timespec *before, const struct timespec *after) {
    return ns_of(after) - ns_of(before);
}

static int fail(const char *row, long long value, int error) {
    printf("clocks: fail %s value=%lld errno=%d\n", row, value, error);
    return 1;
}

static int ids_row(void) {
    if (CLOCK_REALTIME != 0 || CLOCK_MONOTONIC != 1) {
        return fail("ids", CLOCK_MONOTONIC, 0);
    }
    printf("clocks: ids ok\n");
    return 0;
}

static int res_row(void) {
    struct timespec first = {0, 0};
    struct timespec second = {0, 0};
    struct timespec realtime = {0, 0};
    if (clock_getres(CLOCK_MONOTONIC, &first) != 0 ||
        clock_getres(CLOCK_MONOTONIC, &second) != 0 ||
        clock_getres(CLOCK_REALTIME, &realtime) != 0) {
        return fail("getres", 0, errno);
    }
    if (ns_of(&first) <= 0 || ns_of(&first) != ns_of(&second) || ns_of(&realtime) <= 0) {
        return fail("getres", ns_of(&first), 0);
    }
    printf("clocks: getres ok\n");
    return 0;
}

static int monotonic_row(void) {
    struct timespec previous = {0, 0};
    struct timespec current = {0, 0};
    long long first;
    long long last;
    if (clock_gettime(CLOCK_MONOTONIC, &previous) != 0) {
        return fail("monotonic", 0, errno);
    }
    first = ns_of(&previous);
    if (first <= 0) {
        return fail("monotonic", first, 0);
    }
    for (int sample = 0; sample < 10000; sample += 1) {
        if (clock_gettime(CLOCK_MONOTONIC, &current) != 0) {
            return fail("monotonic", sample, errno);
        }
        if (ns_of(&current) < ns_of(&previous) || ns_of(&current) <= 0) {
            return fail("monotonic", ns_of(&current), 0);
        }
        previous = current;
    }
    last = ns_of(&previous);
    if (last < first) {
        return fail("monotonic", last, 0);
    }
    printf("clocks: monotonic ok\n");
    return 0;
}

static void *smp_monotonic_worker(void *argument) {
    int *failures = (int *)argument;
    struct timespec current = {0, 0};
    struct timespec previous = {0, 0};
    if (clock_gettime(CLOCK_MONOTONIC, &previous) != 0) {
        *failures = 1;
        return NULL;
    }
    if (ns_of(&previous) <= 0) {
        *failures = 1;
        return NULL;
    }
    for (int sample = 0; sample < 10000; sample += 1) {
        if (clock_gettime(CLOCK_MONOTONIC, &current) != 0 ||
            ns_of(&current) < ns_of(&previous) || ns_of(&current) <= 0) {
            *failures = 1;
            return NULL;
        }
        previous = current;
    }
    return NULL;
}

static int smp_monotonic_row(void) {
    pthread_t first;
    pthread_t second;
    int first_failures = 0;
    int second_failures = 0;
    if (pthread_create(&first, NULL, smp_monotonic_worker, &first_failures) != 0 ||
        pthread_create(&second, NULL, smp_monotonic_worker, &second_failures) != 0) {
        return fail("monotonic_smp create", 0, errno);
    }
    (void)pthread_join(first, NULL);
    (void)pthread_join(second, NULL);
    if (first_failures != 0 || second_failures != 0) {
        return fail("monotonic_smp", first_failures + second_failures, 0);
    }
    // The libc sysconf/sched_getcpu stubs report 1/0; the qtest asserts the kernel's
    // "smp: 2 cpus online" and "sched: running (2 cpus" lines for the SMP evidence.
    printf("clocks: monotonic_smp ok threads=2\n");
    return 0;
}

static int sleep_row(void) {
    struct timespec request = {0, 100000000};
    struct timespec before = {0, 0};
    struct timespec after = {0, 0};
#ifdef MUTATION_EARLY_SLEEP
    request.tv_nsec = 1000000;
#endif
    if (clock_gettime(CLOCK_MONOTONIC, &before) != 0) {
        return fail("sleep", 0, errno);
    }
    if (nanosleep(&request, NULL) != 0) {
        return fail("sleep", 0, errno);
    }
    if (clock_gettime(CLOCK_MONOTONIC, &after) != 0) {
        return fail("sleep", 0, errno);
    }
    if (elapsed_ns(&before, &after) < 100000000LL) {
        return fail("sleep", elapsed_ns(&before, &after), 0);
    }
    printf("clocks: sleep ok elapsed_ns=%lld\n", elapsed_ns(&before, &after));
    return 0;
}

static int eintr_row(void) {
    struct sigaction action;
    struct timespec request = {1, 0};
    struct timespec remaining = {0, 0};
    struct timespec child_delay = {0, 100000000};
    pid_t child;
    int result;
    int saved_errno;

    action.sa_handler = on_signal;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sigaction(SIGUSR1, &action, NULL) != 0) {
        return fail("eintr", 0, errno);
    }
    got_signal = 0;
    child = fork();
    if (child == 0) {
        (void)nanosleep(&child_delay, NULL);
        (void)kill(getppid(), SIGUSR1);
        _exit(0);
    }
    if (child < 0) {
        return fail("eintr", 0, errno);
    }
    errno = 0;
    result = nanosleep(&request, &remaining);
    saved_errno = errno;
    (void)waitpid(child, NULL, 0);
    if (result != -1 || saved_errno != EINTR || got_signal == 0 ||
        (remaining.tv_sec == 0 && remaining.tv_nsec == 0) ||
        ns_of(&remaining) >= 1000000000LL) {
        return fail("eintr", (long long)result, saved_errno);
    }
    printf("clocks: eintr ok remaining_ns=%lld\n", ns_of(&remaining));
    return 0;
}

static int gettimeofday_row(void) {
    struct timespec realtime = {0, 0};
    struct timeval wall = {0, 0};
    if (clock_gettime(CLOCK_REALTIME, &realtime) != 0 ||
        gettimeofday(&wall, NULL) != 0) {
        return fail("gettimeofday", 0, errno);
    }
    long long real_ns = (long long)realtime.tv_sec * 1000000000LL + (long long)realtime.tv_nsec;
    long long wall_ns = (long long)wall.tv_sec * 1000000000LL + (long long)wall.tv_usec * 1000LL;
    long long delta = real_ns - wall_ns;
    if (delta < 0) {
        delta = -delta;
    }
    if (delta > 50000000LL) {
        return fail("gettimeofday", delta, 0);
    }
    printf("clocks: gettimeofday ok delta_ns=%lld\n", delta);
    return 0;
}

static int rate_row(void) {
    struct timespec real_before = {0, 0};
    struct timespec real_after = {0, 0};
    struct timespec mono_before = {0, 0};
    struct timespec mono_after = {0, 0};
    struct timespec request = {0, 100000000};
    if (clock_gettime(CLOCK_REALTIME, &real_before) != 0 ||
        clock_gettime(CLOCK_MONOTONIC, &mono_before) != 0 ||
        nanosleep(&request, NULL) != 0 ||
        clock_gettime(CLOCK_REALTIME, &real_after) != 0 ||
        clock_gettime(CLOCK_MONOTONIC, &mono_after) != 0) {
        return fail("rates", 0, errno);
    }
    long long real = elapsed_ns(&real_before, &real_after);
    long long mono = elapsed_ns(&mono_before, &mono_after);
    long long difference = real - mono;
    if (difference < 0) {
        difference = -difference;
    }
    if (real < 50000000LL || mono < 50000000LL || difference > 50000000LL) {
        return fail("rates", difference, 0);
    }
    printf("clocks: rates ok real_ns=%lld mono_ns=%lld\n", real, mono);
    return 0;
}

int main(void) {
    int failures = 0;
    setvbuf(stdout, NULL, _IONBF, 0);
    failures += ids_row();
    failures += res_row();
    failures += monotonic_row();
    failures += smp_monotonic_row();
    failures += sleep_row();
    failures += eintr_row();
    failures += gettimeofday_row();
    failures += rate_row();
    if (failures != 0) {
        printf("clocks: fail rows=%d\n", failures);
        fflush(stdout);
        _exit(1);
    }
    printf("clocks: ok\n");
    fflush(stdout);
    _exit(0);
}
