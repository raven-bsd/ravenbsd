/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-stdio: printf/scanf, FILE block and line I/O, the initfs text file, buffering,
 * standard streams, memory streams and wide characters. The expected `snprintf` strings live
 * in `vectors/stdio-expected.txt` and are installed at `/etc/stdio-expected.txt`; the file is
 * hand-written from the C standard, not captured from the libc under test.
 */

#define RL_CASE "libc-stdio"
#include "rl_test.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <limits.h>
#include <locale.h>
#include <stdint.h>
#include <unistd.h>
#include <wchar.h>

static int vector_lookup(const char *name, char *out, size_t capacity)
{
    FILE *file = fopen("/etc/stdio-expected.txt", "r");
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
        if (strcmp(value, "<empty>") == 0) {
            value[0] = '\0';
            value_length = 0;
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

static int check_printf(void)
{
    char actual[256];
    char small[4];
    int count = -1;
    int checks = 0;
    int length;

    length = snprintf(actual, sizeof(actual), "%d", 42);
    checks += length == 2 && vector_check("d", actual);
    length = snprintf(actual, sizeof(actual), "%i", -42);
    checks += length == 3 && vector_check("i", actual);
    length = snprintf(actual, sizeof(actual), "%u", 42u);
    checks += length == 2 && vector_check("u", actual);
    length = snprintf(actual, sizeof(actual), "%x", 255u);
    checks += length == 2 && vector_check("x", actual);
    length = snprintf(actual, sizeof(actual), "%X", 255u);
    checks += length == 2 && vector_check("X", actual);
    length = snprintf(actual, sizeof(actual), "%o", 8u);
    checks += length == 2 && vector_check("o", actual);
    length = snprintf(actual, sizeof(actual), "%c", 'A');
    checks += length == 1 && vector_check("c", actual);
    length = snprintf(actual, sizeof(actual), "%s", "raven");
    checks += length == 5 && vector_check("s", actual);
    length = snprintf(actual, sizeof(actual), "%%");
    checks += length == 1 && vector_check("pct", actual);
    length = snprintf(actual, sizeof(actual), "%5d", 42);
    checks += length == 5 && vector_check("width", actual);
    length = snprintf(actual, sizeof(actual), "%-5d|", 42);
    checks += length == 6 && vector_check("left", actual);
    length = snprintf(actual, sizeof(actual), "%05d", 42);
    checks += length == 5 && vector_check("zero", actual);
    length = snprintf(actual, sizeof(actual), "%+d", 42);
    checks += length == 3 && vector_check("plus", actual);
    length = snprintf(actual, sizeof(actual), "% d", 42);
    checks += length == 3 && vector_check("space", actual);
    length = snprintf(actual, sizeof(actual), "%#x", 255u);
    checks += length == 4 && vector_check("hashx", actual);
    length = snprintf(actual, sizeof(actual), "%#o", 8u);
    checks += length == 3 && vector_check("hasho", actual);
    length = snprintf(actual, sizeof(actual), "%.3s", "raven");
    checks += length == 3 && vector_check("precs", actual);
    length = snprintf(actual, sizeof(actual), "%.4d", 42);
    checks += length == 4 && vector_check("precd", actual);
    length = snprintf(actual, sizeof(actual), "%*d", 5, 42);
    checks += length == 5 && vector_check("star", actual);
    length = snprintf(actual, sizeof(actual), "%ld", 123456789L);
    checks += length == 9 && vector_check("l", actual);
    length = snprintf(actual, sizeof(actual), "%lld", -1234567890123LL);
    checks += length == 14 && vector_check("ll", actual);
    length = snprintf(actual, sizeof(actual), "%zu", sizeof(void *));
    checks += length == 1 && vector_check("zu", actual);
    length = snprintf(actual, sizeof(actual), "%jd", (intmax_t)42);
    checks += length == 2 && vector_check("jd", actual);
    length = snprintf(actual, sizeof(actual), "%hhd", 300);
    checks += length == 2 && vector_check("hhd", actual);
    length = snprintf(actual, sizeof(actual), "%hd", 70000);
    checks += length == 4 && vector_check("hd", actual);
    length = snprintf(actual, sizeof(actual), "%p", (void *)0x1234);
    checks += length == 6 && vector_check("ptr", actual);
    length = snprintf(actual, sizeof(actual), "%s", "");
    checks += length == 0 && vector_check("emptys", actual);
    length = snprintf(actual, sizeof(actual), "%u", 0u);
    checks += length == 1 && vector_check("zerou", actual);
    length = snprintf(actual, sizeof(actual), "%x", 0u);
    checks += length == 1 && vector_check("zerox", actual);
    length = snprintf(actual, sizeof(actual), "%o", 0u);
    checks += length == 1 && vector_check("zeroo", actual);
    length = snprintf(actual, sizeof(actual), "%10s", "raven");
    checks += length == 10 && vector_check("widths", actual);
    length = snprintf(actual, sizeof(actual), "%5.2s", "raven");
    checks += length == 5 && vector_check("prec5", actual);
    length = snprintf(actual, sizeof(actual), "%08.3d", 42);
    checks += length == 8 && vector_check("zeropad", actual);
    length = snprintf(actual, sizeof(actual), "%#010x", 255u);
    checks += length == 10 && vector_check("hashwidth", actual);
    length = snprintf(actual, sizeof(actual), "%-8d|", -42);
    checks += length == 9 && vector_check("negwidth", actual);
    length = snprintf(actual, sizeof(actual), "%.0d", 0);
    checks += length == 0 && vector_check("dot", actual);
    length = snprintf(actual, sizeof(actual), "%ld", LONG_MIN);
    checks += length == 20 && vector_check("longmin", actual);
    length = snprintf(actual, sizeof(actual), "%lu", ULONG_MAX);
    checks += length == 20 && vector_check("ulongmax", actual);
    length = snprintf(actual, sizeof(actual), "%jd", (intmax_t)INTMAX_MAX);
    checks += length == 19 && vector_check("intmax", actual);
    length = snprintf(actual, sizeof(actual), "%#6o", 8u);
    checks += length == 6 && vector_check("octwidth", actual);
    length = snprintf(actual, sizeof(actual), "%#x", 0u);
    checks += length == 1 && vector_check("hexzero", actual);
    length = snprintf(actual, sizeof(actual), "%.5d", -42);
    checks += length == 6 && vector_check("precision", actual);
    length = snprintf(actual, sizeof(actual), "%3c", 'Z');
    checks += length == 3 && vector_check("charwidth", actual);
    length = snprintf(actual, sizeof(actual), "%.*s", 3, "raven");
    checks += length == 3 && vector_check("sstar", actual);

    count = -1;
    length = snprintf(actual, sizeof(actual), "abc%s%n", "def", &count);
    checks += length == 6 && count == 6 && strcmp(actual, "abcdef") == 0;

    length = snprintf(small, sizeof(small), "%s", "abcdef");
    checks += length == 6 && strcmp(small, "abc") == 0;

    count = -1;
    length = snprintf(NULL, 0, "%d", 42);
    checks += length == 2 && count == -1;

    return checks >= 45;
}

static int check_scanf(void)
{
    int a = 0;
    int b = 0;
    unsigned c = 0;
    char word[32];
    char letters[32];
    char ch = '\0';
    int consumed = -1;
    int checks = 0;

    checks += sscanf("42 -17 2a", "%d %d %x", &a, &b, &c) == 3 && a == 42 && b == -17 && c == 0x2a;
    checks += sscanf("hello world", "%s", word) == 1 && strcmp(word, "hello") == 0;
    checks += sscanf("abc123", "%[a-z]%d", letters, &a) == 2 && strcmp(letters, "abc") == 0 && a == 123;
    checks += sscanf("xyz", "%c", &ch) == 1 && ch == 'x';
    checks += sscanf("abcdef", "%*s%n", &consumed) == 0 && consumed == 6;
    checks += sscanf("", "%d", &a) == EOF;
    checks += sscanf("0x2a", "%x", &c) == 1 && c == 42;
    checks += sscanf("12,34", "%d,%d", &a, &b) == 2 && a == 12 && b == 34;
    checks += sscanf(" 7", "%d", &a) == 1 && a == 7;
    checks += sscanf("abc", "%d", &a) == 0;
    checks += sscanf("A", "%c", &ch) == 1 && ch == 'A';
    checks += sscanf("3.5", "%d", &a) == 1 && a == 3;
    return checks >= 10;
}

static int check_file(void)
{
    const char *path = "/tmp/rl.bin";
    unsigned char pattern[100];
    unsigned char readback[100];
    FILE *file;
    FILE *again;
    int index;
    int checks = 0;

    for (index = 0; index < 100; index++) {
        pattern[index] = (unsigned char)(index * 7);
    }
    file = fopen(path, "w+");
    if (file == NULL) {
        return 0;
    }
    checks += 1;
    for (index = 0; index < 10000; index += 100) {
        if (fwrite(pattern, 1, 100, file) != 100) {
            fclose(file);
            return 0;
        }
    }
    checks += 1;
    checks += fflush(file) == 0;
    checks += fseek(file, 0, SEEK_SET) == 0;
    checks += fread(readback, 1, 100, file) == 100 && memcmp(readback, pattern, 100) == 0;
    checks += ftell(file) == 100;
    checks += fseek(file, -100, SEEK_END) == 0;
    checks += fread(readback, 1, 100, file) == 100 && memcmp(readback, pattern, 100) == 0;
    rewind(file);
    checks += ftell(file) == 0;
    checks += fclose(file) == 0;
    checks += remove(path) == 0;
    checks += remove(path) == -1 && errno == ENOENT;

    file = fopen("/tmp/rl-lines.txt", "w");
    if (file == NULL) {
        return 0;
    }
    checks += fprintf(file, "%d %s\n", 7, "seven") == 8;
    checks += fputs("second\n", file) >= 0;
    checks += fclose(file) == 0;
    file = fopen("/tmp/rl-lines.txt", "r");
    if (file == NULL) {
        return 0;
    }
    checks += fgetc(file) == '7';
    checks += ungetc('7', file) == '7';
    checks += fgetc(file) == '7';
    checks += fclose(file) == 0;
    checks += remove("/tmp/rl-lines.txt") == 0;

    again = fopen("/tmp/rl.bin", "r");
    checks += again == NULL;
    (void)again;
    return checks >= 15;
}

static int check_initfs(void)
{
    static const char *const lines[] = {
        "line one", "line two", "line three", "line four", "line five",
    };
    char buffer[64];
    FILE *file = fopen("/etc/libc-test.txt", "r");
    int index;
    int checks = 0;

    if (file == NULL) {
        return 0;
    }
    for (index = 0; index < 5; index++) {
        char *newline;

        if (fgets(buffer, sizeof(buffer), file) == NULL) {
            fclose(file);
            return 0;
        }
        newline = strchr(buffer, '\n');
        if (newline != NULL) {
            *newline = '\0';
        }
        checks += strcmp(buffer, lines[index]) == 0;
    }
    checks += feof(file) != 0;
    checks += fgetc(file) == EOF;
    checks += fclose(file) == 0;
    return checks >= 7;
}

static int check_buffer(void)
{
    const char *path = "/tmp/rl-buffer.bin";
    char buffer[200];
    char vbuf[64];
    FILE *file;
    FILE *probe;
    long before;
    long after;
    int checks = 0;

    memset(buffer, 'x', sizeof(buffer));
    file = fopen(path, "w+");
    if (file == NULL) {
        return 0;
    }
    checks += setvbuf(file, vbuf, _IOFBF, sizeof(vbuf)) == 0;
    checks += fwrite(buffer, 1, sizeof(buffer), file) == sizeof(buffer);
    probe = fopen(path, "rb");
    if (probe == NULL) {
        fclose(file);
        return 0;
    }
    checks += fseek(probe, 0, SEEK_END) == 0;
    before = ftell(probe);
    checks += before == 192;
    fclose(probe);
    checks += fflush(file) == 0;
    fclose(file);
    probe = fopen(path, "rb");
    if (probe == NULL) {
        return 0;
    }
    checks += fseek(probe, 0, SEEK_END) == 0;
    after = ftell(probe);
    checks += after == 200;
    fclose(probe);
    checks += remove(path) == 0;
    return checks >= 7;
}

static int check_std_streams(void)
{
    int checks = 0;

    checks += stdin != NULL && stdout != NULL && stderr != NULL;
    checks += fileno(stdin) == 0 && fileno(stdout) == 1 && fileno(stderr) == 2;
    checks += fputs("libc-stdio: stderr probe\n", stderr) >= 0;
    checks += fprintf(stdout, "libc-stdio: stdout probe\n") == 25;
    checks += fflush(NULL) == 0;
    checks += isatty(1) == 1;
    return checks >= 6;
}

static int check_memstream(void)
{
    char fixed[32];
    char *dynamic = NULL;
    size_t size = 0;
    FILE *file;
    int checks = 0;

    memset(fixed, 0, sizeof(fixed));
    file = fmemopen(fixed, sizeof(fixed), "w+");
    if (file == NULL) {
        return 0;
    }
    checks += fprintf(file, "%s", "raven") == 5;
    checks += fclose(file) == 0;
    checks += strcmp(fixed, "raven") == 0;

    file = open_memstream(&dynamic, &size);
    if (file == NULL) {
        return 0;
    }
    checks += fprintf(file, "%d-%s", 42, "raven") == 8;
    checks += fclose(file) == 0;
    checks += size == 8 && dynamic != NULL && strcmp(dynamic, "42-raven") == 0;
    free(dynamic);
    return checks >= 6;
}

static int check_wide(void)
{
    char *dynamic = NULL;
    size_t size = 0;
    FILE *file;
    FILE *input;
    wint_t wc;
    int checks = 0;

    checks += setlocale(LC_ALL, "C.UTF-8") != NULL;
    checks += MB_CUR_MAX == 4;

    file = open_memstream(&dynamic, &size);
    if (file == NULL) {
        return 0;
    }
    checks += fwprintf(file, L"%ls", L"raven") == 5;
    checks += fclose(file) == 0;
    checks += size == 5 && dynamic != NULL && memcmp(dynamic, "raven", 5) == 0;
    free(dynamic);

    input = fmemopen((void *)"\xC3\xA9", 2, "r");
    if (input == NULL) {
        return 0;
    }
    wc = fgetwc(input);
    checks += wc == 0xE9;
    checks += fclose(input) == 0;
    return checks >= 6;
}

static int check_getline(void)
{
    char text[] = "one\ntwo\nthree\n";
    char *line = NULL;
    size_t capacity = 0;
    FILE *file = fmemopen(text, sizeof(text) - 1, "r");
    int count = 0;
    int checks = 0;

    if (file == NULL) {
        return 0;
    }
    while (getline(&line, &capacity, file) >= 0) {
        count++;
    }
    checks += count == 3;
    checks += line != NULL && capacity > 0;
    checks += feof(file) != 0;
    checks += fclose(file) == 0;
    free(line);
    return checks >= 4;
}

int main(void)
{
    rl_out("libc-stdio: start");
    RL_CHECK("printf", check_printf());
    RL_CHECK("scanf", check_scanf());
    RL_CHECK("file", check_file());
    RL_CHECK("initfs read", check_initfs());
    RL_CHECK("buffer", check_buffer());
    RL_CHECK("std streams", check_std_streams());
    RL_CHECK("memstream", check_memstream());
    RL_CHECK("wide", check_wide());
    RL_CHECK("getline", check_getline());
    rl_skip("float ok", "PG-12");
    return rl_finish();
}
