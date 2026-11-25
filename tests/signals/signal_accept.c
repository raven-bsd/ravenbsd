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

// S711: the kernel C19 provider is exercised through the raw numbers from raven-abi. The public
// C wrappers still carry the retiring libc mirror, so that path is covered by
// `signal-libc`/S710 rows instead.
#define RAW_SIGACTION 0x87
#define RAW_SIGPROCMASK 0x88
#define RAW_PROC_SIGNAL 0x4A

typedef struct {
    unsigned long long handler;
    unsigned long long mask[2];
    unsigned int flags;
    unsigned int pad;
    unsigned long long restorer;
} raw_sigaction_t;

static inline long raw3(long number, long a, long b, long c) {
    register long rax __asm__("rax") = number;
    register long rdi __asm__("rdi") = a;
    register long rsi __asm__("rsi") = b;
    register long rdx __asm__("rdx") = c;
    __asm__ volatile("syscall"
                     : "+r"(rax)
                     : "r"(rdi), "r"(rsi), "r"(rdx)
                     : "rcx", "r11", "memory");
    return rax;
}

static long raw_sigaction(int signo, raw_sigaction_t *act, raw_sigaction_t *old) {
    return raw3(RAW_SIGACTION, signo, (long)act, (long)old);
}

static long raw_procmask(int how, unsigned long long *set, unsigned long long *old) {
    return raw3(RAW_SIGPROCMASK, how, (long)set, (long)old);
}

static long raw_signal(int pid, int signo) {
    return raw3(RAW_PROC_SIGNAL, pid, signo, 0);
}

static volatile sig_atomic_t outer_count;
static volatile sig_atomic_t nested_count;
static volatile int outer_sig;
static volatile int nested_sig;
static volatile uintptr_t interrupted_rip;
static volatile uintptr_t interrupted_rsp;
static volatile uintptr_t expected_rip;
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
        if (raw_signal(getpid(), SIGUSR2) != 0) {
            nested_sig = -1;
        }
    }
}

