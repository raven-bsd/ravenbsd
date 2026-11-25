// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S185 C19 signal acceptance fixture. Test-only: it must fail on the pre-S44 SIG_SETUP image and
// pass unchanged after the kernel-owned sigframe/sigcode/SIGRETURN rewrite.

#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <ucontext.h>
#include <unistd.h>

static volatile sig_atomic_t outer_count;
static volatile sig_atomic_t nested_count;
static volatile int outer_sig;
static volatile int nested_sig;
static volatile uintptr_t interrupted_rip;
static volatile uintptr_t interrupted_rsp;
static volatile uintptr_t expected_rsp;
static volatile int after_raise;
static volatile int raise_nested;

static void fail_row(const char *row, long long value, int error) {
    printf("signal: fail %s value=%lld errno=%d\n", row, value, error);
}

static void nested_handler(int signal_number, siginfo_t *info, void *context) {
    (void)context;
    nested_count++;
    nested_sig = signal_number;
    if (info == NULL || info->si_signo != signal_number) {
        nested_sig = -1;
    }
}

static void outer_handler(int signal_number, siginfo_t *info, void *context) {
    ucontext_t *ucontext = (ucontext_t *)context;
    outer_count++;
    outer_sig = signal_number;
    if (info == NULL || info->si_signo != signal_number) {
        outer_sig = -1;
    }
    if (ucontext != NULL) {
        interrupted_rip = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RIP];
        interrupted_rsp = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RSP];
    } else {
        interrupted_rip = 0;
        interrupted_rsp = 0;
    }
    if (raise_nested != 0) {
        raise_nested = 0;
        if (raise(SIGUSR2) != 0) {
            nested_sig = -1;
        }
    }
}

static int install_handlers(void) {
    struct sigaction outer;
    struct sigaction nested;
    memset(&outer, 0, sizeof(outer));
    outer.sa_sigaction = outer_handler;
    sigemptyset(&outer.sa_mask);
    outer.sa_flags = SA_SIGINFO;
    memset(&nested, 0, sizeof(nested));
    nested.sa_sigaction = nested_handler;
    sigemptyset(&nested.sa_mask);
    nested.sa_flags = SA_SIGINFO;
    if (sigaction(SIGUSR1, &outer, NULL) != 0) {
        fail_row("sigaction-us1", 0, errno);
        return 1;
    }
    if (sigaction(SIGUSR2, &nested, NULL) != 0) {
        fail_row("sigaction-us2", 0, errno);
        return 1;
    }
    return 0;
}

static int handler_row(void) {
    if (outer_count != 1 || outer_sig != SIGUSR1) {
        fail_row("handler", (long long)outer_count, outer_sig);
        return 1;
    }
    printf("signal: handler ok sig=%d\n", outer_sig);
    return 0;
}

static int ucontext_row(void) {
    long long distance;
    /* S720: raise is thread-directed through the kernel, so the interrupted PC is the libc
       instruction after the syscall, not the old libc-direct call site. Both live in the same
       statically linked text image (the linker places .text first at 0x400000 and .rodata after
       it), so the context must point into that image rather than at zero, the trampoline or data. */
    uintptr_t text_high = (uintptr_t)"signal: ucontext ok";
    if (interrupted_rip == 0 || interrupted_rsp == 0) {
        fail_row("ucontext-empty", (long long)interrupted_rip, 0);
        return 1;
    }
    if (interrupted_rip < 0x400000 || interrupted_rip >= text_high) {
        fail_row("ucontext-rip", (long long)interrupted_rip, 0);
        return 1;
    }
    distance = (long long)interrupted_rsp - (long long)expected_rsp;
    if (distance < 0) {
        distance = -distance;
    }
    if (distance > 1048576) {
        fail_row("ucontext-rsp", distance, 0);
        return 1;
    }
    printf("signal: ucontext ok rip=0x%llx rsp=0x%llx\n",
           (unsigned long long)interrupted_rip, (unsigned long long)interrupted_rsp);
    return 0;
}

static int nested_row(void) {
    if (nested_count != 1 || nested_sig != SIGUSR2) {
        fail_row("nested", (long long)nested_count, nested_sig);
        return 1;
    }
    printf("signal: nested ok sig=%d\n", nested_sig);
    return 0;
}

static int exec_default_row(void) {
    pid_t child = fork();
    int status = 0;
    if (child == 0) {
        execl("/bin/signal-acceptance", "signal-acceptance", "exec-child", (char *)NULL);
        _exit(127);
    }
    if (child < 0) {
        fail_row("exec-fork", 0, errno);
        return 1;
    }
    if (waitpid(child, &status, 0) != child) {
        fail_row("exec-wait", 0, errno);
        return 1;
    }
    if (!WIFSIGNALED(status) || WTERMSIG(status) != SIGUSR1) {
        fail_row("exec-default", status, 0);
        return 1;
    }
    printf("signal: exec-default ok signal=%d\n", WTERMSIG(status));
    return 0;
}

int main(int argc, char **argv) {
    int failures = 0;
    char marker = 0;

    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc > 1 && strcmp(argv[1], "exec-child") == 0) {
        (void)raise(SIGUSR1);
        printf("signal: exec-child survived after exec\n");
        _exit(1);
    }

    if (install_handlers() != 0) {
        _exit(1);
    }
    expected_rsp = (uintptr_t)&marker;
    after_raise = 0;
    raise_nested = 1;
    if (raise(SIGUSR1) != 0) {
        fail_row("raise-us1", 0, errno);
        _exit(1);
    }
    after_raise = 1;

    failures += handler_row();
    failures += ucontext_row();
    if (after_raise != 1) {
        fail_row("return", after_raise, 0);
        failures += 1;
    } else {
        printf("signal: return ok\n");
    }
    failures += nested_row();
    failures += exec_default_row();

    if (failures != 0) {
        printf("signal: fail rows=%d\n", failures);
        fflush(stdout);
        _exit(1);
    }
    printf("signal: ok\n");
    fflush(stdout);
    _exit(0);
}
