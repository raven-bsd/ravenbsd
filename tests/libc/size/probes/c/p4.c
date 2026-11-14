// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P4: pthread_create + pthread_join.
#include <pthread.h>

static void *start(void *arg) {
    (void)arg;
    return 0;
}

int main(int argc, char **argv) {
    pthread_t thread;
    (void)argc;
    (void)argv;
    if (pthread_create(&thread, 0, start, 0) != 0) {
        return 1;
    }
    return pthread_join(thread, 0) == 0 ? 0 : 2;
}
