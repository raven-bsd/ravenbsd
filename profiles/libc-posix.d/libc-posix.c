/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * K295/P1 libc-posix: one on-target client that exercises a representative POSIX.1-2024
 * surface from the P1 batches: stdio, string/collation, stdlib sort/search, time, locale,
 * wide characters, ctype and process identity. It is built by profiles/libc-posix.d/build.sh
 * and staged by the existing libc-test profile as /bin/libc-test.
 */

#define RL_CASE "libc-posix"
#include "rl_test.h"

#include <ctype.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wchar.h>

static int compare_int(const void *left, const void *right)
{
    int a = *(const int *)left;
    int b = *(const int *)right;

    return (a > b) - (a < b);
}

static int check_stdio(void)
{
    char buffer[64];
    char word[16];
    char line[32];
    FILE *file;
    int number = 0;
    int checks = 0;

    checks += snprintf(buffer, sizeof(buffer), "%s %d", "posix", 7) > 0;
    checks += strcmp(buffer, "posix 7") == 0;
    checks += sscanf("42 word", "%d %15s", &number, word) == 2;
    checks += number == 42 && strcmp(word, "word") == 0;

    file = fopen("/etc/libc-test.txt", "r");
    if (file == NULL) {
        return 0;
    }
    checks += fgets(line, sizeof(line), file) != NULL;
    checks += strncmp(line, "line one", 8) == 0;
    checks += fclose(file) == 0;
    return checks >= 7;
}

static int check_string(void)
{
    const char *text = "alpha beta";
    char copy[16];
    char transformed[32];
    size_t length;
    int checks = 0;

    checks += strlen(text) == 10;
    checks += strcmp("alpha", "alpha") == 0;
    checks += strchr(text, 'b') == text + 6;
    checks += strstr(text, "beta") == text + 6;
    memset(copy, 0, sizeof(copy));
    memcpy(copy, text, 5);
    checks += strcmp(copy, "alpha") == 0;
    length = strxfrm(transformed, "alpha", sizeof(transformed));
    checks += length == 5 && strcmp(transformed, "alpha") == 0;
    checks += strcoll("alpha", "beta") < 0;
    checks += strcoll("beta", "alpha") > 0;
    return checks >= 8;
}

static int check_stdlib(void)
{
    int values[5] = { 4, 1, 3, 5, 2 };
    int key = 3;
    int *found;
    void *block;
    int checks = 0;

    qsort(values, 5, sizeof(values[0]), compare_int);
    checks += values[0] == 1 && values[1] == 2 && values[2] == 3;
    checks += values[3] == 4 && values[4] == 5;
    found = bsearch(&key, values, 5, sizeof(values[0]), compare_int);
    checks += found != NULL && *found == 3;
    key = 6;
    checks += bsearch(&key, values, 5, sizeof(values[0]), compare_int) == NULL;
    block = malloc(256);
    checks += block != NULL;
    if (block != NULL) {
        memset(block, 0x5a, 256);
        checks += ((unsigned char *)block)[0] == 0x5a;
        free(block);
    }
    return checks >= 6;
}

static int check_time(void)
{
    struct timespec now;
    struct tm broken;
    time_t epoch = 0;
    time_t current;
    char year[16];
    int checks = 0;

    checks += clock_gettime(CLOCK_MONOTONIC, &now) == 0;
    checks += now.tv_nsec >= 0 && now.tv_nsec < 1000000000L;
    current = time(NULL);
    checks += current != (time_t)-1;
    checks += gmtime_r(&epoch, &broken) != NULL;
    checks += broken.tm_year == 70 && broken.tm_mon == 0 && broken.tm_mday == 1;
    checks += strftime(year, sizeof(year), "%Y", &broken) == 4;
    checks += strcmp(year, "1970") == 0;
    return checks >= 7;
}

static int check_locale(void)
{
    struct lconv *conventions;
    int checks = 0;

    checks += setlocale(LC_ALL, "C") != NULL;
    conventions = localeconv();
    checks += conventions != NULL;
    checks += conventions != NULL && conventions->decimal_point != NULL;
    return checks >= 3;
}

static int check_wchar(void)
{
    wchar_t wide[8];
    char narrow[8];
    wchar_t converted = 0;
    mbstate_t state;
    size_t length;
    int checks = 0;

    memset(wide, 0, sizeof(wide));
    length = mbstowcs(wide, "abc", 8);
    checks += length == 3 && wide[0] == L'a' && wide[2] == L'c';
    checks += wcslen(wide) == 3;
    memset(narrow, 0, sizeof(narrow));
    length = wcstombs(narrow, wide, sizeof(narrow));
    checks += length == 3 && strcmp(narrow, "abc") == 0;
    memset(&state, 0, sizeof(state));
    length = mbrtowc(&converted, "x", 1, &state);
    checks += length == 1 && converted == L'x';
    return checks >= 4;
}

static int check_ctype(void)
{
    int checks = 0;

    checks += isalpha((unsigned char)'A') != 0;
    checks += isdigit((unsigned char)'7') != 0;
    checks += isspace((unsigned char)' ') != 0;
    checks += tolower((unsigned char)'A') == 'a';
    checks += toupper((unsigned char)'a') == 'A';
    return checks >= 5;
}

static int check_process(void)
{
    int checks = 0;

    checks += getpid() > 0;
    checks += getppid() > 0;
    checks += getuid() >= 0;
    checks += geteuid() >= 0;
    return checks >= 4;
}

int main(void)
{
    rl_out("libc-posix: start");
    RL_CHECK("stdio", check_stdio());
    RL_CHECK("string", check_string());
    RL_CHECK("stdlib", check_stdlib());
    RL_CHECK("time", check_time());
    RL_CHECK("locale", check_locale());
    RL_CHECK("wchar", check_wchar());
    RL_CHECK("ctype", check_ctype());
    RL_CHECK("process", check_process());
    return rl_finish();
}
