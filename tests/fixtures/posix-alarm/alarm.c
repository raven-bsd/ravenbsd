/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C74hq posix-alarm: prove on target that SIGALRM is delivered by the libc alarm/pause/ualarm
 * seam. The SIGALRM handler is installed through sigaction and read back to prove the stored
 * disposition; the timer-driven signals are consumed by pause(), which is the P1f8 delivery seam.
 * Every check prints a pass marker only after the observable result, and rl_finish() prints done
 * only when all checks passed.
 */

#define RL_CASE "posix-alarm"
#include "rl_test.h"

#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t alarm_count;

static void on_alarm(int signo)
{
    (void)signo;
    alarm_count++;
}

static void *thread_noop(void *argument)
{
    return argument;
}

static long long now_ms(void)
{
    struct timespec ts;

    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return -1;
    }
    return (long long)ts.tv_sec * 1000LL + (long long)ts.tv_nsec / 1000000LL;
}

static void emit_value(const char *name, long value)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": ");
    rl_append(&cursor, name);
    rl_append(&cursor, "=");
    rl_append_uint(&cursor, value);
    rl_out(line);
}

static void emit_ms(const char *name, long value)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": ");
    rl_append(&cursor, name);
    rl_append(&cursor, "-ms=");
    rl_append_uint(&cursor, value);
    rl_out(line);
}

int main(void)
{
    struct sigaction action;
    struct sigaction old;
    struct sigaction current;
    long long started;
    long long elapsed;
    int installed;

    rl_out("posix-alarm: start");

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = on_alarm;
    action.sa_flags = 0;
    memset(&old, 0, sizeof(old));
    memset(&current, 0, sizeof(current));
    installed = sigaction(SIGALRM, &action, &old) == 0 && old.sa_handler == SIG_DFL;
    RL_CHECK("handler-installed", installed);
    installed = sigaction(SIGALRM, NULL, &current) == 0 && current.sa_handler == on_alarm;
    RL_CHECK("handler-stored", installed);

    pthread_t probe;
    /* libc alarm starts a detached timer thread; surface that prerequisite explicitly. */
    int thread_rc = pthread_create(&probe, NULL, thread_noop, NULL);
    RL_CHECK("thread-create", thread_rc == 0);
    emit_value("thread-rc", thread_rc);
    if (thread_rc != 0) {
        return rl_finish();
    }
    (void)pthread_detach(probe);

    errno = 0;
    int previous = (int)alarm(1);
    started = now_ms();
    int paused = pause();
    int saved_errno = errno;
    elapsed = now_ms() - started;
    RL_CHECK("alarm-eintr", previous == 0 && paused == -1 && saved_errno == EINTR);
    RL_CHECK("alarm-bounded", elapsed >= 800 && elapsed <= 5000);
    emit_ms("alarm", (long)elapsed);

    errno = 0;
    previous = (int)ualarm(200000U, 0U);
    started = now_ms();
    paused = pause();
    saved_errno = errno;
    elapsed = now_ms() - started;
    RL_CHECK("ualarm-eintr", previous == 0 && paused == -1 && saved_errno == EINTR);
    RL_CHECK("ualarm-bounded", elapsed >= 100 && elapsed <= 5000);
    emit_ms("ualarm", (long)elapsed);

    errno = 0;
    previous = (int)ualarm(100000U, 100000U);
    started = now_ms();
    int first = pause();
    int first_errno = errno;
    int second = pause();
    int second_errno = errno;
    elapsed = now_ms() - started;
    RL_CHECK("interval-first", previous == 0 && first == -1 && first_errno == EINTR);
    RL_CHECK("interval-second", second == -1 && second_errno == EINTR);
    RL_CHECK("interval-bounded", elapsed >= 150 && elapsed <= 6000);
    emit_ms("interval", (long)elapsed);
    (void)ualarm(0U, 0U);

    previous = (int)alarm(10);
    int cancelled = (int)alarm(0);
    RL_CHECK("cancel-return", previous == 0 && cancelled >= 9);
    RL_CHECK("cancel-clear", alarm(0) == 0);

    return rl_finish();
}
