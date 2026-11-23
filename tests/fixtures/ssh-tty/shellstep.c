// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495 rev166 test-only shell-step fixture. It reproduces what a shell does around one external
// command (posix_spawnp or fork/exec, then waitpid) without brush, and captures a fatal page fault
// from the faulting process itself: a watchdog sibling reads the faulting process's K391 words
// (last syscall, last user pc, in_syscall) with PROC_PID_INFO while the signal handler spins with
// no syscalls, so the pre-fault values are still in the Process struct. It is test tooling, not
// base code.

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <ucontext.h>
#include <unistd.h>

extern char **environ;

#define PROC_PID_INFO 0x3A
#define INFO_WORDS 61
#define REG_RIP_INDEX 16
#define REG_TRAPNO_INDEX 20
#define REG_ERR_INDEX 19

struct shared {
    volatile unsigned long flag; // 0 = running, 1 = faulted, 2 = finished
    volatile unsigned long sig;
    volatile unsigned long code;
    volatile unsigned long addr;
    volatile unsigned long rip;
    volatile unsigned long rsp;
    volatile unsigned long trapno;
    volatile unsigned long err;
    volatile unsigned long pid;
    volatile unsigned long entry;
    volatile unsigned long self_last_syscall;
    volatile unsigned long self_last_pc;
    volatile unsigned long self_in_syscall;
};

static struct shared *shared;

static long sys3(long number, long a0, long a1, long a2) {
    register long r10 __asm__("r10") = 0;
    (void)r10;
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(a0), "S"(a1), "d"(a2)
                     : "rcx", "r11", "memory");
    return result;
}

static void put(const char *text) {
    size_t length = strlen(text);
    (void)write(STDOUT_FILENO, text, length);
}

static void put_hex(unsigned long value) {
    char buffer[19];
    size_t index = sizeof(buffer);
    buffer[--index] = '\0';
    do {
        unsigned long digit = value & 0xf;
        buffer[--index] = (char)(digit < 10 ? '0' + digit : 'a' + (digit - 10));
        value >>= 4;
    } while (value != 0 && index > 0);
    buffer[--index] = 'x';
    buffer[--index] = '0';
    put(&buffer[index]);
}

static void put_dec(long value) {
    char buffer[24];
    size_t index = sizeof(buffer);
    buffer[--index] = '\0';
    unsigned long magnitude = value < 0 ? (unsigned long)(-value) : (unsigned long)value;
    do {
        buffer[--index] = (char)('0' + (magnitude % 10));
        magnitude /= 10;
    } while (magnitude != 0 && index > 0);
    if (value < 0) {
        buffer[--index] = '-';
    }
    put(&buffer[index]);
}

static void label_value(const char *label, unsigned long value, int hex) {
    put(label);
    if (hex) {
        put_hex(value);
    } else {
        put_dec((long)value);
    }
    put("\n");
}

static void fault_handler(int sig, siginfo_t *info, void *context) {
    ucontext_t *uc = (ucontext_t *)context;
    shared->sig = (unsigned long)sig;
    shared->code = (unsigned long)info->si_code;
    shared->addr = (unsigned long)info->si_addr;
    shared->rip = uc->uc_mcontext.gregs[REG_RIP_INDEX];
    shared->rsp = uc->uc_mcontext.gregs[15];
    shared->trapno = uc->uc_mcontext.gregs[REG_TRAPNO_INDEX];
    shared->err = uc->uc_mcontext.gregs[REG_ERR_INDEX];
    shared->entry = (unsigned long)(uintptr_t)&fault_handler;
    shared->flag = 1;
    for (;;) {
        __asm__ volatile("pause");
    }
}

static void install_handler(void) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_sigaction = fault_handler;
    action.sa_flags = SA_SIGINFO | SA_NODEFER;
    (void)sigemptyset(&action.sa_mask);
    (void)sigaction(SIGSEGV, &action, NULL);
    (void)sigaction(SIGBUS, &action, NULL);
    (void)sigaction(SIGILL, &action, NULL);
    (void)sigaction(SIGFPE, &action, NULL);
}

