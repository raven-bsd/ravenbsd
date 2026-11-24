// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K434 rev207 diagnostic: print guest processes' K391 state and last syscall. Test tooling only.
// usage: proc-probe PID | proc-probe --scan

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define PROC_PID_INFO 0x3a
#define INFO_WORDS 61

#if defined(__aarch64__)
static long raw_call(long number, long a, long b, long c) {
    register long x0 asm("x0") = a;
    register long x1 asm("x1") = b;
    register long x2 asm("x2") = c;
    register long x8 asm("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x2), "r"(x8) : "memory");
    return x0;
}
#else
static long raw_call(long number, long a, long b, long c) {
    register long rax asm("rax") = number;
    register long rdi asm("rdi") = a;
    register long rsi asm("rsi") = b;
    register long rdx asm("rdx") = c;
    __asm__ volatile("syscall" : "+r"(rax) : "r"(rdi), "r"(rsi), "r"(rdx) : "rcx", "r11", "memory");
    return rax;
}
#endif

static long raven_debug_write(const char *bytes, size_t len) {
#if defined(__aarch64__)
    register long x0 asm("x0") = (long)bytes;
    register long x1 asm("x1") = (long)len;
    register long x8 asm("x8") = 3;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x8) : "memory");
    return x0;
#else
    register long rax asm("rax") = 3;
    register long rdi asm("rdi") = (long)bytes;
    register long rsi asm("rsi") = (long)len;
    __asm__ volatile("syscall" : "+r"(rax) : "r"(rdi), "r"(rsi) : "rcx", "r11", "memory");
    return rax;
#endif
}

static void say_line(const char *line, int length) {
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= 256) {
            size = 255;
        }
        (void)raven_debug_write(line, size);
    }
}

static int probe_one(long pid, int scan) {
    unsigned char data[512];
    memset(data, 0, sizeof(data));
    long result = raw_call(PROC_PID_INFO, pid, (long)data, INFO_WORDS);
    if (result < 0) {
        if (!scan) {
            char line[256];
            int length = snprintf(line, sizeof(line), "proc-probe: pid=%ld errno=%ld\n", pid, -result);
            say_line(line, length);
        }
        return -1;
    }
    char name[64];
    memcpy(name, data + 64, sizeof(name) - 1);
    name[sizeof(name) - 1] = '\0';
    unsigned long long *w = (unsigned long long *)(data + 328);
    if (scan && w[5] == 0 && strstr(name, "ssh") == NULL) {
        return 0;
    }
    {
        char line[256];
        int length = snprintf(
            line, sizeof(line),
            "%s: pid=%ld name=%s state=%llu runnable=%llu last_syscall=0x%llx "
            "last_pc=0x%llx wait=0x%llx in_syscall=%llu\n",
            scan ? "proc-scan" : "proc-probe", pid, name, w[0], w[1], w[2], w[3], w[4], w[5]);
        say_line(line, length);
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2 || strcmp(argv[1], "--scan") == 0) {
        {
            char start[] = "proc-scan: start\n";
            say_line(start, (int)(sizeof(start) - 1));
        }
        for (;;) {
            long pid;
            for (pid = 1; pid <= 64; pid++) {
                (void)probe_one(pid, 1);
            }
            sleep(2);
        }
    }
    long pid = strtol(argv[1], NULL, 10);
    return probe_one(pid, 0) < 0 ? 1 : 0;
}
