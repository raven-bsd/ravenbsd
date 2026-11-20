/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * K213 itimer: the libc setitimer/getitimer C ABI over the real ITIMER_REAL kernel timer and
 * SIGALRM delivery path. Proves set/get, the previous value, a zero-value cancel, the shared
 * alarm(2) state, a repeating interval, and EINVAL for unsupported selectors and out-of-range
 * timeval fields.
 */

#define RL_CASE "itimer"
#include "rl_test.h"

#include <errno.h>
#include <signal.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

static void on_alarm(int signo)
{
    (void)signo;
}

static long long now_ms(void)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
        return -1;
    return (long long)ts.tv_sec * 1000LL + (long long)ts.tv_nsec / 1000000LL;
}

static long long usec_of(const struct timeval *value)
{
    return (long long)value->tv_sec * 1000000LL + (long long)value->tv_usec;
}

static void zero(struct itimerval *value)
{
    memset(value, 0, sizeof(*value));
}

int main(void)
{
    struct itimerval request;
    struct itimerval old;
    struct itimerval current;
    struct sigaction action;
    struct sigaction stored;
    long long started;
    long long elapsed;
    long long remaining;
    long long previous_us;
    int rc;

    rl_out("itimer: start");

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = on_alarm;
    action.sa_flags = 0;
    memset(&stored, 0, sizeof(stored));
    RL_CHECK("handler-installed", sigaction(SIGALRM, &action, NULL) == 0);
    RL_CHECK("handler-stored", sigaction(SIGALRM, NULL, &stored) == 0 &&
                                    stored.sa_handler == on_alarm);

    zero(&request);
    zero(&old);
    request.it_value.tv_usec = 300000;
    errno = 0;
    rc = setitimer(ITIMER_REAL, &request, &old);
    RL_CHECK("set-first", rc == 0 && usec_of(&old.it_value) == 0 &&
                               usec_of(&old.it_interval) == 0);

    zero(&current);
    rc = getitimer(ITIMER_REAL, &current);
    remaining = usec_of(&current.it_value);
    RL_CHECK("get-armed", rc == 0 && remaining > 0 && remaining <= 300000 &&
                              usec_of(&current.it_interval) == 0);

    started = now_ms();
    errno = 0;
    rc = pause();
    elapsed = now_ms() - started;
    /* The existing libc delivery seam consumes SIGALRM through pause/PROC_SIGRECV. */
    RL_CHECK("deliver-eintr", rc == -1 && errno == EINTR);
    RL_CHECK("deliver-bounded", elapsed >= 150 && elapsed <= 5000);

    zero(&current);
    rc = getitimer(ITIMER_REAL, &current);
    RL_CHECK("get-expired", rc == 0 && usec_of(&current.it_value) == 0);

    /* A replacement returns the previous (5 s) value. */
    zero(&request);
    request.it_value.tv_sec = 5;
    (void)setitimer(ITIMER_REAL, &request, NULL);
    zero(&request);
    request.it_value.tv_usec = 200000;
    zero(&old);
    rc = setitimer(ITIMER_REAL, &request, &old);
    previous_us = usec_of(&old.it_value);
    RL_CHECK("set-old", rc == 0 && previous_us >= 4000000 && previous_us <= 5000000);

    /* A zero value cancels and reports the 200 ms previous value. */
    zero(&request);
    zero(&old);
    rc = setitimer(ITIMER_REAL, &request, &old);
    previous_us = usec_of(&old.it_value);
    RL_CHECK("cancel-old", rc == 0 && previous_us > 0 && previous_us <= 200000);
    zero(&current);
    rc = getitimer(ITIMER_REAL, &current);
    RL_CHECK("cancel-clear", rc == 0 && usec_of(&current.it_value) == 0);

    /* alarm(2) shares the same ITIMER_REAL state. */
    errno = 0;
    rc = (int)alarm(2);
    zero(&current);
    (void)getitimer(ITIMER_REAL, &current);
    remaining = usec_of(&current.it_value);
    RL_CHECK("alarm-shared", rc == 0 && remaining > 1500000 && remaining <= 2000000);
    rc = (int)alarm(0);
    RL_CHECK("alarm-cancel", rc >= 1);

    /* A repeating interval delivers more than one signal. */
    zero(&request);
    request.it_value.tv_usec = 100000;
    request.it_interval.tv_usec = 100000;
    rc = setitimer(ITIMER_REAL, &request, NULL);
    RL_CHECK("interval-set", rc == 0);
    errno = 0;
    int first = pause();
    errno = 0;
    int second = pause();
    RL_CHECK("interval-hit", first == -1 && second == -1);
    zero(&request);
    (void)setitimer(ITIMER_REAL, &request, NULL);

    /* Unsupported selectors and out-of-range fields are EINVAL. */
    zero(&request);
    request.it_value.tv_sec = 1;
    errno = 0;
    rc = setitimer(ITIMER_VIRTUAL, &request, NULL);
    RL_CHECK("einval-virtual", rc == -1 && errno == EINVAL);
    zero(&current);
    errno = 0;
    rc = getitimer(ITIMER_PROF, &current);
    RL_CHECK("einval-prof", rc == -1 && errno == EINVAL);
    zero(&request);
    request.it_value.tv_usec = 1000000;
    errno = 0;
    rc = setitimer(ITIMER_REAL, &request, NULL);
    RL_CHECK("einval-usec", rc == -1 && errno == EINVAL);
    zero(&request);
    request.it_value.tv_sec = -1;
    errno = 0;
    rc = setitimer(ITIMER_REAL, &request, NULL);
    RL_CHECK("einval-negative", rc == -1 && errno == EINVAL);

    return rl_finish();
}