static void watchdog(unsigned long parent_pid) {
    struct timespec pause = {0, 1000000};
    while (shared->flag == 0) {
        (void)nanosleep(&pause, NULL);
    }
    if (shared->flag == 2) {
        put("SHELLSTEP-WATCHDOG clean\n");
        _exit(0);
    }
    unsigned long words[INFO_WORDS];
    memset(words, 0, sizeof(words));
    long rc = sys3(PROC_PID_INFO, (long)parent_pid, (long)words, INFO_WORDS);
    put("SHELLSTEP-FAULT\n");
    label_value("SHELLSTEP pid=", shared->pid, 0);
    label_value("SHELLSTEP sig=", shared->sig, 0);
    label_value("SHELLSTEP code=", shared->code, 0);
    label_value("SHELLSTEP addr=", shared->addr, 1);
    label_value("SHELLSTEP rip=", shared->rip, 1);
    label_value("SHELLSTEP rsp=", shared->rsp, 1);
    label_value("SHELLSTEP trapno=", shared->trapno, 0);
    label_value("SHELLSTEP err=", shared->err, 1);
    label_value("SHELLSTEP entry=", shared->entry, 1);
    label_value("SHELLSTEP info_rc=", (unsigned long)rc, 0);
    if (rc == 0) {
        label_value("SHELLSTEP minfo_pid=", words[0], 0);
        label_value("SHELLSTEP last_syscall=", words[43], 0);
        label_value("SHELLSTEP last_pc=", words[44], 1);
        label_value("SHELLSTEP in_syscall=", words[46], 0);
    }
    (void)kill((pid_t)parent_pid, SIGKILL);
    _exit(0);
}

static void marker(const char *mode) {
    put("SHELLSTEP-BEGIN mode=");
    put(mode);
    put(" pid=");
    put_dec((long)getpid());
    put(" entry=");
    put_hex((unsigned long)(uintptr_t)&marker);
    put("\n");
}

static int run_none(void) {
    put("SHELLSTEP-NONE done\n");
    return 0;
}

static int run_fork(char **target) {
    pid_t child = fork();
    if (child < 0) {
        put("SHELLSTEP-FORK fork-failed\n");
        return 1;
    }
    if (child == 0) {
        execv(target[0], target);
        _exit(127);
    }
    int status = 0;
    pid_t reaped = waitpid(child, &status, 0);
    put("SHELLSTEP-FORK waitpid=");
    put_dec((long)reaped);
    put(" status=");
    put_dec((long)status);
    put("\n");
    return 0;
}

static int run_spawn(char **target, int blocking) {
    pid_t child = 0;
    int rc = posix_spawnp(&child, target[0], NULL, NULL, target, environ);
    put("SHELLSTEP-SPAWN posix_spawnp=");
    put_dec((long)rc);
    put(" pid=");
    put_dec((long)child);
    put("\n");
    if (rc != 0) {
        return 1;
    }
    int status = 0;
    struct timespec pause = {0, 1000000};
    long reaped = 0;
    int spins = 0;
    if (blocking) {
        reaped = waitpid(child, &status, 0);
    } else {
        do {
            reaped = waitpid(child, &status, WNOHANG);
            if (reaped == 0) {
                (void)nanosleep(&pause, NULL);
                spins += 1;
            }
        } while (reaped == 0 && spins < 5000);
    }
    put("SHELLSTEP-SPAWN waitpid=");
    put_dec(reaped);
    put(" status=");
    put_dec((long)status);
    put(" spins=");
    put_dec(spins);
    put(" blocking=");
    put_dec(blocking);
    put("\n");
    return 0;
}

int main(int argc, char **argv) {
    void *mapping = mmap(NULL, sizeof(struct shared), PROT_READ | PROT_WRITE,
                         MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    if (mapping == MAP_FAILED) {
        put("SHELLSTEP-MMAP failed\n");
        return 1;
    }
    shared = (struct shared *)mapping;
    memset((void *)shared, 0, sizeof(*shared));
    shared->pid = (unsigned long)getpid();
    shared->entry = (unsigned long)(uintptr_t)&main;

    pid_t self = getpid();
    pid_t watcher = fork();
    if (watcher < 0) {
        put("SHELLSTEP-WATCHDOG fork-failed\n");
        return 1;
    }
    if (watcher == 0) {
        watchdog((unsigned long)self);
    }

    install_handler();
    const char *mode = argc > 1 ? argv[1] : "spawn";
    char *default_target[] = {(char *)"/bin/ls", (char *)"/", NULL};
    char **target = argc > 2 ? &argv[2] : default_target;
    marker(mode);
    int rc = 0;
    if (strcmp(mode, "none") == 0) {
        rc = run_none();
    } else if (strcmp(mode, "fork") == 0) {
        rc = run_fork(target);
    } else if (strcmp(mode, "spawn") == 0) {
        rc = run_spawn(target, 0);
    } else if (strcmp(mode, "spawnblock") == 0) {
        rc = run_spawn(target, 1);
    } else {
        put("SHELLSTEP-UNKNOWN mode\n");
        rc = 2;
    }
    put("SHELLSTEP-END rc=");
    put_dec(rc);
    put("\n");
    shared->flag = 2;
    (void)waitpid(watcher, NULL, 0);
    return rc;
}
