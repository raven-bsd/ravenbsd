// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S85d malloc/stdio concurrency stress: several threads allocate/free blocks of varying sizes
// and append lines to one stdio file under `flockfile`, then the main thread joins them and
// re-reads every line. It runs on the qtest harness's fixed `-smp 4`.

#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define THREADS 8
#define ITERATIONS 2000
#define EXPECTED_LINES (THREADS * ITERATIONS)

static FILE *log_file;

static void *worker(void *argument) {
    long id = (long)argument;
    char line[128];

    for (int index = 0; index < ITERATIONS; index++) {
        size_t size = 1 + (size_t)((id * 37 + index * 17) % 4096);
        unsigned char *block = malloc(size);
        unsigned char value = (unsigned char)(id + index);
        if (block == NULL) {
            return (void *)1;
        }
        memset(block, value, size);
        if (block[0] != value || block[size - 1] != value) {
            free(block);
            return (void *)2;
        }
        free(block);

        int length = snprintf(line, sizeof(line), "t%ld i%d size%zu\n", id, index, size);
        if (length <= 0 || (size_t)length >= sizeof(line)) {
            return (void *)3;
        }
        flockfile(log_file);
        if (fputs(line, log_file) < 0) {
            int saved = errno;
            funlockfile(log_file);
            printf("smp-libc-concurrency: worker id=%ld fputs errno=%d\n", id, saved);
            return (void *)4;
        }
        funlockfile(log_file);
    }
    return NULL;
}

int main(void) {
    pthread_t threads[THREADS];
    long identifiers[THREADS];
    int failures = 0;

    printf("smp-libc-concurrency: start\n");
    log_file = fopen("/tmp/smp-libc-concurrency.log", "w");
    if (log_file == NULL) {
        printf("smp-libc-concurrency: open failed\n");
        return 1;
    }
    for (long index = 0; index < THREADS; index++) {
        identifiers[index] = index;
        int result = pthread_create(&threads[index], NULL, worker, (void *)identifiers[index]);
        if (result != 0) {
            printf("smp-libc-concurrency: create id=%ld rc=%d\n", index, result);
            failures++;
        }
    }
    for (int index = 0; index < THREADS; index++) {
        void *result = NULL;
        int joined = pthread_join(threads[index], &result);
        if (joined != 0) {
            printf("smp-libc-concurrency: join id=%d rc=%d\n", index, joined);
            failures++;
        } else if (result != NULL) {
            printf("smp-libc-concurrency: worker id=%d code=%ld\n", index, (long)result);
            failures++;
        }
    }
    fclose(log_file);
    log_file = NULL;
    if (failures != 0) {
        printf("smp-libc-concurrency: FAIL threads=%d\n", failures);
        return 1;
    }
    printf("smp-libc-concurrency: malloc ok\n");

    FILE *reader = fopen("/tmp/smp-libc-concurrency.log", "r");
    if (reader == NULL) {
        printf("smp-libc-concurrency: read open failed\n");
        return 1;
    }
    int lines = 0;
    char buffer[256];
    while (fgets(buffer, sizeof(buffer), reader) != NULL) {
        lines++;
    }
    fclose(reader);
    if (lines != EXPECTED_LINES) {
        printf("smp-libc-concurrency: FAIL lines=%d expected=%d\n", lines, EXPECTED_LINES);
        return 1;
    }
    printf("smp-libc-concurrency: stdio ok lines=%d\n", lines);
    printf("smp-libc-concurrency: done\n");
    return 0;
}
