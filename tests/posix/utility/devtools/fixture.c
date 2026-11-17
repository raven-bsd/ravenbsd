/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#define LIMIT 10

struct point {
    int x;
    int y;
};

enum color { RED, GREEN };

int global_count = 0;

static int helper(int value) {
    return value + LIMIT;
}

int add(int left, int right) {
    return left + right;
}

int main(void) {
    int total = add(1, 2);
    total += helper(total);
    global_count = total;
    return total;
}
