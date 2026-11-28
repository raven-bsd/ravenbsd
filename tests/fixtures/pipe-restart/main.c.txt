/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * S781 pipe-restart fixture: the NetBSD `t_pipe.c:pipe_restart` scenario at the libc boundary.
 *
 * The child blocks on a pipe read and then on a signal; the parent fills the pipe with 2 MiB and
 * blocks in write. SIGALRM interrupts that blocked write, the handler signals the child, and the
 * write must continue so every byte arrives. A lost wake, a swallowed EINTR or a byte dropped on
 * the interrupted path leaves one side blocked and the test prints the step it reached.
 */

#include <errno.h>
#include <signal.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

/* /init has no stdio fds; print through the kernel debug console (DEBUG_WRITE). */
static long debug_write(const char *bytes, size_t length) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t len asm("rsi") = length;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(len)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)debug_write(text, strlen(text));
}

static void say_count(const char *label, long value) {
    char digits[24];
    size_t used = 0;
    unsigned long magnitude;
    say(label);
    if (value < 0) {
        say("-");
        magnitude = (unsigned long)(-(value + 1)) + 1;
    } else {
        magnitude = (unsigned long)value;
    }
    do {
        digits[used++] = (char)('0' + (magnitude % 10));
        magnitude /= 10;
    } while (magnitude != 0 && used < sizeof(digits));
    while (used > 0) {
        used--;
        (void)debug_write(&digits[used], 1);
    }
    say("\n");
}

#define TOTAL (2L * 1024L * 1024L)
#define FIRST_READ (128L * 1024L)

static volatile sig_atomic_t child_signal_seen;
static pid_t reader_pid;

static void handler(int sig) {
    if (sig == SIGALRM) {
        (void)kill(reader_pid, SIGUSR1);
    } else if (sig == SIGUSR1) {
        child_signal_seen++;
    }
}

static volatile sig_atomic_t restart_signals;

static void restart_handler(int sig) {
    if (sig == SIGUSR2) {
        restart_signals++;
    }
}

/// The `SA_RESTART` row: the write blocks with no byte accepted, a caught signal interrupts the
/// wait, and the call must restart instead of returning `EINTR`.
static int restart_row(void) {
    int pipefd[2];
    int ready[2];
    pid_t pid;
    int status = 0;
    char byte = 0;
    ssize_t count;
    long total;

    if (signal(SIGUSR2, restart_handler) == SIG_ERR) {
        say("pipe-restart: fail restart signal\n");
        return 20;
    }
    if (pipe(pipefd) != 0 || pipe(ready) != 0) {
        say("pipe-restart: fail restart pipe\n");
        return 21;
    }
    pid = fork();
    if (pid < 0) {
        say("pipe-restart: fail restart fork\n");
        return 22;
    }
    if (pid == 0) {
        struct timespec pause = {1, 0};
        char *drain;
        long got = 0;
        ssize_t part;

        (void)close(pipefd[1]);
        (void)close(ready[0]);
        if (write(ready[1], &byte, 1) != 1) {
            _exit(30);
        }
        if (nanosleep(&pause, NULL) != 0) {
            _exit(31);
        }
        if (kill(getppid(), SIGUSR2) != 0) {
            _exit(32);
        }
        if (nanosleep(&pause, NULL) != 0) {
            _exit(33);
        }
        drain = malloc(65536 + 4096);
        if (drain == NULL) {
            _exit(34);
        }
        while ((part = read(pipefd[0], drain, 65536 + 4096)) > 0) {
            got += part;
        }
        free(drain);
        _exit(got == 65536 + 4096 ? 0 : 35);
    }
    (void)close(pipefd[0]);
    (void)close(ready[1]);
    if (read(ready[0], &byte, 1) != 1) {
        say("pipe-restart: fail restart ready\n");
        return 23;
    }
    (void)close(ready[0]);

    char *fill = malloc(65536);
    char *payload = malloc(4096);
    if (fill == NULL || payload == NULL) {
        say("pipe-restart: fail restart malloc\n");
        return 24;
    }
    memset(fill, 0x5a, 65536);
    memset(payload, 0x3c, 4096);
    if (write(pipefd[1], fill, 65536) != 65536) {
        say("pipe-restart: fail restart fill\n");
        free(fill);
        free(payload);
        return 25;
    }
    free(fill);

    restart_signals = 0;
    count = write(pipefd[1], payload, 4096);
    total = restart_signals;
    free(payload);
    say_count("pipe-restart: restart signals=", total);
    say_count("pipe-restart: restart write=", (long)count);
    (void)close(pipefd[1]);

    if (waitpid(pid, &status, 0) != pid) {
        say("pipe-restart: fail restart waitpid\n");
        return 26;
    }
    if (count != 4096 || total != 1 || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return 27;
    }
    say("pipe-restart: restart ok\n");
    return 0;
}

