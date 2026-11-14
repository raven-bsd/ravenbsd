// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P8: qsort + bsearch + strtol + regcomp/regexec. Regex weight.
#include <regex.h>
#include <stdlib.h>

static int compare(const void *left, const void *right) {
    int a = *(const int *)left;
    int b = *(const int *)right;
    return (a > b) - (a < b);
}

int main(int argc, char **argv) {
    int values[3] = {3, 1, 2};
    char *end = 0;
    regex_t regex;
    qsort(values, 3, sizeof values[0], compare);
    (void)bsearch(&values[0], values, 3, sizeof values[0], compare);
    (void)strtol(argc > 0 ? argv[0] : "1", &end, 10);
    if (regcomp(&regex, "a+", REG_EXTENDED) == 0) {
        (void)regexec(&regex, "aaa", 0, 0, 0);
        regfree(&regex);
    }
    return argc;
}
