/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S188: POSIX pthread key destructor rules, red-first.
 *
 * Thread 1 sets key A and key B; A's destructor reads B. POSIX sets each key to NULL only
 * immediately before its own destructor, so A must see B's 42 and must not fault.
 * Thread 2's destructor re-sets its own key; the destructor must run PTHREAD_DESTRUCTOR_ITERATIONS
 * (4) times and then stop.
 *
 * The fixture runs as /bin/libc-test under the libc-test profile. */

#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define ITERATIONS 4

static pthread_key_t key_a;
static pthread_key_t key_b;
static pthread_key_t key_self;

static volatile int a_ran;
static volatile int b_ran;
static volatile int self_ran;
static volatile int a_read_ok;
static volatile int a_read_value;

static void a_dtor(void *value) {
    void *other;

    a_ran++;
    other = pthread_getspecific(key_b);
    a_read_value = (int)(intptr_t)other;
    a_read_ok = a_read_value == 42;
    printf("tls-keys: a dtor value=%ld b=%ld\n", (long)(intptr_t)value,
           (long)(intptr_t)other);
    fflush(stdout);
}

static void b_dtor(void *value) {
    b_ran++;
    printf("tls-keys: b dtor value=%ld\n", (long)(intptr_t)value);
    fflush(stdout);
}

static void self_dtor(void *value) {
    self_ran++;
    printf("tls-keys: self dtor count=%d value=%ld\n", self_ran, (long)(intptr_t)value);
    fflush(stdout);
    if (self_ran < ITERATIONS)
        (void)pthread_setspecific(key_self, (void *)(intptr_t)1);
}

static void *reader_thread(void *argument) {
    (void)argument;
    if (pthread_setspecific(key_a, (void *)(intptr_t)7) != 0)
        return (void *)(intptr_t)1;
    if (pthread_setspecific(key_b, (void *)(intptr_t)42) != 0)
        return (void *)(intptr_t)1;
    pthread_exit(NULL);
}

static void *self_thread(void *argument) {
    (void)argument;
    if (pthread_setspecific(key_self, (void *)(intptr_t)1) != 0)
        return (void *)(intptr_t)1;
    pthread_exit(NULL);
}

int main(void) {
    pthread_t thread;
    void *result = NULL;
    int failed = 0;

    setvbuf(stdout, NULL, _IONBF, 0);

    if (pthread_key_create(&key_a, a_dtor) != 0 ||
        pthread_key_create(&key_b, b_dtor) != 0 ||
        pthread_key_create(&key_self, self_dtor) != 0) {
        printf("tls-keys: key-create errno=%d\n", errno);
        return 1;
    }
    if (pthread_create(&thread, NULL, reader_thread, NULL) != 0) {
        printf("tls-keys: reader-create errno=%d\n", errno);
        return 1;
    }
    if (pthread_join(thread, &result) != 0 || result != NULL) {
        printf("tls-keys: reader-join failed\n");
        return 1;
    }
    if (a_ran != 1 || b_ran != 1 || !a_read_ok) {
        printf("tls-keys: reader FAIL a=%d b=%d read=%d b-value=%d\n", a_ran, b_ran,
               (int)a_read_ok, a_read_value);
        failed = 1;
    } else {
        printf("tls-keys: reader ok a=%d b=%d b-value=%d\n", a_ran, b_ran, a_read_value);
    }

    if (pthread_create(&thread, NULL, self_thread, NULL) != 0) {
        printf("tls-keys: self-create errno=%d\n", errno);
        return 1;
    }
    if (pthread_join(thread, &result) != 0 || result != NULL) {
        printf("tls-keys: self-join failed\n");
        return 1;
    }
    if (self_ran != ITERATIONS) {
        printf("tls-keys: self FAIL runs=%d expected=%d\n", self_ran, ITERATIONS);
        failed = 1;
    } else {
        printf("tls-keys: self ok runs=%d\n", self_ran);
    }

    printf("tls-keys: %s\n", failed ? "FAIL" : "PASS");
    return failed;
}
