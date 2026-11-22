/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K491: the libc NOFILE consumer must fail with EMFILE exactly at the soft limit, and the
 * kernel CPU limit set by the parent must survive exec. The report goes through DEBUG_WRITE
 * because a bare initfs process has no libc stdio wiring. */

#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

#define SYS_DEBUG_WRITE 0x03
#define PROC_GETRLIMIT 0x4E
#define DEFAULT_SOFT_NOFILE 1024

static long raw2(long number, long a, long b)
{
    long ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(number), "D"(a), "S"(b)
                     : "rcx", "r11", "memory");
    return ret;
}

static void say(const char *text)
{
    long length = 0;
    while (text[length] != '\0') {
        length++;
    }
    (void)raw2(SYS_DEBUG_WRITE, (long)text, length);
}

static void say_number(const char *label, long value)
{
    char buffer[64];
    int used = 0;
    while (label[used] != '\0' && used < 48) {
        buffer[used] = label[used];
        used++;
    }
    char digits[16];
    int count = 0;
    if (value == 0) {
        digits[count++] = '0';
    } else {
        while (value > 0 && count < 16) {
            digits[count++] = (char)('0' + value % 10);
            value /= 10;
        }
    }
    while (count > 0 && used < 62) {
        buffer[used++] = digits[--count];
    }
    buffer[used++] = '\n';
    (void)raw2(SYS_DEBUG_WRITE, (long)buffer, used);
}

static int open_fds(int soft)
{
    int count = 0;
    for (int fd = 0; fd < soft; fd++) {
        if (fcntl(fd, F_GETFD) != -1) {
            count++;
        }
    }
    return count;
}

int main(void)
{
    unsigned long long cpu[2] = {0, 0};
    int inherited = raw2(PROC_GETRLIMIT, 0, (long)cpu) == 0 && cpu[0] == 1 && cpu[1] == 1;

    long soft = sysconf(_SC_OPEN_MAX);
    int count = soft > 0 ? open_fds((int)soft) : 0;
    int fds[2];
    int last = -1;
    while (soft > 0 && pipe(fds) == 0) {
        count += 2;
        last = fds[0];
    }
    int pair_emfile = errno == EMFILE;
    if (pair_emfile && count == soft - 1 && last >= 0) {
        int extra = dup(last);
        if (extra >= 0) {
            count++;
        }
    }
    errno = 0;
    int overflow = last >= 0 ? dup(last) : -1;
    int emfile = pair_emfile && count == soft && overflow < 0 && errno == EMFILE;

    if (inherited && emfile && soft == DEFAULT_SOFT_NOFILE && count == DEFAULT_SOFT_NOFILE) {
        say("proc-rlimit: exec-inherit=1 nofile-emfile=1 open-fds=1024 soft=1024\n");
        return 0;
    }
    say_number("proc-rlimit: fail inherit=", inherited);
    say_number("proc-rlimit: fail emfile=", emfile);
    say_number("proc-rlimit: fail count=", count);
    say_number("proc-rlimit: fail soft=", soft);
    say_number("proc-rlimit: fail errno=", errno);
    return 1;
}
