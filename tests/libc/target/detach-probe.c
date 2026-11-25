// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S85d detached-thread isolation probe. The full libc-threads case stops after `barrier ok` on
// TCG -smp 4 and never prints `detach ok`. This fixture separates three cases with markers:
//   1. create+join one joinable thread at a time (create path, live count 1);
//   2. create 200 detached threads with a 256 KiB stack (detached path, low memory);
//   3. create 200 detached threads with the default 8 MiB stack (detached path, high memory).
// The last marker names the hanging call or phase in the serial log.

#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>

#define PROBE_THREADS 200
#define PROBE_POLLS 2000
#define SMALL_STACK (256 * 1024)

static atomic_int thread_runs;

static void *probe_worker(void *argument) {
    (void)argument;
    atomic_fetch_add(&thread_runs, 1);
    return NULL;
}

static int run_joinable_phase(void) {
    pthread_t thread;
    int index;

    printf("detach-probe: phase1 start\n");
    for (index = 0; index < PROBE_THREADS; index++) {
        int rc;
        if (index % 25 == 0) {
            printf("detach-probe: phase1 create-begin index=%d\n", index);
        }
        rc = pthread_create(&thread, NULL, probe_worker, NULL);
        if (rc != 0) {
            printf("detach-probe: phase1 create-fail index=%d rc=%d\n", index, rc);
            return 1;
        }
        rc = pthread_join(thread, NULL);
        if (rc != 0) {
            printf("detach-probe: phase1 join-fail index=%d rc=%d\n", index, rc);
            return 1;
        }
    }
    printf("detach-probe: phase1 done\n");
    return 0;
}

static int run_detached_phase(const char *name, size_t stack_size) {
    pthread_attr_t attr;
    pthread_t thread;
    struct timespec pause;
    int index;
    int rc;
    int runs;

    printf("detach-probe: %s start stack=%zu\n", name, stack_size);
    atomic_store(&thread_runs, 0);
    rc = pthread_attr_init(&attr);
    printf("detach-probe: %s attr-init rc=%d\n", name, rc);
    if (rc != 0) {
        return 1;
    }
    rc = pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    if (rc == 0 && stack_size != 0) {
        rc = pthread_attr_setstacksize(&attr, stack_size);
    }
    printf("detach-probe: %s attr-setup rc=%d\n", name, rc);
    if (rc != 0) {
        return 1;
    }
    for (index = 0; index < PROBE_THREADS; index++) {
        if (index % 25 == 0) {
            printf("detach-probe: %s create-begin index=%d\n", name, index);
        }
        rc = pthread_create(&thread, &attr, probe_worker, NULL);
        if (rc != 0) {
            printf("detach-probe: %s create-fail index=%d rc=%d\n", name, index, rc);
            return 1;
        }
    }
    printf("detach-probe: %s creates-done count=%d\n", name, PROBE_THREADS);
    rc = pthread_attr_destroy(&attr);
    printf("detach-probe: %s attr-destroy rc=%d\n", name, rc);
    pause.tv_sec = 0;
    pause.tv_nsec = 1000 * 1000;
    for (index = 0; index < PROBE_POLLS && atomic_load(&thread_runs) < PROBE_THREADS; index++) {
        if (index % 500 == 0) {
            printf("detach-probe: %s wait index=%d runs=%d\n", name, index,
                   atomic_load(&thread_runs));
        }
        (void)nanosleep(&pause, NULL);
    }
    runs = atomic_load(&thread_runs);
    printf("detach-probe: %s final runs=%d\n", name, runs);
    if (runs != PROBE_THREADS) {
        printf("detach-probe: %s FAIL runs=%d expected=%d\n", name, runs, PROBE_THREADS);
        return 1;
    }
    printf("detach-probe: %s done\n", name);
    return 0;
}

int main(void) {
    printf("detach-probe: start\n");
    if (run_joinable_phase() != 0) {
        return 1;
    }
    if (run_detached_phase("phase2-small", SMALL_STACK) != 0) {
        return 1;
    }
    if (run_detached_phase("phase3-default", 0) != 0) {
        return 1;
    }
    printf("detach-probe: all-done\n");
    return 0;
}
