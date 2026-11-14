/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-hello: the first C program built against ravenlibc. It checks the crt path, the
 * startup block, errno, static TLS, malloc and the string functions. Every marker is printed
 * after the check it names; the atexit marker is printed by the handler after `done`.
 */

#define RL_CASE "libc-hello"
#include "rl_test.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <sys/auxv.h>
#include <elf.h>

extern char **environ;

static _Thread_local int rl_tdata = 7;
static _Thread_local int rl_tbss;

static void atexit_marker(void)
{
    rl_out("libc-hello: atexit ran");
}

static int check_argv(int argc, char **argv)
{
    return argc == 1 && argv[0] != NULL && strcmp(argv[0], "/bin/libc-test") == 0 &&
           argv[1] == NULL;
}

static int check_environ(void)
{
    int index;

    if (environ == NULL) {
        return 0;
    }
    for (index = 0; index < 4096; index++) {
        if (environ[index] == NULL) {
            return 1;
        }
    }
    return 0;
}

static int check_auxv(void)
{
    unsigned long random = getauxval(AT_RANDOM);
    unsigned long phnum = getauxval(AT_PHNUM);
    const unsigned char *bytes;
    unsigned long index;
    volatile unsigned long sum = 0;

    if (getauxval(AT_PAGESZ) != 4096 || random == 0 || phnum == 0) {
        return 0;
    }
    bytes = (const unsigned char *)random;
    for (index = 0; index < 16; index++) {
        sum += bytes[index];
    }
    (void)sum;
    return 1;
}

static int check_errno(void)
{
    int fd = open("/nonexistent/x", O_RDONLY);

    if (fd >= 0) {
        close(fd);
        return 0;
    }
    return errno == ENOENT && strcmp(strerror(errno), "No such file or directory") == 0;
}

static int check_tls(void)
{
    return rl_tdata == 7 && rl_tbss == 0;
}

static int check_malloc(void)
{
    unsigned char *block = malloc(1024 * 1024);
    unsigned char *grown;
    unsigned char *zeroed;
    int index;

    if (block == NULL) {
        return 0;
    }
    memset(block, 0xa5, 1024 * 1024);
    grown = realloc(block, 2 * 1024 * 1024);
    if (grown == NULL) {
        free(block);
        return 0;
    }
    for (index = 0; index < 1024 * 1024; index++) {
        if (grown[index] != 0xa5) {
            free(grown);
            return 0;
        }
    }
    zeroed = calloc(1000, 1000);
    if (zeroed == NULL) {
        free(grown);
        return 0;
    }
    for (index = 0; index < 1000 * 1000; index++) {
        if (zeroed[index] != 0) {
            free(zeroed);
            free(grown);
            return 0;
        }
    }
    free(zeroed);
    free(grown);
    for (index = 0; index < 10000; index++) {
        unsigned char *small = malloc(48);
        if (small == NULL) {
            return 0;
        }
        memset(small, index & 0xff, 48);
        free(small);
    }
    return 1;
}

static int check_strings(void)
{
    static const char *const names[] = {
        "", "a", "abc", "ravenbsd", "The quick brown fox",
    };
    static const size_t lengths[] = { 0, 1, 3, 8, 19 };
    const char *haystack = "ravenbsd";
    char buffer[64];
    char moved[64];
    int index;
    int vectors = 0;

    for (index = 0; index < 5; index++) {
        if (strlen(names[index]) != lengths[index]) {
            return 0;
        }
        vectors++;
    }
    if (strcmp("abc", "abc") != 0 || strcmp("abc", "abd") >= 0 || strcmp("abd", "abc") <= 0) {
        return 0;
    }
    vectors++;
    memset(buffer, 0, sizeof(buffer));
    memcpy(buffer, "raven", 5);
    if (strcmp(buffer, "raven") != 0) {
        return 0;
    }
    vectors++;
    memcpy(moved, "abcdef", 7);
    memmove(moved + 1, moved, 5);
    if (strcmp(moved, "aabcdf") != 0) {
        return 0;
    }
    vectors++;
    if (strchr(haystack, 'b') == NULL || strchr(haystack, 'z') != NULL) {
        return 0;
    }
    vectors++;
    if (strchr(haystack, 'r') != haystack) {
        return 0;
    }
    vectors++;
    if (memcmp("abc", "abc", 3) != 0 || memcmp("abc", "abd", 3) == 0) {
        return 0;
    }
    vectors++;
    if (strstr(haystack, "venb") == NULL || strstr(haystack, "xyz") != NULL) {
        return 0;
    }
    vectors++;
    return vectors >= 12;
}

int main(int argc, char **argv)
{
    int ok;

    rl_out("libc-hello: start");
    RL_CHECK("argc", check_argv(argc, argv));
    RL_CHECK("environ", check_environ());
    RL_CHECK("auxv", check_auxv());
    RL_CHECK("errno", check_errno());
    RL_CHECK("tls", check_tls());
    RL_CHECK("malloc", check_malloc());
    RL_CHECK("string", check_strings());

    ok = atexit(atexit_marker) == 0;
    RL_CHECK("atexit", ok);
    if (!ok) {
        return rl_finish();
    }

    /* H-5: the public ABI has no entropy query; the documented state until PG-52 is weak. */
    rl_out("libc-hello: entropy weak=1");
    rl_out("libc-hello: done");
    return rl_finish();
}
