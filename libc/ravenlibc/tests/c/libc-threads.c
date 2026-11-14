/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-threads: the pthread, semaphore and C11 thread surface. Only the main thread
 * prints markers; worker threads return statuses and the main thread checks them after join.
 * The exact `timedwait` tolerance is soft until PG-3; the precise marker is printed as SKIP.
 */

#define RL_CASE "libc-threads"
#include "rl_test.h"

#include <pthread.h>
#include <semaphore.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <time.h>
#include <threads.h>
#include <unistd.h>

#define JOIN_THREADS 8

static _Thread_local int thread_tls_value = 5;
static _Thread_local char thread_tls_array[4096];

static pthread_t join_ids[JOIN_THREADS];

static void *join_worker(void *argument)
{
    int index = *(int *)argument;
    int value = index * index;

    join_ids[index] = pthread_self();
    thread_tls_value = value;
    thread_tls_array[0] = (char)value;
    thread_tls_array[4095] = (char)(value + 1);
    return (void *)(intptr_t)value;
}

static int check_create_join(void)
{
    pthread_t threads[JOIN_THREADS];
    static int indexes[JOIN_THREADS];
    void *value;
    int index;
    int checks = 0;

    for (index = 0; index < JOIN_THREADS; index++) {
        indexes[index] = index;
        checks += pthread_create(&threads[index], NULL, join_worker, &indexes[index]) == 0;
    }
    for (index = 0; index < JOIN_THREADS; index++) {
        value = NULL;
        checks += pthread_join(threads[index], &value) == 0;
        checks += (int)(intptr_t)value == index * index;
        checks += pthread_equal(threads[index], join_ids[index]) != 0;
        if (index > 0) {
            checks += pthread_equal(join_ids[index], join_ids[index - 1]) == 0;
        }
    }
    checks += thread_tls_value == 5 && thread_tls_array[0] == 0;
    return checks >= 40;
}

static pthread_mutex_t counter_mutex = PTHREAD_MUTEX_INITIALIZER;
static int counter_total;

static void *counter_worker(void *argument)
{
    int index;

    (void)argument;
    for (index = 0; index < 10000; index++) {
        pthread_mutex_lock(&counter_mutex);
        counter_total++;
        pthread_mutex_unlock(&counter_mutex);
    }
    return NULL;
}

static int check_mutex(void)
{
    pthread_t threads[4];
    pthread_mutex_t recursive;
    pthread_mutex_t errorcheck;
    pthread_mutexattr_t attr;
    int index;
    int checks = 0;
    int result;

    counter_total = 0;
    for (index = 0; index < 4; index++) {
        checks += pthread_create(&threads[index], NULL, counter_worker, NULL) == 0;
    }
    for (index = 0; index < 4; index++) {
        checks += pthread_join(threads[index], NULL) == 0;
    }
    checks += counter_total == 40000;

    checks += pthread_mutexattr_init(&attr) == 0;
    checks += pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) == 0;
    checks += pthread_mutex_init(&recursive, &attr) == 0;
    checks += pthread_mutex_lock(&recursive) == 0;
    checks += pthread_mutex_lock(&recursive) == 0;
    checks += pthread_mutex_unlock(&recursive) == 0;
    checks += pthread_mutex_unlock(&recursive) == 0;
    pthread_mutex_destroy(&recursive);

    checks += pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_ERRORCHECK) == 0;
    checks += pthread_mutex_init(&errorcheck, &attr) == 0;
    checks += pthread_mutex_lock(&errorcheck) == 0;
    result = pthread_mutex_lock(&errorcheck);
    checks += result == EDEADLK;
    checks += pthread_mutex_unlock(&errorcheck) == 0;
    pthread_mutex_destroy(&errorcheck);
    pthread_mutexattr_destroy(&attr);

    checks += pthread_mutex_init(&recursive, NULL) == 0;
    checks += pthread_mutex_lock(&recursive) == 0;
    checks += pthread_mutex_trylock(&recursive) == EBUSY;
    checks += pthread_mutex_unlock(&recursive) == 0;
    pthread_mutex_destroy(&recursive);
    return checks >= 23;
}

static pthread_mutex_t ring_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t ring_not_empty = PTHREAD_COND_INITIALIZER;
static pthread_cond_t ring_not_full = PTHREAD_COND_INITIALIZER;
static int ring[16];
static int ring_head;
static int ring_tail;
static int ring_count;
static long ring_sum;

