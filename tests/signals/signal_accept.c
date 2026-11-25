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
#include <sys/mman.h>
#include <sys/wait.h>
#include <ucontext.h>
#include <unistd.h>

#define BOUNDARY_PAGE 4096
#define BOUNDARY_CANARY 0x5a
#define RAVEN_DEBUG_WRITE 3

/// The service's inherited stdout is not the serial console on every architecture, so the
/// boundary rows go to the kernel debug sink (`DEBUG_WRITE`) that the qtest serial captures.
static long boundary_write(const char *bytes, unsigned long len) {
#if defined(__x86_64__)
    register long rax __asm__("rax") = RAVEN_DEBUG_WRITE;
    register long rdi __asm__("rdi") = (long)bytes;
    register long rsi __asm__("rsi") = (long)len;
    __asm__ volatile("syscall" : "+r"(rax) : "r"(rdi), "r"(rsi) : "rcx", "r11", "memory");
    return rax;
#elif defined(__aarch64__)
    register long x8 __asm__("x8") = RAVEN_DEBUG_WRITE;
    register long x0 __asm__("x0") = (long)bytes;
    register long x1 __asm__("x1") = (long)len;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x8), "r"(x1) : "memory");
    return x0;
#else
    (void)bytes;
    (void)len;
    return -1;
#endif
}

static void boundary_log(const char *text) {
    unsigned long len = 0;
    while (text[len] != '\0') {
        len++;
    }
    (void)boundary_write(text, len);
}

/// Emit `<name> ok flags=0x<hex> mask=1` with the installed flags actually read back.
static void boundary_row(const char *name, unsigned flags) {
    char buffer[160];
    size_t at = 0;
    const char *prefix = "signal: boundary ";
    while (*prefix != '\0') {
        buffer[at++] = *prefix++;
    }
    while (*name != '\0') {
        buffer[at++] = *name++;
    }
    const char *suffix = " ok flags=0x";
    while (*suffix != '\0') {
        buffer[at++] = *suffix++;
    }
    char digits[8];
    size_t count = 0;
    if (flags == 0) {
        digits[count++] = '0';
    }
    while (flags != 0 && count < sizeof(digits)) {
        unsigned digit = flags & 0xfu;
        digits[count++] = (char)(digit < 10 ? '0' + digit : 'a' + digit - 10);
        flags >>= 4;
    }
    while (count > 0) {
        buffer[at++] = digits[--count];
    }
    const char *tail = " mask=1\n";
    while (*tail != '\0') {
        buffer[at++] = *tail++;
    }
    (void)boundary_write(buffer, (unsigned long)at);
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
#if defined(__x86_64__)
    if (ucontext != NULL) {
        interrupted_rip = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RIP];
        interrupted_rsp = (uintptr_t)ucontext->uc_mcontext.gregs[REG_RSP];
    } else {
        interrupted_rip = 0;
        interrupted_rsp = 0;
    }
#else
    // The boundary rows do not deliver a signal; the raise-dependent ucontext rows stay x86_64.
    (void)ucontext;
    interrupted_rip = 0;
    interrupted_rsp = 0;
#endif
    if (raise_nested != 0) {
        raise_nested = 0;
        if (raise(SIGUSR2) != 0) {
            nested_sig = -1;
        }
    }
}

static int boundary_fail(const char *row, long long value, int error) {
    printf("signal: boundary-fail %s value=%lld errno=%d\n", row, value, error);
    (void)value;
    (void)error;
    boundary_log("signal: boundary-fail\n");
    return 1;
}

/// Canary bytes survive outside the two public 152-byte objects.
static int boundary_canaries_intact(const unsigned char *page, size_t first, size_t second) {
    for (size_t i = 0; i < BOUNDARY_PAGE; i++) {
        if ((i >= first && i < first + sizeof(struct sigaction)) ||
            (i >= second && i < second + sizeof(struct sigaction))) {
            continue;
        }
        if (page[i] != BOUNDARY_CANARY) {
            return 0;
        }
    }
    return 1;
}

