/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-time: clocks, sleep, gmtime/timegm, the POSIX TZ parser, strftime/strptime,
 * time(), clock() and difftime. Expected strings are hand-written in
 * `vectors/time-expected.txt` and installed at `/etc/time-expected.txt`.
 */

#define RL_CASE "libc-time"
#include "rl_test.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <time.h>
#include <stdint.h>

extern char *tzname[2];

static int vector_lookup(const char *name, char *out, size_t capacity)
{
    FILE *file = fopen("/etc/time-expected.txt", "r");
    char line[256];
    size_t length = strlen(name);
    int found = 0;

    if (file == NULL) {
        return 0;
    }
    while (fgets(line, sizeof(line), file) != NULL) {
        char *value;
        size_t value_length;

        if (line[0] == '#') {
            continue;
        }
        if (strncmp(line, name, length) != 0 || line[length] != '\t') {
            continue;
        }
        value = line + length + 1;
        value_length = strlen(value);
        while (value_length > 0 &&
               (value[value_length - 1] == '\n' || value[value_length - 1] == '\r')) {
            value[--value_length] = '\0';
        }
        if (value_length + 1 <= capacity) {
            memcpy(out, value, value_length + 1);
            found = 1;
        }
        break;
    }
    fclose(file);
    return found;
}

static int vector_check(const char *name, const char *actual)
{
    char expected[128];

    return vector_lookup(name, expected, sizeof(expected)) && strcmp(expected, actual) == 0;
}

static long long diff_ms(const struct timespec *start, const struct timespec *end)
{
    long long seconds = (long long)end->tv_sec - (long long)start->tv_sec;
    long long nanos = (long long)end->tv_nsec - (long long)start->tv_nsec;

    return seconds * 1000 + nanos / 1000000;
}

static int check_clocks(void)
{
    struct timespec now;
    struct timespec res;
    struct timespec previous;
    int index;
    int advanced = 0;
    int checks = 0;

    checks += clock_gettime(CLOCK_REALTIME, &now) == 0;
    checks += now.tv_nsec >= 0 && now.tv_nsec < 1000000000L;
    checks += clock_gettime(CLOCK_MONOTONIC, &previous) == 0;
    for (index = 0; index < 1000; index++) {
        checks += clock_gettime(CLOCK_MONOTONIC, &now) == 0;
        if (now.tv_sec < previous.tv_sec ||
            (now.tv_sec == previous.tv_sec && now.tv_nsec < previous.tv_nsec)) {
            return 0;
        }
        if (now.tv_sec > previous.tv_sec ||
            (now.tv_sec == previous.tv_sec && now.tv_nsec > previous.tv_nsec)) {
            advanced = 1;
        }
        previous = now;
    }
    checks += advanced;
    checks += clock_getres(CLOCK_MONOTONIC, &res) == 0;
    checks += res.tv_sec > 0 || res.tv_nsec > 0;

    errno = 0;
    if (clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &now) != 0) {
        checks += errno == EINVAL || errno == ENOSYS;
    } else {
        checks += now.tv_nsec >= 0 && now.tv_nsec < 1000000000L;
    }
    return checks >= 1007;
}

static int check_sleep(void)
{
    struct timespec start;
    struct timespec end;
    struct timespec request;
    struct timespec invalid;
    long long elapsed;
    int checks = 0;

    request.tv_sec = 0;
    request.tv_nsec = 50 * 1000 * 1000;
    checks += clock_gettime(CLOCK_MONOTONIC, &start) == 0;
    checks += nanosleep(&request, NULL) == 0;
    checks += clock_gettime(CLOCK_MONOTONIC, &end) == 0;
    elapsed = diff_ms(&start, &end);
    checks += elapsed >= 50 && elapsed <= 500;
    checks += usleep(10000) == 0;
    checks += sleep(0) == 0;

    invalid.tv_sec = 0;
    invalid.tv_nsec = 1000000000L;
    errno = 0;
    checks += nanosleep(&invalid, NULL) == -1 && errno == EINVAL;
    return checks >= 7;
}

static int check_gmtime(void)
{
    static const time_t values[] = {
        0, 951782400, 2147483647, -1, 4102444800,
    };
    static const int years[] = { 1970, 2000, 2038, 1969, 2100 };
    static const int months[] = { 0, 1, 0, 11, 0 };
    static const int days[] = { 1, 29, 19, 31, 1 };
    struct tm broken;
    time_t overflow = INT64_MAX / 60;
    int index;
    int checks = 0;

    for (index = 0; index < 5; index++) {
        memset(&broken, 0, sizeof(broken));
        checks += gmtime_r(&values[index], &broken) != NULL;
        checks += broken.tm_year == years[index] - 1900;
        checks += broken.tm_mon == months[index];
        checks += broken.tm_mday == days[index];
        checks += broken.tm_gmtoff == 0;
        checks += broken.tm_zone != NULL;
    }
    errno = 0;
    checks += gmtime_r(&overflow, &broken) == NULL && errno == EOVERFLOW;
    return checks >= 31;
}