#ifdef S781_DONOR_ONLY
static volatile sig_atomic_t donor_nsiginfo;
static pid_t donor_pid;

/* The FreeBSD `contrib/netbsd-tests/lib/libc/sys/t_pipe.c` `pipe_restart` handler: the parent's
 * SIGALRM signals the child with SIGINFO, which is the donor's "status request" wake. */
static void donor_handler(int sig) {
    if (sig == SIGALRM) {
        say("pipe-restart: donor alarm\n");
        (void)kill(donor_pid, SIGINFO);
        say("pipe-restart: donor kill\n");
    }
    if (sig == SIGINFO) {
        donor_nsiginfo++;
    }
}

/// The donor row verbatim: 128 KiB first read, SIGINFO wake, alarm(2), 1 MiB drain. The marker
/// after each step names the last completed stage if the body still hangs.
static int donor_row(void) {
    int pp[2];
    int st = 0;
    ssize_t sz;
    ssize_t todo = TOTAL;
    ssize_t done;
    char *f;
    sigset_t asigset, osigset, emptysigset;
    pid_t pid;

    f = malloc((size_t)TOTAL);
    if (f == NULL) {
        say("pipe-restart: donor fail malloc\n");
        return 40;
    }
    memset(f, 0xa5, (size_t)TOTAL);
    sigemptyset(&emptysigset);
    sigemptyset(&asigset);
    sigaddset(&asigset, SIGINFO);
    if (signal(SIGINFO, donor_handler) == SIG_ERR) {
        say("pipe-restart: donor fail signal SIGINFO\n");
        return 41;
    }
    if (signal(SIGALRM, donor_handler) == SIG_ERR) {
        say("pipe-restart: donor fail signal SIGALRM\n");
        return 42;
    }
    if (pipe(pp) != 0) {
        say("pipe-restart: donor fail pipe\n");
        return 43;
    }
    pid = fork();
    if (pid < 0) {
        say("pipe-restart: donor fail fork\n");
        return 44;
    }
    if (pid == 0) {
        (void)close(pp[1]);
        done = read(pp[0], f, 128L * 1024L);
        say_count("pipe-restart: donor child first=", (long)done);
        if (done < 0) {
            _exit(50);
        }
        if (sigprocmask(SIG_BLOCK, &asigset, &osigset) != 0) {
            _exit(51);
        }
        say("pipe-restart: donor child waiting\n");
        while (donor_nsiginfo == 0) {
            if (sigsuspend(&emptysigset) != -1 || errno != EINTR) {
                _exit(52);
            }
        }
        if (sigprocmask(SIG_SETMASK, &osigset, NULL) != 0) {
            _exit(53);
        }
        say("pipe-restart: donor child signalled\n");
        while ((sz = read(pp[0], f, 1024L * 1024L)) > 0) {
            done += sz;
        }
        say_count("pipe-restart: donor child total=", (long)done);
        _exit(done != todo ? 54 : 0);
    }
    donor_pid = pid;
    (void)close(pp[0]);
    alarm(2);
    while (todo > 0 && (sz = write(pp[1], f, (size_t)todo)) > 0) {
        todo -= sz;
    }
    say_count("pipe-restart: donor parent remaining=", (long)todo);
    (void)close(pp[1]);
    if (waitpid(pid, &st, 0) != pid) {
        say("pipe-restart: donor fail waitpid\n");
        return 45;
    }
    say_count("pipe-restart: donor child code=", WEXITSTATUS(st));
    free(f);
    if (todo != 0 || !WIFEXITED(st) || WEXITSTATUS(st) != 0) {
        return 46;
    }
    say("pipe-restart: donor ok\n");
    return 0;
}
#endif