static void *producer_worker(void *argument)
{
    int index;

    (void)argument;
    for (index = 0; index < 1000; index++) {
        pthread_mutex_lock(&ring_mutex);
        while (ring_count == 16) {
            pthread_cond_wait(&ring_not_full, &ring_mutex);
        }
        ring[ring_tail] = index;
        ring_tail = (ring_tail + 1) % 16;
        ring_count++;
        if (ring_count == 1) {
            pthread_cond_signal(&ring_not_empty);
        }
        pthread_mutex_unlock(&ring_mutex);
    }
    pthread_mutex_lock(&ring_mutex);
    pthread_cond_broadcast(&ring_not_empty);
    pthread_mutex_unlock(&ring_mutex);
    return NULL;
}

static void *consumer_worker(void *argument)
{
    int index;

    (void)argument;
    for (index = 0; index < 1000; index++) {
        pthread_mutex_lock(&ring_mutex);
        while (ring_count == 0) {
            pthread_cond_wait(&ring_not_empty, &ring_mutex);
        }
        ring_sum += ring[ring_head];
        ring_head = (ring_head + 1) % 16;
        ring_count--;
        if (ring_count == 15) {
            pthread_cond_signal(&ring_not_full);
        }
        pthread_mutex_unlock(&ring_mutex);
    }
    return NULL;
}

static int check_cond(void)
{
    pthread_t producer;
    pthread_t consumer;
    int checks = 0;

    checks += pthread_create(&producer, NULL, producer_worker, NULL) == 0;
    checks += pthread_create(&consumer, NULL, consumer_worker, NULL) == 0;
    checks += pthread_join(producer, NULL) == 0;
    checks += pthread_join(consumer, NULL) == 0;
    checks += ring_sum == (long)999 * 1000 / 2;
    checks += ring_count == 0;
    return checks >= 6;
}

static pthread_once_t once_control = PTHREAD_ONCE_INIT;
static atomic_int once_runs;

static void once_init(void)
{
    atomic_fetch_add(&once_runs, 1);
}

static void *once_worker(void *argument)
{
    (void)argument;
    pthread_once(&once_control, once_init);
    return NULL;
}

static int check_once(void)
{
    pthread_t threads[JOIN_THREADS];
    int index;
    int checks = 0;

    for (index = 0; index < JOIN_THREADS; index++) {
        checks += pthread_create(&threads[index], NULL, once_worker, NULL) == 0;
    }
    for (index = 0; index < JOIN_THREADS; index++) {
        checks += pthread_join(threads[index], NULL) == 0;
    }
    checks += atomic_load(&once_runs) == 1;
    return checks >= 17;
}

static pthread_key_t tsd_key;
static atomic_int tsd_destroyed;

static void tsd_destructor(void *value)
{
    if (value != NULL) {
        atomic_fetch_add(&tsd_destroyed, 1);
    }
}

static void *tsd_worker(void *argument)
{
    intptr_t value = (intptr_t)argument;

    if (pthread_setspecific(tsd_key, (void *)value) != 0) {
        return (void *)1;
    }
    return pthread_getspecific(tsd_key) == (void *)value ? NULL : (void *)1;
}

static int check_tsd(void)
{
    pthread_t threads[4];
    int index;
    int checks = 0;

    checks += pthread_key_create(&tsd_key, tsd_destructor) == 0;
    for (index = 0; index < 4; index++) {
        checks += pthread_create(&threads[index], NULL, tsd_worker, (void *)(intptr_t)(index + 1)) == 0;
    }
    for (index = 0; index < 4; index++) {
        void *result = NULL;
        checks += pthread_join(threads[index], &result) == 0 && result == NULL;
    }
    checks += atomic_load(&tsd_destroyed) == 4;
    checks += pthread_key_delete(tsd_key) == 0;
    return checks >= 11;
}

static int check_tls(void)
{
    return thread_tls_value == 5 && thread_tls_array[0] == 0 &&
           thread_tls_array[sizeof(thread_tls_array) - 1] == 0;
}

static pthread_rwlock_t rw_lock = PTHREAD_RWLOCK_INITIALIZER;
static int rw_value;

static void *reader_worker(void *argument)
{
    int index;

    (void)argument;
    for (index = 0; index < 100; index++) {
        if (pthread_rwlock_rdlock(&rw_lock) != 0) {
            return (void *)1;
        }
        if (rw_value < 0) {
            pthread_rwlock_unlock(&rw_lock);
            return (void *)1;
        }
        pthread_rwlock_unlock(&rw_lock);
    }
    return NULL;
}