static int check_timegm(void)
{
    static const time_t values[] = {
        0, 951782400, 2147483647, -1, 4102444800,
    };
    struct tm broken;
    int index;
    int checks = 0;

    for (index = 0; index < 5; index++) {
        checks += gmtime_r(&values[index], &broken) != NULL;
        checks += timegm(&broken) == values[index];
    }
    return checks >= 10;
}

static int check_localtime(void)
{
    struct tm broken;
    struct tm roundtrip;
    time_t value;
    int checks = 0;

    checks += setenv("TZ", "EST5EDT,M3.2.0,M11.1.0", 1) == 0;
    tzset();

    value = 1700000000;
    checks += localtime_r(&value, &broken) != NULL;
    checks += broken.tm_isdst == 0;
    checks += broken.tm_gmtoff == -18000;
    checks += tzname[0] != NULL && strcmp(tzname[0], "EST") == 0;
    roundtrip = broken;
    checks += mktime(&roundtrip) == value;

    value = 1690000000;
    checks += localtime_r(&value, &broken) != NULL;
    checks += broken.tm_isdst == 1;
    checks += broken.tm_gmtoff == -14400;
    checks += tzname[1] != NULL && strcmp(tzname[1], "EDT") == 0;
    roundtrip = broken;
    checks += mktime(&roundtrip) == value;

    checks += setenv("TZ", "<+0530>-5:30", 1) == 0;
    tzset();
    value = 0;
    checks += localtime_r(&value, &broken) != NULL;
    checks += broken.tm_gmtoff == 19800;
    return checks >= 14;
}

static int check_strftime(void)
{
    static const char *const names[] = {
        "Y", "m", "d", "H", "M", "S", "Z", "z", "a", "A", "b", "B", "j",
        "U", "W", "w", "y", "C", "e", "p", "I", "s", "pct", "F", "T", "D", "R",
    };
    static const char *const formats[] = {
        "%Y", "%m", "%d", "%H", "%M", "%S", "%Z", "%z", "%a", "%A", "%b", "%B", "%j",
        "%U", "%W", "%w", "%y", "%C", "%e", "%p", "%I", "%s", "%%", "%F", "%T", "%D", "%R",
    };
    struct tm broken;
    time_t value = 1709210096;
    char buffer[128];
    char small[4];
    int index;
    int checks = 0;

    checks += gmtime_r(&value, &broken) != NULL;
    for (index = 0; index < 27; index++) {
        memset(buffer, 0, sizeof(buffer));
        checks += strftime(buffer, sizeof(buffer), formats[index], &broken) > 0;
        checks += vector_check(names[index], buffer);
    }
    checks += strftime(small, sizeof(small), "%Y-%m-%d", &broken) == 0;
    return checks >= 55;
}

static int check_strptime(void)
{
    struct tm broken;
    char *end;
    time_t value = 1709210096;
    int checks = 0;

    memset(&broken, 0, sizeof(broken));
    end = strptime("2024-02-29 12:34:56", "%Y-%m-%d %H:%M:%S", &broken);
    checks += end != NULL && *end == '\0';
    checks += broken.tm_year == 124 && broken.tm_mon == 1 && broken.tm_mday == 29;
    checks += broken.tm_hour == 12 && broken.tm_min == 34 && broken.tm_sec == 56;
    checks += timegm(&broken) == value;

    checks += strptime("2024/02/29", "%Y-%m-%d", &broken) == NULL;
    return checks >= 5;
}

static int check_time(void)
{
    struct timespec now;
    time_t seconds;
    long long difference;

    if (clock_gettime(CLOCK_REALTIME, &now) != 0) {
        return 0;
    }
    seconds = time(NULL);
    if (seconds == (time_t)-1) {
        return 0;
    }
    difference = (long long)seconds - (long long)now.tv_sec;
    return difference >= -2 && difference <= 2;
}

static int check_clock(void)
{
    clock_t first = clock();
    clock_t second;

    if (first == (clock_t)-1) {
        return 1;
    }
    if (first < 0) {
        return 0;
    }
    second = clock();
    return second != (clock_t)-1 && second >= first;
}

static int check_difftime(void)
{
    return difftime(20, 10) == 10.0 && difftime(10, 20) == -10.0 && difftime(7, 7) == 0.0;
}

int main(void)
{
    rl_out("libc-time: start");
    RL_CHECK("clocks", check_clocks());
    RL_CHECK("sleep", check_sleep());
    RL_CHECK("gmtime", check_gmtime());
    RL_CHECK("timegm", check_timegm());
    RL_CHECK("localtime", check_localtime());
    RL_CHECK("strftime", check_strftime());
    RL_CHECK("strptime", check_strptime());
    RL_CHECK("time", check_time());
    RL_CHECK("clock", check_clock());
    RL_CHECK("difftime", check_difftime());
    rl_skip("abstime ok", "PG-17");
    rl_skip("cputime ok", "PG-18");
    return rl_finish();
}
