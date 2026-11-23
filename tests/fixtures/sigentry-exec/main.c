/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K548 rev355: the program-side proof that exec clears the per-thread SIG_SETUP entry.
 *
 * The parent registers an absolute SIG_SETUP entry (the local `sigentry_stub`), prints the value
 * it registered, forks and execs this same static binary. The child must not call SIG_SETUP. It
 * maps the inherited absolute context page, sets the body cell to a function that prints the
 * entry it was invoked with, and kicks itself. If the kernel kept the parent's entry, the kick
 * enters `sigentry_stub`, whose body prints `child entry=0x<value>`. If exec cleared the entry,
 * the kick is a bare wake and the child prints `child entry=0x0`.
 *
 * No brush, no libc signal registration: the raw SIG_SETUP/THREAD_KICK/SIGRETURN contract is the
 * subject. The fixture runs as /bin/libc-test under the libc-test profile. */

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define RAVEN_SIG_SETUP 0x80
#define RAVEN_THREAD_KICK 0x81
#define RAVEN_HANDLE_NONE 0xffffffffUL
#define RAVEN_SIG_SETUP_ABS 2
#define RAVEN_PAGE 4096
#define CTX_ADDRESS ((void *)0x30000000UL)

/* The child sets this from its signal body; the parent never enters the stub. */
static volatile int entered;

void sigentry_stub(void);

/* The child's body: report the value it was invoked with and return to the stub's SIGRETURN. */
void sigentry_body(unsigned long bits, const void *fault) {
    char line[96];
    int length;

    (void)bits;
    (void)fault;
    entered = 1;
    length = snprintf(line, sizeof line, "sigentry-exec: child entry=0x%lx\n",
                      (unsigned long)(uintptr_t)&sigentry_stub);
    if (length > 0)
        (void)write(1, line, (size_t)length);
}

__asm__(
    ".text\n"
    ".global sigentry_stub\n"
    ".type sigentry_stub,@function\n"
    "sigentry_stub:\n"
    "  and $-16, %rsp\n"
    "  call sigentry_body\n"
    "  mov $0x82, %eax\n"
    "  syscall\n"
    "  ud2\n"
    ".size sigentry_stub, .-sigentry_stub\n");

void sigentry_stub(void);

/* RavenBSD raw syscall; the C library's syscall(2) is a deliberate ENOSYS stub. */
static inline long raven_syscall6(long number, long a0, long a1, long a2, long a3, long a4,
                                  long a5) {
    register long r10 __asm__("r10") = a3;
    register long r8 __asm__("r8") = a4;
    register long r9 __asm__("r9") = a5;
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(a0), "S"(a1), "d"(a2), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return result;
}

static int map_context(void) {
    void *mapped = mmap(CTX_ADDRESS, RAVEN_PAGE, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (mapped != CTX_ADDRESS)
        return -1;
    memset(mapped, 0, RAVEN_PAGE);
    return 0;
}

static int child_main(void) {
    struct timespec pause = {0, 20 * 1000 * 1000};
    long kicked;

    if (map_context() != 0) {
        printf("sigentry-exec: child map errno=%d\n", errno);
        return 1;
    }
    entered = 0;
    /* Handle 0 is the calling thread; the kernel delivers this kick only through a registered
     * SIG_SETUP entry. No SIG_SETUP call happens here. */
    kicked = raven_syscall6(RAVEN_THREAD_KICK, RAVEN_HANDLE_NONE, 1, 0, 0, 0, 0);
    if (kicked != 0) {
        printf("sigentry-exec: child kick rc=%ld\n", kicked);
        return 1;
    }
    (void)nanosleep(&pause, NULL);
    if (!entered) {
        printf("sigentry-exec: child entry=0x0\n");
        fflush(stdout);
    }
    return 0;
}

static int parent_main(void) {
    long setup;

    if (map_context() != 0) {
        printf("sigentry-exec: parent map errno=%d\n", errno);
        return 1;
    }
    setup = raven_syscall6(RAVEN_SIG_SETUP, (long)(uintptr_t)&sigentry_stub,
                           (long)(uintptr_t)CTX_ADDRESS, RAVEN_SIG_SETUP_ABS, 0, 0, 0);
    if (setup != 0) {
        printf("sigentry-exec: parent setup rc=%ld\n", setup);
        return 1;
    }
    printf("sigentry-exec: parent entry=0x%lx setup=0\n",
           (unsigned long)(uintptr_t)&sigentry_stub);
    fflush(stdout);
    /* Replace this image with the same binary. The child mode does not call SIG_SETUP, so the
     * kick reports whatever entry the kernel kept across this exec. */
    execl("/bin/libc-test", "libc-test", "--sigentry-child", (char *)NULL);
    _exit(3);
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc > 1 && strcmp(argv[1], "--sigentry-child") == 0)
        return child_main();
    return parent_main();
}
