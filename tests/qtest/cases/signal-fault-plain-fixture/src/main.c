// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S779 plain `signal(2)` fault-handler fixture.
//!
//! Replays the NetBSD `t_mprotect.c:284-314` `mprotect_write` shape through the public libc:
//! a page is mapped read/write with `mmap(NULL, page, PROT_READ|PROT_WRITE, MAP_ANON, -1, 0)`
//! (the donor passes `MAP_ANON` without a `MAP_PRIVATE`/`MAP_SHARED` type), written, made
//! read-only with `mprotect(2)`, and a child writes it again. The child installs
//! `signal(SIGSEGV, sighandler)` (`SA_RESTART`, no `SA_SIGINFO`); `sighandler` is the donor
//! `_exit(signo)`. The C19 kernel frame must enter the plain handler and `_exit(SIGSEGV)` must
//! terminate the child with status 11.
//!
//! The same image keeps the S776 coverage: an `SA_SIGINFO` handler observes
//! `SIGSEGV`/`SEGV_ACCERR` and the fault address, rewrites the saved RIP and resumes through
//! the kernel sigcode; a default-disposition child dies with status -11.
//!
//! `/init` stdout is not the serial console, so the fixture reports through the
//! `DEBUG_WRITE` syscall (`raven_abi::syscall::DEBUG_WRITE`, 0x03) like `raven-rt::println!`.

#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <ucontext.h>
#include <unistd.h>

#define PAGE_SIZE 4096

static volatile sig_atomic_t saved_signo;
static volatile sig_atomic_t saved_code;
static volatile uintptr_t saved_addr;
static char *resume_after;

extern void sf_fault(char *page);
extern char sf_after[];

__asm__(
    ".text\n"
    ".globl sf_fault\n"
    ".type sf_fault,@function\n"
    "sf_fault:\n"
    "    movb $1,(%rdi)\n"
    ".globl sf_after\n"
    "sf_after:\n"
    "    ret\n"
    ".size sf_fault, .-sf_fault\n");

/* `raven_abi::syscall::DEBUG_WRITE` (0x03): write to the kernel debug console. */
static void console_write(const char *text, unsigned long len) {
    register long number __asm__("rax") = 0x03;
    register const char *address __asm__("rdi") = text;
    register unsigned long count __asm__("rsi") = len;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(address), "r"(count)
                     : "rcx", "r11", "memory");
}

static void emit(const char *text) {
    unsigned long len = 0;
    while (text[len] != '\0') {
        len++;
    }
    console_write(text, len);
}

static void emit_i32(const char *prefix, int value, const char *suffix) {
    char digits[12];
    int length = 0;
    unsigned int magnitude = value < 0 ? 0u - (unsigned int)value : (unsigned int)value;
    do {
        digits[length++] = (char)('0' + magnitude % 10u);
        magnitude /= 10u;
    } while (magnitude != 0);
    emit(prefix);
    if (value < 0) {
        console_write("-", 1);
    }
    while (length > 0) {
        console_write(&digits[--length], 1);
    }
    emit(suffix);
}

static void emit_hex(const char *prefix, uintptr_t value, const char *suffix) {
    static const char hex[] = "0123456789abcdef";
    char digits[2 * sizeof(value)];
    int length = 0;
    do {
        digits[length++] = hex[value & 0xfu];
        value >>= 4;
    } while (value != 0);
    emit(prefix);
    while (length > 0) {
        console_write(&digits[--length], 1);
    }
    emit(suffix);
}

static void info_handler(int sig, siginfo_t *info, void *context) {
    ucontext_t *uc = context;
    saved_signo = sig;
    saved_code = info->si_code;
    saved_addr = (uintptr_t)info->si_addr;
    uc->uc_mcontext.gregs[REG_RIP] = (greg_t)(uintptr_t)resume_after;
}

/* NetBSD `t_mprotect.c` `sighandler`: exit with the signal number. */
static void exit_handler(int sig) { _exit(sig); }

#ifdef S779_PRIVATE_MAP
/* `signal-fault-plain-private`: the same shape with an explicit private type. */
#define S779_MAP_FLAGS (MAP_ANON | MAP_PRIVATE)
#else
/* The donor `mprotect_write` flags: `MAP_ANON` with no private/shared type. */
#define S779_MAP_FLAGS MAP_ANON
#endif

static int map_read_only(char **page) {
    char *mapped = mmap(NULL, PAGE_SIZE, PROT_READ | PROT_WRITE, S779_MAP_FLAGS, -1, 0);
    if (mapped == MAP_FAILED) {
        emit_i32("signal-fault: donor-mmap failed errno=", errno, "\n");
        return -1;
    }
    memset(mapped, 'A', PAGE_SIZE);
    if (mprotect(mapped, PAGE_SIZE, PROT_READ) != 0) {
        emit("signal-fault: mprotect failed\n");
        return -1;
    }
    *page = mapped;
    return 0;
}

static int child_status(pid_t pid, int *status) {
    return waitpid(pid, status, 0) == pid ? 0 : -1;
}

static int run_caught_child(char *page) {
    pid_t pid = fork();
    if (pid < 0) {
        return -1;
    }
    if (pid == 0) {
        if (signal(SIGSEGV, exit_handler) == SIG_ERR) {
            _exit(101);
        }
        sf_fault(page);
        _exit(102);
    }
    int status = 0;
    if (child_status(pid, &status) != 0 || !WIFEXITED(status)) {
        return -1;
    }
    emit_i32("signal-fault: caught status=", WEXITSTATUS(status), "\n");
    return WEXITSTATUS(status) == SIGSEGV ? 0 : -1;
}

static int run_default_child(char *page) {
    pid_t pid = fork();
    if (pid < 0) {
        return -1;
    }
    if (pid == 0) {
        if (signal(SIGSEGV, SIG_DFL) == SIG_ERR) {
            _exit(103);
        }
        sf_fault(page);
        _exit(104);
    }
    int status = 0;
    if (child_status(pid, &status) != 0 || !WIFSIGNALED(status)) {
        return -1;
    }
    emit_i32("signal-fault: default status=-", WTERMSIG(status), "\n");
    return WTERMSIG(status) == SIGSEGV ? 0 : -1;
}

int main(void) {
    char *page = NULL;
    if (map_read_only(&page) != 0) {
        return 1;
    }
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_sigaction = info_handler;
    action.sa_flags = SA_SIGINFO;
    if (sigaction(SIGSEGV, &action, NULL) != 0) {
        emit("signal-fault: sigaction failed\n");
        return 1;
    }
    resume_after = sf_after;
    sf_fault(page);
    if (saved_signo != SIGSEGV) {
        emit("signal-fault: page handler=0\n");
        return 1;
    }
    emit("signal-fault: page handler=1");
    emit_i32(" code=", (int)saved_code, "");
    emit_hex(" addr=0x", saved_addr, "");
    emit_i32(" addr-match=",
             saved_addr == (uintptr_t)page && saved_code == SEGV_ACCERR, "\n");
    emit("signal-fault: page resumed=1\n");
    if (run_caught_child(page) != 0 || run_default_child(page) != 0) {
        return 1;
    }
    emit("signal-fault: done\n");
    return 0;
}