static int install_handlers(void) {
    raw_sigaction_t outer;
    raw_sigaction_t nested;
    memset(&outer, 0, sizeof(outer));
    outer.handler = (unsigned long long)(uintptr_t)&outer_handler;
    outer.flags = SA_SIGINFO;
    memset(&nested, 0, sizeof(nested));
    nested.handler = (unsigned long long)(uintptr_t)&nested_handler;
    nested.flags = SA_SIGINFO;
    long result = raw_sigaction(SIGUSR1, &outer, NULL);
    if (result != 0) {
        fail_row("sigaction-us1", result, 0);
        return 1;
    }
    result = raw_sigaction(SIGUSR2, &nested, NULL);
    if (result != 0) {
        fail_row("sigaction-us2", result, 0);
        return 1;
    }
    {
        raw_sigaction_t query;
        memset(&query, 0, sizeof(query));
        (void)raw_sigaction(SIGUSR1, NULL, &query);
        printf("signal: action expected=0x%llx got=0x%llx flags=0x%x\n",
               (unsigned long long)(uintptr_t)&outer_handler, query.handler, query.flags);
        if (query.handler != (unsigned long long)(uintptr_t)&outer_handler) {
            fail_row("sigaction-readback", (long long)query.handler, 0);
            return 1;
        }
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
    if (interrupted_rip == 0 || interrupted_rsp == 0) {
        fail_row("ucontext-empty", (long long)interrupted_rip, 0);
        return 1;
    }
    distance = (long long)interrupted_rip - (long long)expected_rip;
    if (distance < 0) {
        distance = -distance;
    }
    if (distance > 65536) {
        fail_row("ucontext-rip", distance, 0);
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

// S711: a real interruptible wait must return EINTR because a caught kernel signal is queued,
// including when the signal arrives before the wait enrolls (the sticky pre-enrollment kick).
static int interrupt_row(void) {
    int fds[2];
    char byte = 0;
    int status = 0;
    int error = 0;
    long result;
    pid_t child;
    if (pipe(fds) != 0) {
        fail_row("interrupt-pipe", 0, errno);
        return 1;
    }
    child = fork();
    if (child == 0) {
        // Keep the write end open while spinning: the parent's read must block, not see EOF,
        // until the signal interrupts it.
        close(fds[0]);
        for (volatile long spin = 0; spin < 50000000; spin++) {
            __asm__ volatile("" ::: "memory");
        }
        raw_signal(getppid(), SIGUSR1);
        _exit(0);
    }
    close(fds[1]);
    if (child < 0) {
        close(fds[0]);
        fail_row("interrupt-fork", 0, errno);
        return 1;
    }
    errno = 0;
    result = read(fds[0], &byte, 1);
    error = errno;
    close(fds[0]);
    (void)waitpid(child, &status, 0);
    if (result != -1 || error != EINTR) {
        fail_row("interrupt-read", result, error);
        return 1;
    }
    if (outer_sig != SIGUSR1) {
        fail_row("interrupt-handler", outer_sig, 0);
        return 1;
    }
    printf("signal: interrupt ok sig=%d\n", outer_sig);
    return 0;
}

// S711: a blocked default-fatal signal is held, not applied at generation, and terminates when
// the mask opens (FreeBSD `tdsendsignal` SIG_HOLD + `issignal`).
static int blocked_row(void) {
    pid_t child = fork();
    int status = 0;
    if (child == 0) {
        raw_sigaction_t action;
        unsigned long long set = 1ULL << (SIGUSR1 - 1);
        memset(&action, 0, sizeof(action));
        (void)raw_sigaction(SIGUSR1, &action, NULL);
        if (raw_procmask(SIG_BLOCK, &set, NULL) != 0) {
            _exit(2);
        }
        if (raw_signal(getpid(), SIGUSR1) != 0) {
            _exit(3);
        }
        printf("signal: blocked-held\n");
        if (raw_procmask(SIG_UNBLOCK, &set, NULL) != 0) {
            _exit(4);
        }
        printf("signal: blocked-survived\n");
        _exit(0);
    }
    if (child < 0) {
        fail_row("blocked-fork", 0, errno);
        return 1;
    }
    if (waitpid(child, &status, 0) != child) {
        fail_row("blocked-wait", 0, errno);
        return 1;
    }
    if (!WIFSIGNALED(status) || WTERMSIG(status) != SIGUSR1) {
        fail_row("blocked-default", status, 0);
        return 1;
    }
    printf("signal: blocked ok signal=%d\n", WTERMSIG(status));
    return 0;
}

// S711: SIG_IGN wins over a default-terminate action and the record is discarded.
static int ignored_row(void) {
    raw_sigaction_t action;
    memset(&action, 0, sizeof(action));
    action.handler = (unsigned long long)(uintptr_t)SIG_IGN;
    if (raw_sigaction(SIGUSR1, &action, NULL) != 0) {
        fail_row("ignored-action", 0, 0);
        return 1;
    }
    if (raw_signal(getpid(), SIGUSR1) != 0) {
        fail_row("ignored-send", 0, 0);
        return 1;
    }
    memset(&action, 0, sizeof(action));
    (void)raw_sigaction(SIGUSR1, &action, NULL);
    printf("signal: ignored ok\n");
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
    printf("signal: start pid=%d\n", (int)getpid());
    if (argc > 1 && strcmp(argv[1], "exec-child") == 0) {
        // The kernel sigacts table is reset by exec; the raw default action must terminate the
        // new image with SIGUSR1, not a normal exit.
        (void)raw_signal(getpid(), SIGUSR1);
        printf("signal: exec-child survived after exec\n");
        _exit(1);
    }

    if (install_handlers() != 0) {
        _exit(1);
    }
    printf("signal: handlers-installed\n");
    expected_rip = (uintptr_t)&raw_signal;
    expected_rsp = (uintptr_t)&marker;
    after_raise = 0;
    raise_nested = 1;
    if (raw_signal(getpid(), SIGUSR1) != 0) {
        fail_row("raise-us1", 0, 0);
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
    failures += interrupt_row();
    failures += blocked_row();
    failures += exec_default_row();
    failures += ignored_row();

    if (failures != 0) {
        printf("signal: fail rows=%d\n", failures);
        fflush(stdout);
        _exit(1);
    }
    printf("signal: ok\n");
    fflush(stdout);
    _exit(0);
}