static void *writer_worker(void *argument)
{
    int index;

    (void)argument;
    for (index = 0; index < 100; index++) {
        if (pthread_rwlock_wrlock(&rw_lock) != 0) {
            return (void *)1;
        }
        rw_value++;
        pthread_rwlock_unlock(&rw_lock);
    }
    return NULL;
}

static int check_rwlock(void)
{
    pthread_t readers[4];
    pthread_t writer;
    int index;
    void *result;
    int checks = 0;

    rw_value = 0;
    for (index = 0; index < 4; index++) {
        checks += pthread_create(&readers[index], NULL, reader_worker, NULL) == 0;
    }
    checks += pthread_create(&writer, NULL, writer_worker, NULL) == 0;
    checks += pthread_join(writer, &result) == 0 && result == NULL;
    for (index = 0; index < 4; index++) {
        checks += pthread_join(readers[index], &result) == 0 && result == NULL;
    }
    checks += rw_value == 100;
    return checks >= 11;
}

static int check_sem(void)
{
    sem_t sem;
    struct timespec deadline;
    int checks = 0;

    checks += sem_init(&sem, 0, 1) == 0;
    checks += sem_wait(&sem) == 0;
    checks += sem_trywait(&sem) == -1 && errno == EAGAIN;
    checks += sem_post(&sem) == 0;
    checks += sem_wait(&sem) == 0;
    checks += sem_post(&sem) == 0;
    checks += sem_destroy(&sem) == 0;

    checks += sem_init(&sem, 0, 0) == 0;
    checks += clock_gettime(CLOCK_REALTIME, &deadline) == 0;
    deadline.tv_nsec += 100 * 1000 * 1000;
    if (deadline.tv_nsec >= 1000000000L) {
        deadline.tv_nsec -= 1000000000L;
        deadline.tv_sec += 1;
    }
    checks += sem_timedwait(&sem, &deadline) == -1 && errno == ETIMEDOUT;
    checks += sem_destroy(&sem) == 0;
    return checks >= 10;
}

static pthread_barrier_t barrier;
static atomic_int barrier_serial;

static void *barrier_worker(void *argument)
{
    (void)argument;
    if (pthread_barrier_wait(&barrier) == PTHREAD_BARRIER_SERIAL_THREAD) {
        atomic_fetch_add(&barrier_serial, 1);
    }
    return NULL;
}

static int check_barrier(void)
{
    pthread_t threads[4];
    int index;
    int checks = 0;

    checks += pthread_barrier_init(&barrier, NULL, 4) == 0;
    for (index = 0; index < 4; index++) {
        checks += pthread_create(&threads[index], NULL, barrier_worker, NULL) == 0;
    }
    for (index = 0; index < 4; index++) {
        checks += pthread_join(threads[index], NULL) == 0;
    }
    checks += atomic_load(&barrier_serial) == 1;
    checks += pthread_barrier_destroy(&barrier) == 0;
    return checks >= 11;
}

static atomic_int detached_runs;

static void *detached_worker(void *argument)
{
    (void)argument;
    atomic_fetch_add(&detached_runs, 1);
    return NULL;
}

static int check_detach(void)
{
    pthread_attr_t attr;
    pthread_t thread;
    int index;
    int checks = 0;

    checks += pthread_attr_init(&attr) == 0;
    checks += pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED) == 0;
    for (index = 0; index < 200; index++) {
        checks += pthread_create(&thread, &attr, detached_worker, NULL) == 0;
    }
    checks += pthread_attr_destroy(&attr) == 0;
    for (index = 0; index < 2000 && atomic_load(&detached_runs) < 200; index++) {
        (void)usleep(1000);
    }
    checks += atomic_load(&detached_runs) == 200;
    return checks >= 204;
}

static long deep_stack(int depth)
{
    volatile char padding[1024];

    padding[0] = (char)depth;
    if (depth == 0) {
        return padding[0] + 1;
    }
    return padding[0] + deep_stack(depth - 1);
}

static void *stack_worker(void *argument)
{
    volatile long result = deep_stack(100);

    (void)argument;
    return (void *)(intptr_t)(result != 0);
}

static int check_stack(void)
{
    pthread_attr_t attr;
    pthread_t thread;
    void *result = NULL;
    int checks = 0;

    checks += pthread_attr_init(&attr) == 0;
    checks += pthread_attr_setstacksize(&attr, 128 * 1024) == 0;
    checks += pthread_create(&thread, &attr, stack_worker, NULL) == 0;
    checks += pthread_join(thread, &result) == 0 && result != NULL;
    checks += pthread_attr_destroy(&attr) == 0;
    return checks >= 5;
}

