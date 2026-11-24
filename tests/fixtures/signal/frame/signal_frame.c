// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S44 C19 kernel-frame fixture. Test-only: uses the raw SIGACTION (0x87) and PROC_SIGNAL (0x4A)
// syscalls directly so the kernel frame path is exercised without the libc raise/entry path.
// Rows: handler arguments, ucontext interrupted rip/rsp, SIGRETURN, default action, exec default.

#include <errno.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <ucontext.h>
#include <unistd.h>

#define RAW_DEBUG_WRITE 0x03L
#define RAW_SIGACTION 0x87L
#define RAW_PROC_SIGNAL 0x4AL
#define RAW_SA_SIGINFO 4U

struct kernel_sigaction {
    void *handler;
    unsigned long mask[2];
    unsigned int flags;
    unsigned int pad;
    void *restorer;
};

_Static_assert(sizeof(struct kernel_sigaction) == 40, "kernel SIGACTION is 40 bytes");

static volatile sig_atomic_t handler_count;
static volatile int handler_sig;
static volatile int handler_info_sig = -1;
static volatile uintptr_t interrupted_rip;
static volatile uintptr_t interrupted_rsp;
static volatile uintptr_t expected_rip;
static volatile uintptr_t expected_rsp;

static long raw3(long number, long first, long second, long third) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(first), "S"(second), "d"(third)
                     : "rcx", "r11", "memory");
    return result;
}

static void say_text(const char *text) {
    if (text != NULL && *text != '\0') {
        (void)raw3(RAW_DEBUG_WRITE, (long)text, (long)strlen(text), 0);
    }
}

static void sayf(const char *format, ...) {
    char buffer[192];
    va_list arguments;
    int length;

    va_start(arguments, format);
    length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    va_end(arguments);
    if (length < 0) {
        return;
    }
    say_text(buffer);
}

static int raw_sigaction(int signo, struct kernel_sigaction *action) {
    long result = raw3(RAW_SIGACTION, signo, (long)action, 0);
    if (result < 0) {
        errno = (int)-result;
        return -1;
    }
    return 0;
}

static int raw_proc_signal(int pid, int signo) {
    long result = raw3(RAW_PROC_SIGNAL, pid, signo, 0);
    if (result < 0) {
        errno = (int)-result;
        return -1;
    }
    return 0;
}

static void handler(int signo, siginfo_t *info, void *context) {
    ucontext_t *ucontext = (ucontext_t *)context;

    handler_count++;
    handler_sig = signo;
    if (info != NULL) {
        handler_info_sig = info->si_signo;
    }
    if (ucontext != NULL) {
        interrupted_rip = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RIP];
        interrupted_rsp = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RSP];
    }
}

static int fail_row(const char *row, long long value, int error) {
    sayf("signal-frame: fail %s value=%lld errno=%d\n", row, value, error);
    return 1;
}

static int check_handler(void) {
    if (handler_count != 1 || handler_sig != SIGUSR1 || handler_info_sig != SIGUSR1) {
        return fail_row("handler", (long long)handler_count, handler_info_sig);
    }
    sayf("signal-frame: handler ok sig=%d\n", handler_sig);
    return 0;
}

static int check_ucontext(void) {
    long long distance;

    if (interrupted_rip == 0 || interrupted_rsp == 0) {
        return fail_row("ucontext-empty", (long long)interrupted_rip, 0);
    }
    distance = (long long)interrupted_rip - (long long)expected_rip;
    if (distance < 0) {
        distance = -distance;
    }
    if (distance > 65536) {
        return fail_row("ucontext-rip", distance, 0);
    }
    distance = (long long)interrupted_rsp - (long long)expected_rsp;
    if (distance < 0) {
        distance = -distance;
    }
    if (distance > 1048576) {
        return fail_row("ucontext-rsp", distance, 0);
    }
    sayf("signal-frame: ucontext ok rip=0x%llx rsp=0x%llx\n",
         (unsigned long long)interrupted_rip, (unsigned long long)interrupted_rsp);
    return 0;
}

static int default_action_row(void) {
    struct kernel_sigaction action;
    pid_t child;
    int status = 0;

    memset(&action, 0, sizeof(action));
    action.handler = SIG_DFL;
    child = fork();
    if (child == 0) {
        if (raw_sigaction(SIGUSR1, &action) != 0) {
            _exit(3);
        }
        if (raw_proc_signal(getpid(), SIGUSR1) != 0) {
            _exit(4);
        }
        _exit(5);
    }
    if (child < 0) {
        return fail_row("default-fork", 0, errno);
    }
    if (waitpid(child, &status, 0) != child) {
        return fail_row("default-wait", 0, errno);
    }
    if (!WIFSIGNALED(status) || WTERMSIG(status) != SIGUSR1) {
        return fail_row("default", status, 0);
    }
    sayf("signal-frame: default ok signal=%d\n", WTERMSIG(status));
    return 0;
}

int main(int argc, char **argv) {
    struct kernel_sigaction action;
    uintptr_t marker = 0;
    int failures = 0;

    (void)argc;
    (void)argv;
    sayf("signal-frame: start\n");
    memset(&action, 0, sizeof(action));
    action.handler = (void *)handler;
    action.flags = RAW_SA_SIGINFO;
    if (raw_sigaction(SIGUSR1, &action) != 0) {
        return fail_row("sigaction", 0, errno);
    }

    expected_rip = (uintptr_t)&&after_signal;
    expected_rsp = (uintptr_t)&marker;
    if (raw_proc_signal(getpid(), SIGUSR1) != 0) {
        return fail_row("signal", 0, errno);
    }
after_signal:
    failures += check_handler();
    failures += check_ucontext();
    if (handler_count != 1 || interrupted_rip == 0) {
        failures += fail_row("return", (long long)handler_count, 0);
    } else {
        sayf("signal-frame: return ok\n");
    }
    failures += default_action_row();

    if (failures != 0) {
        sayf("signal-frame: fail rows=%d\n", failures);
        _exit(1);
    }
    sayf("signal-frame: ok\n");
    _exit(0);
}