static int reader(int read_fd) {
    sigset_t block, old, empty;
    long total = 0;
    ssize_t count;

    char *buffer = malloc(FIRST_READ);
    if (buffer == NULL) {
        say("pipe-restart: fail child malloc\n");
        return 10;
    }
    count = read(read_fd, buffer, FIRST_READ);
    if (count < 0) {
        say("pipe-restart: fail child initial read\n");
        return 11;
    }
    total = count;
    say_count("pipe-restart: child first=", total);

    sigemptyset(&empty);
    sigemptyset(&block);
    sigaddset(&block, SIGUSR1);
    if (sigprocmask(SIG_BLOCK, &block, &old) != 0) {
        say("pipe-restart: fail child sigprocmask block\n");
        return 12;
    }
    while (child_signal_seen == 0) {
        if (sigsuspend(&empty) != -1 || errno != EINTR) {
            say("pipe-restart: fail child sigsuspend\n");
            return 13;
        }
    }
    if (sigprocmask(SIG_SETMASK, &old, NULL) != 0) {
        say("pipe-restart: fail child sigprocmask restore\n");
        return 14;
    }
    say("pipe-restart: child signalled\n");
    while ((count = read(read_fd, buffer, FIRST_READ)) > 0) {
        total += count;
    }
    if (count < 0) {
        say("pipe-restart: fail child drain read\n");
        return 15;
    }
    free(buffer);
    say_count("pipe-restart: child total=", total);
    return total == TOTAL ? 0 : 16;
}

#ifdef S781_DONOR_ONLY
/* The exact FreeBSD/NetBSD `pipe_restart` body in a fresh process: no preceding alarm may
 * leave the ITIMER_REAL state or signal dispositions touching this run. */
int main(void) {
    int donor = donor_row();
    if (donor != 0) {
        say("pipe-restart: fail\n");
        return donor;
    }
    say("pipe-restart: ok\n");
    return 0;
}
#else
int main(void) {
    int pipefd[2];
    pid_t pid;
    int status = 0;
    long todo = TOTAL;
    ssize_t count;

    if (signal(SIGUSR1, handler) == SIG_ERR || signal(SIGALRM, handler) == SIG_ERR) {
        say("pipe-restart: fail signal\n");
        return 1;
    }
    if (pipe(pipefd) != 0) {
        say("pipe-restart: fail pipe\n");
        return 2;
    }
    pid = fork();
    if (pid < 0) {
        say("pipe-restart: fail fork\n");
        return 3;
    }
    if (pid == 0) {
        (void)close(pipefd[1]);
        _exit(reader(pipefd[0]));
    }
    reader_pid = pid;
    (void)close(pipefd[0]);

    char *buffer = malloc(TOTAL);
    if (buffer == NULL) {
        say("pipe-restart: fail parent malloc\n");
        return 4;
    }
    memset(buffer, 0xa5, TOTAL);

    alarm(1);
    while (todo > 0) {
        errno = 0;
        count = write(pipefd[1], buffer, (size_t)todo);
        if (count <= 0) {
            say_count("pipe-restart: write ret=", (long)count);
            say_count("pipe-restart: write errno=", (long)errno);
            break;
        }
        todo -= count;
    }
    (void)alarm(0);
    say_count("pipe-restart: parent remaining=", todo);
    (void)close(pipefd[1]);
    free(buffer);

    if (waitpid(pid, &status, 0) != pid) {
        say("pipe-restart: fail waitpid\n");
        return 5;
    }
    if (!WIFEXITED(status)) {
        say_count("pipe-restart: fail child signal=", status);
        return 6;
    }
    say_count("pipe-restart: child code=", WEXITSTATUS(status));
    if (WEXITSTATUS(status) != 0 || todo != 0) {
        say("pipe-restart: fail\n");
        return 7;
    }
    int restart = restart_row();
    if (restart != 0) {
        say("pipe-restart: fail\n");
        return restart;
    }
    say("pipe-restart: ok\n");
    return 0;
}
#endif