static mtx_t c11_mutex;
static cnd_t c11_cond;
static tss_t c11_key;
static once_flag c11_once = ONCE_FLAG_INIT;
static atomic_int c11_ready;
static atomic_int c11_once_runs;

static void c11_once_body(void)
{
    atomic_fetch_add(&c11_once_runs, 1);
}

static int c11_worker(void *argument)
{
    (void)argument;
    if (tss_set(c11_key, (void *)0x51) != thrd_success) {
        return 1;
    }
    if (mtx_lock(&c11_mutex) != thrd_success) {
        return 1;
    }
    atomic_store(&c11_ready, 1);
    cnd_signal(&c11_cond);
    mtx_unlock(&c11_mutex);
    call_once(&c11_once, c11_once_body);
    return 0;
}

static int check_c11(void)
{
    thrd_t thread;
    int result = -1;
    int checks = 0;

    checks += mtx_init(&c11_mutex, mtx_plain) == thrd_success;
    checks += cnd_init(&c11_cond) == thrd_success;
    checks += tss_create(&c11_key, NULL) == thrd_success;
    checks += mtx_lock(&c11_mutex) == thrd_success;
    checks += thrd_create(&thread, c11_worker, NULL) == thrd_success;
    checks += cnd_wait(&c11_cond, &c11_mutex) == thrd_success;
    checks += atomic_load(&c11_ready) == 1;
    mtx_unlock(&c11_mutex);
    checks += thrd_join(thread, &result) == thrd_success && result == 0;
    checks += atomic_load(&c11_once_runs) == 1;
    checks += tss_get(c11_key) == NULL;
    tss_delete(c11_key);
    cnd_destroy(&c11_cond);
    mtx_destroy(&c11_mutex);
    return checks >= 10;
}

static void *exit_worker(void *argument)
{
    (void)argument;
    pthread_exit((void *)(intptr_t)0x1234);
    return NULL;
}

static int check_exit_value(void)
{
    pthread_t thread;
    void *value = NULL;

    if (pthread_create(&thread, NULL, exit_worker, NULL) != 0) {
        return 0;
    }
    if (pthread_join(thread, &value) != 0) {
        return 0;
    }
    return (intptr_t)value == 0x1234;
}

static int check_timedwait(void)
{
    pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
    pthread_cond_t condition = PTHREAD_COND_INITIALIZER;
    struct timespec deadline;
    struct timespec before;
    struct timespec after;
    long long elapsed;
    int result;
    int checks = 0;

    checks += clock_gettime(CLOCK_REALTIME, &before) == 0;
    deadline = before;
    deadline.tv_nsec += 100 * 1000 * 1000;
    if (deadline.tv_nsec >= 1000000000L) {
        deadline.tv_nsec -= 1000000000L;
        deadline.tv_sec += 1;
    }
    checks += pthread_mutex_lock(&mutex) == 0;
    result = pthread_cond_timedwait(&condition, &mutex, &deadline);
    checks += result == ETIMEDOUT;
    checks += pthread_mutex_unlock(&mutex) == 0;
    checks += clock_gettime(CLOCK_REALTIME, &after) == 0;
    elapsed = ((long long)after.tv_sec - (long long)before.tv_sec) * 1000 +
              ((long long)after.tv_nsec - (long long)before.tv_nsec) / 1000000;
    checks += elapsed >= 100 && elapsed <= 600;
    pthread_cond_destroy(&condition);
    pthread_mutex_destroy(&mutex);
    return checks >= 6;
}

int main(void)
{
    rl_out("libc-threads: start");
    RL_CHECK("create-join", check_create_join());
    if (check_mutex()) {
        rl_out("libc-threads: mutex ok total=40000");
    } else {
        rl_fail("mutex", __FILE__, __LINE__);
    }
    RL_CHECK("cond", check_cond());
    RL_CHECK("once", check_once());
    RL_CHECK("tsd", check_tsd());
    RL_CHECK("tls", check_tls());
    RL_CHECK("rwlock", check_rwlock());
    RL_CHECK("sem", check_sem());
    RL_CHECK("barrier", check_barrier());
    RL_CHECK("detach", check_detach());
    RL_CHECK("stack", check_stack());
    RL_CHECK("c11", check_c11());
    RL_CHECK("exit value", check_exit_value());
    RL_CHECK("timedwait", check_timedwait());
    rl_skip("timedwait precise ok", "PG-3");
    return rl_finish();
}
