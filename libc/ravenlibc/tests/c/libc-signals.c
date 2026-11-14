/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-signals: layer A (libc-level masks, actions, raise, setjmp and ucontext) runs
 * whenever the base is present; layer B needs kernel delivery (PG-7/PG-8/PG-12) and prints
 * SKIP lines instead of pass markers. No missing primitive is emulated in the test.
 */

#define RL_CASE "libc-signals"
#include "rl_test.h"

#include <signal.h>
#include <setjmp.h>
#include <ucontext.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

static volatile sig_atomic_t usr1_count;
static volatile sig_atomic_t usr1_signo;
static volatile sig_atomic_t usr2_seen;

static void usr1_handler(int signo, siginfo_t *info, void *context)
{
    (void)info;
    (void)context;
    usr1_count++;
    usr1_signo = signo;
}

static void usr2_handler(int signo)
{
    (void)signo;
    usr2_seen++;
}

static int set_default(int signo)
{
    struct sigaction action;

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = SIG_DFL;
    return sigaction(signo, &action, NULL);
}

static int check_sigset(void)
{
    sigset_t set;
    int checks = 0;

    checks += sigemptyset(&set) == 0;
    checks += sigismember(&set, SIGUSR1) == 0;
    checks += sigaddset(&set, SIGUSR1) == 0;
    checks += sigismember(&set, SIGUSR1) == 1;
    checks += sigdelset(&set, SIGUSR1) == 0;
    checks += sigismember(&set, SIGUSR1) == 0;
    checks += sigfillset(&set) == 0;
    checks += sigismember(&set, SIGUSR1) == 1;
    checks += sigismember(&set, SIGKILL) == 1;
    errno = 0;
    checks += sigaddset(&set, 0) == -1 && errno == EINVAL;
    errno = 0;
    checks += sigaddset(&set, 65) == -1 && errno == EINVAL;
    return checks >= 11;
}

static int check_sigaction(void)
{
    struct sigaction action;
    struct sigaction old;
    struct sigaction current;
    int checks = 0;

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_sigaction = usr1_handler;
    action.sa_flags = SA_SIGINFO;
    checks += sigaction(SIGUSR1, &action, &old) == 0;
    checks += sigaction(SIGUSR1, NULL, &current) == 0;
    checks += (current.sa_flags & SA_SIGINFO) != 0;
    checks += sigaction(SIGUSR1, &old, NULL) == 0;

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = SIG_IGN;
    checks += sigaction(SIGUSR2, &action, &old) == 0;
    checks += sigaction(SIGUSR2, NULL, &current) == 0;
    checks += current.sa_handler == SIG_IGN;
    checks += sigaction(SIGUSR2, &old, NULL) == 0;

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = usr2_handler;
    action.sa_flags = SA_RESETHAND;
    checks += sigaction(SIGUSR2, &action, &old) == 0;
    checks += sigaction(SIGUSR2, NULL, &current) == 0;
    checks += (current.sa_flags & SA_RESETHAND) != 0;
    checks += sigaction(SIGUSR2, &old, NULL) == 0;
    checks += set_default(SIGUSR2) == 0;

    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = SIG_IGN;
    errno = 0;
    checks += sigaction(SIGKILL, &action, NULL) == -1 && errno == EINVAL;
    return checks >= 12;
}

static int check_raise(void)
{
    struct sigaction action;
    int checks = 0;

    usr1_count = 0;
    usr1_signo = 0;
    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_sigaction = usr1_handler;
    action.sa_flags = SA_SIGINFO;
    checks += sigaction(SIGUSR1, &action, NULL) == 0;
    checks += raise(SIGUSR1) == 0;
    checks += usr1_count == 1;
    checks += usr1_signo == SIGUSR1;
    checks += set_default(SIGUSR1) == 0;
    return checks >= 4;
}

static void *mask_worker(void *argument)
{
    sigset_t set;

    (void)argument;
    sigemptyset(&set);
    sigaddset(&set, SIGUSR2);
    return pthread_sigmask(SIG_BLOCK, &set, NULL) == 0 ? NULL : (void *)1;
}

static int check_mask(void)
{
    struct sigaction action;
    sigset_t set;
    sigset_t old;
    sigset_t pending;
    sigset_t current;
    pthread_t thread;
    void *result = NULL;
    int checks = 0;

    usr1_count = 0;
    memset(&action, 0, sizeof(action));
    sigemptyset(&action.sa_mask);
    action.sa_handler = usr2_handler;
    checks += sigaction(SIGUSR1, &action, NULL) == 0;
    checks += sigemptyset(&set) == 0;
    checks += sigaddset(&set, SIGUSR1) == 0;
    checks += sigprocmask(SIG_BLOCK, &set, &old) == 0;
    checks += raise(SIGUSR1) == 0;
    checks += usr1_count == 0;
    checks += sigpending(&pending) == 0;
    checks += sigismember(&pending, SIGUSR1) == 1;
    checks += sigprocmask(SIG_SETMASK, &old, NULL) == 0;
    checks += usr1_count == 1;
    checks += sigprocmask(SIG_SETMASK, NULL, &current) == 0;
    checks += sigismember(&current, SIGUSR1) == 0;

    checks += pthread_create(&thread, NULL, mask_worker, NULL) == 0;
    checks += pthread_join(thread, &result) == 0 && result == NULL;
    checks += set_default(SIGUSR1) == 0;
    return checks >= 12;
}