/// The public 152-byte C `struct sigaction` row: libc must marshal it to the private 40-byte
/// kernel request, preserve the public size, and return the installed flags and mask exactly.
/// The new and old objects sit at the last public-struct slot before an inaccessible guard page;
/// a separate copy is surrounded by canaries so a wider write is visible as data damage.
static int boundary_rows(void) {
    int failures = 0;
    size_t len = 4 * BOUNDARY_PAGE;

    _Static_assert(sizeof(struct sigaction) == 152, "the public C sigaction keeps its 152 bytes");

    unsigned char *region =
        mmap(NULL, len, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (region == MAP_FAILED) {
        return boundary_fail("mmap-region", 0, errno);
    }
    if (mprotect(region + BOUNDARY_PAGE, BOUNDARY_PAGE, PROT_NONE) != 0 ||
        mprotect(region + 3 * BOUNDARY_PAGE, BOUNDARY_PAGE, PROT_NONE) != 0) {
        munmap(region, len);
        return boundary_fail("mprotect-region", 0, errno);
    }

    struct sigaction *act =
        (struct sigaction *)(region + BOUNDARY_PAGE - sizeof(struct sigaction));
    struct sigaction *old =
        (struct sigaction *)(region + 3 * BOUNDARY_PAGE - sizeof(struct sigaction));
    memset(act, 0, sizeof(*act));
    memset(old, 0, sizeof(*old));
    act->sa_sigaction = outer_handler;
    act->sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&act->sa_mask);
    if (sigaddset(&act->sa_mask, SIGUSR2) != 0) {
        munmap(region, len);
        return boundary_fail("sigaddset", 0, errno);
    }

    if (sigaction(SIGUSR1, act, NULL) != 0) {
        failures += boundary_fail("guard-install", 0, errno);
    } else if (sigaction(SIGUSR1, NULL, old) != 0) {
        failures += boundary_fail("guard-query", 0, errno);
    } else if (old->sa_sigaction != outer_handler ||
               (old->sa_flags & (SA_SIGINFO | SA_ONSTACK)) != (SA_SIGINFO | SA_ONSTACK) ||
               sigismember(&old->sa_mask, SIGUSR2) != 1 || old->sa_restorer != NULL) {
        failures += boundary_fail("guard-flags-mask", old->sa_flags, errno);
    } else {
        boundary_row("guard", (unsigned)old->sa_flags);
    }
    munmap(region, len);

    unsigned char *page =
        mmap(NULL, BOUNDARY_PAGE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (page == MAP_FAILED) {
        return failures + boundary_fail("mmap-canary", 0, errno);
    }
    memset(page, BOUNDARY_CANARY, BOUNDARY_PAGE);
    const size_t act_offset = 128;
    const size_t old_offset = 512;
    struct sigaction *cact = (struct sigaction *)(page + act_offset);
    struct sigaction *cold = (struct sigaction *)(page + old_offset);
    memset(cact, 0, sizeof(*cact));
    memset(cold, 0, sizeof(*cold));
    cact->sa_sigaction = nested_handler;
    cact->sa_flags = SA_SIGINFO;
    sigemptyset(&cact->sa_mask);
    if (sigaddset(&cact->sa_mask, SIGUSR1) != 0) {
        munmap(page, BOUNDARY_PAGE);
        return failures + boundary_fail("canary-sigaddset", 0, errno);
    }
    if (sigaction(SIGUSR2, cact, NULL) != 0) {
        failures += boundary_fail("canary-install", 0, errno);
    } else if (sigaction(SIGUSR2, NULL, cold) != 0) {
        failures += boundary_fail("canary-query", 0, errno);
    } else if (cold->sa_sigaction != nested_handler ||
               (cold->sa_flags & SA_SIGINFO) != SA_SIGINFO ||
               sigismember(&cold->sa_mask, SIGUSR1) != 1 ||
               !boundary_canaries_intact(page, act_offset, old_offset)) {
        failures += boundary_fail("canary-data-flags-mask", cold->sa_flags, errno);
    } else {
        boundary_row("canary", (unsigned)cold->sa_flags);
    }
    munmap(page, BOUNDARY_PAGE);
    return failures;
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
    if (argc > 1 && strcmp(argv[1], "boundary") == 0) {
        if (boundary_rows() != 0) {
            boundary_log("signal: boundary-fail rows\n");
            _exit(1);
        }
        boundary_log("signal: boundary ok\n");
        _exit(0);
    }
    if (argc > 1 && strcmp(argv[1], "exec-child") == 0) {
        (void)raise(SIGUSR1);
        printf("signal: exec-child survived after exec\n");
        _exit(1);
    }

    if (install_handlers() != 0) {
        _exit(1);
    }
    expected_rip = (uintptr_t)&raise;
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