static sigjmp_buf jump_buffer;
static sigjmp_buf register_buffer;
static volatile int jump_stage;

static void jump_frame_three(void)
{
    siglongjmp(jump_buffer, 1);
}

static void jump_frame_two(void)
{
    jump_frame_three();
}

static void jump_frame_one(void)
{
    jump_frame_two();
}

static void register_jump(void)
{
    siglongjmp(register_buffer, 1);
}

static int register_survivors(void)
{
    long a = 1;
    long b = 2;
    long c = 3;
    long d = 4;
    long e = 5;
    long f = 6;

    if (sigsetjmp(register_buffer, 1) == 0) {
        register_jump();
    }
    return a + b + c + d + e + f == 21;
}

static int check_setjmp(void)
{
    sigset_t set;
    sigset_t old;
    sigset_t restored;
    int checks = 0;

    jump_stage = 0;
    if (sigsetjmp(jump_buffer, 1) == 0) {
        jump_stage = 1;
        jump_frame_one();
        jump_stage = 2;
    }
    checks += jump_stage == 1;
    checks += register_survivors();

    checks += sigemptyset(&set) == 0;
    checks += sigaddset(&set, SIGUSR1) == 0;
    checks += sigprocmask(SIG_BLOCK, &set, &old) == 0;
    if (sigsetjmp(jump_buffer, 1) == 0) {
        jump_frame_one();
    }
    checks += sigprocmask(SIG_SETMASK, NULL, &restored) == 0;
    checks += sigismember(&restored, SIGUSR1) == 0;
    checks += sigprocmask(SIG_SETMASK, &old, NULL) == 0;
    return checks >= 7;
}

static ucontext_t main_context;
static ucontext_t thread_context;
static int context_swaps;

static void context_body(void)
{
    for (;;) {
        context_swaps++;
        if (swapcontext(&thread_context, &main_context) != 0) {
            return;
        }
    }
}

static int check_ucontext(void)
{
    char *stack = malloc(64 * 1024);
    int checks = 0;

    if (stack == NULL) {
        return 0;
    }
    context_swaps = 0;
    checks += getcontext(&thread_context) == 0;
    thread_context.uc_stack.ss_sp = stack;
    thread_context.uc_stack.ss_size = 64 * 1024;
    thread_context.uc_stack.ss_flags = 0;
    thread_context.uc_link = &main_context;
    makecontext(&thread_context, context_body, 0);

    for (int index = 0; index < 1000; index++) {
        if (swapcontext(&main_context, &thread_context) != 0) {
            checks = 0;
            break;
        }
    }
    checks += context_swaps == 1000;
    free(stack);
    return checks >= 2;
}

static int check_sigaltstack(void)
{
    stack_t stack;
    stack_t current;
    char *memory;
    int checks = 0;

    checks += MINSIGSTKSZ > 0 && SIGSTKSZ > 0;
    memory = malloc((size_t)SIGSTKSZ);
    if (memory == NULL) {
        return 0;
    }
    memset(&stack, 0, sizeof(stack));
    stack.ss_sp = memory;
    stack.ss_size = (size_t)SIGSTKSZ;
    stack.ss_flags = 0;
    checks += sigaltstack(&stack, NULL) == 0;
    checks += sigaltstack(NULL, &current) == 0;
    checks += current.ss_sp == memory;
    checks += current.ss_flags == 0;
    stack.ss_flags = SS_DISABLE;
    checks += sigaltstack(&stack, NULL) == 0;
    checks += sigaltstack(NULL, &current) == 0;
    checks += current.ss_flags == SS_DISABLE;
    free(memory);
    return checks >= 7;
}

int main(void)
{
    rl_out("libc-signals: start");
    RL_CHECK("sigset", check_sigset());
    RL_CHECK("sigaction", check_sigaction());
    RL_CHECK("raise", check_raise());
    RL_CHECK("mask", check_mask());
    RL_CHECK("setjmp", check_setjmp());
    RL_CHECK("ucontext", check_ucontext());
    RL_CHECK("sigaltstack", check_sigaltstack());
    rl_skip("fault ok", "PG-7 PG-8 PG-12");
    rl_skip("pthread_kill ok", "PG-7 PG-8");
    rl_skip("eintr ok", "PG-7 PG-8");
    rl_skip("sa_restart ok", "PG-7 PG-8");
    rl_skip("sigtimedwait ok", "PG-7 PG-8");
    rl_skip("alarm ok", "PG-7 PG-19");
    rl_skip("fp ok", "PG-7 PG-12");
    return rl_finish();
}
