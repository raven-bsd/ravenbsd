// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K548 rev343: the static-program exec/startup split for Ken's pre-banner buckets.
//
// This case owns `_start` (tests/libc/target/build.sh omits crt1.o when the object defines it),
// so the first instruction after the kernel enters the image is observable. The parent records
// CLOCK_MONOTONIC before fork, after fork returns in the parent, and after waitpid. The child is
// this same static binary re-exec'd; `_start` records the kernel-entry point with a raw
// clock_gettime syscall (no libc init yet), the constructor records the end of libc init, main
// records entry, and the first write records the first-io point. All values are absolute guest
// monotonic nanoseconds, so the host can compute:
//   fork_ns       = exec_ret - fork_t
//   exec_to_start = start - fork_t      (fork + kernel execve + ELF load + first page faults)
//   start_to_ctor = ctor - start        (libc init, __raven_libc_start steps 1-15)
//   ctor_to_main  = main - ctor         (last init stage)
//   main_to_io    = io - main           (program body)
//   total_ns      = wait - fork_t
// The fixture prints the raw values; it does not claim a policy.

#include <signal.h>
#include <spawn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

extern char **environ;
extern int posix_spawn_file_actions_addchdir_np(posix_spawn_file_actions_t *, const char *);

// Raw `CLOCK_GET` (raven ABI 0x04, not the Linux 228) before libc init; CLOCK_MONOTONIC == 1.
static uint64_t raw_now_ns(void) {
    struct timespec ts;
    register long rax __asm__("rax") = 0x04;
    register long rdi __asm__("rdi") = 1;
    register void *rsi __asm__("rsi") = &ts;
    __asm__ volatile("syscall" : "+r"(rax) : "r"(rdi), "r"(rsi) : "rcx", "r11", "memory");
    if (rax != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static uint64_t start_ns;
static uint64_t ctor_ns;
static volatile int ctor_called;

__attribute__((constructor)) static void exec_timing_ctor(void) {
    ctor_called = 1;
    ctor_ns = now_ns();
}

extern void __raven_libc_start(void *sp0, unsigned long rtld_fini, unsigned long iplt_start,
                              unsigned long iplt_end, void *entry);

// `_start` mirrors crt1.o (libc/ravenlibc/crt/src/crt1.rs) and adds the entry timestamp. It must
// not use libc: the syscall is issued directly and no TLS/errno is touched.
__asm__(
    ".text\n"
    ".globl _start\n"
    ".type _start,@function\n"
    "_start:\n"
    "xor %ebp,%ebp\n"
    "mov %rsp,%rdi\n"
    "xor %esi,%esi\n"
    ".weak __rela_iplt_start\n"
    ".weak __rela_iplt_end\n"
    "lea __rela_iplt_start(%rip),%rdx\n"
    "lea __rela_iplt_end(%rip),%rcx\n"
    "lea main(%rip),%r8\n"
    "and $-16,%rsp\n"
    "call exec_measure_start\n"
    "ud2\n"
    ".size _start,.-_start\n"
    ".section .note.GNU-stack,\"\",%progbits\n");

__attribute__((no_stack_protector, used)) void exec_measure_start(void *sp0,
                                                                  unsigned long rtld_fini,
                                                                  unsigned long iplt_start,
                                                                  unsigned long iplt_end,
                                                                  void *entry) {
    start_ns = raw_now_ns();
    __raven_libc_start(sp0, rtld_fini, iplt_start, iplt_end, entry);
    __builtin_unreachable();
}

int main(int argc, char **argv) {
    if (argc > 1 && (strcmp(argv[1], "child") == 0 || strcmp(argv[1], "spawn-child") == 0)) {
        int spawned = strcmp(argv[1], "spawn-child") == 0;
        uint64_t main_ns = now_ns();
        char line[224];
        int length = snprintf(line, sizeof(line),
                              "EXEC-TIMING %s start=%llu ctor=%llu main=%llu pid=%d called=%d\n",
                              spawned ? "spawn-child" : "child",
                              (unsigned long long)start_ns, (unsigned long long)ctor_ns,
                              (unsigned long long)main_ns, (int)getpid(), (int)ctor_called);
        uint64_t io_ns = now_ns();
        (void)write(1, line, (size_t)length);
        length = snprintf(line, sizeof(line), "EXEC-TIMING %s io=%llu\n",
                          spawned ? "spawn-child" : "child", (unsigned long long)io_ns);
        (void)write(1, line, (size_t)length);
        return 0;
    }

    // K548 rev346: a parent-heap canary around the two child-creation paths. The child inherits
    // these pages copy-on-write and execs; the parent's bytes must survive both. A changed canary
    // after fork+execve or posix_spawn names a kernel COW/exec teardown defect or a libc spawn
    // descriptor defect, instead of the brush event table (the fault victim).
    size_t canary_len = 256u * 1024u;
    unsigned char *canary = malloc(canary_len);
    if (canary != NULL) {
        memset(canary, 0xA5, canary_len);
    }
    enum { CHUNK_COUNT = 1024, CHUNK_SIZE = 64 };
    unsigned char *chunks[CHUNK_COUNT];
    for (int index = 0; index < CHUNK_COUNT; index++) {
        chunks[index] = malloc(CHUNK_SIZE);
        if (chunks[index] != NULL) {
            memset(chunks[index], 0x5A, CHUNK_SIZE);
        }
    }
    uint64_t fork_ns = now_ns();
    pid_t child = fork();
    if (child == 0) {
        char *args[] = {"libc-test", "child", NULL};
        char *env[] = {NULL};
        (void)execve("/bin/libc-test", args, env);
        _exit(127);
    }
    uint64_t exec_ret_ns = now_ns();
    int status = 0;
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    uint64_t wait_ns = now_ns();
    int fork_ok = canary != NULL;
    if (canary != NULL) {
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                fork_ok = 0;
                break;
            }
        }
    }

    pid_t spawned = -1;
    int spawn_ok = canary != NULL;
    if (canary != NULL) {
        char *spawn_args[] = {"libc-test", "spawn-child", NULL};
        char *spawn_env[] = {NULL};
        if (posix_spawn(&spawned, "/bin/libc-test", NULL, NULL, spawn_args, spawn_env) == 0) {
            int spawn_status = 0;
            (void)waitpid(spawned, &spawn_status, 0);
        } else {
            spawn_ok = 0;
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                spawn_ok = 0;
                break;
            }
        }
    }

    int socket_ok = canary != NULL;
    if (canary != NULL) {
        int sv[2] = {-1, -1};
        if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) == 0) {
            posix_spawn_file_actions_t actions;
            if (posix_spawn_file_actions_init(&actions) == 0) {
                (void)posix_spawn_file_actions_adddup2(&actions, sv[0], 0);
                (void)posix_spawn_file_actions_adddup2(&actions, sv[0], 1);
                pid_t socket_child = -1;
                char *socket_args[] = {"libc-test", "spawn-child", NULL};
                char *socket_env[] = {NULL};
                if (posix_spawn(&socket_child, "/bin/libc-test", &actions, NULL,
                                socket_args, socket_env) == 0) {
                    int socket_status = 0;
                    (void)waitpid(socket_child, &socket_status, 0);
                } else {
                    socket_ok = 0;
                }
                (void)posix_spawn_file_actions_destroy(&actions);
            } else {
                socket_ok = 0;
            }
            (void)close(sv[0]);
            (void)close(sv[1]);
        } else {
            socket_ok = 0;
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                socket_ok = 0;
                break;
            }
        }
    }

    int spawnp_ok = canary != NULL;
    int big_ok = canary != NULL;
    if (canary != NULL) {
        int sv2[2] = {-1, -1};
        if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv2) == 0) {
            posix_spawn_file_actions_t actions2;
            posix_spawnattr_t attr;
            int ready = posix_spawn_file_actions_init(&actions2) == 0;
            if (ready) {
                ready = posix_spawnattr_init(&attr) == 0;
            }
            if (ready) {
                sigset_t empty;
                (void)sigemptyset(&empty);
                (void)posix_spawn_file_actions_adddup2(&actions2, sv2[0], 0);
                (void)posix_spawn_file_actions_adddup2(&actions2, sv2[0], 1);
                (void)posix_spawnattr_setsigdefault(&attr, &empty);
                (void)posix_spawnattr_setsigmask(&attr, &empty);
                (void)posix_spawnattr_setflags(&attr,
                                               POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK);
                char *spawnp_args[] = {"libc-test", "spawn-child", NULL};
                char *spawnp_env[] = {"PATH=/bin", "HOME=/root", "USER=root",
                                      "SHELL=/bin/brush",
                                      "SSH_CONNECTION=10.0.2.2 12345 10.0.2.15 22",
                                      "SSH_CLIENT=10.0.2.2 12345 22", "TERM=xterm", NULL};
                pid_t spawnp_child = -1;
                if (posix_spawnp(&spawnp_child, "libc-test", &actions2, &attr, spawnp_args,
                                 spawnp_env) == 0) {
                    int spawnp_status = 0;
                    (void)waitpid(spawnp_child, &spawnp_status, 0);
                } else {
                    spawnp_ok = 0;
                }
                char big_env_storage[32][160];
                char *big_env[34];
                for (int index = 0; index < 32; index++) {
                    (void)snprintf(big_env_storage[index], sizeof(big_env_storage[index]),
                                   "RAVEN_TEST_VAR_%02d=%0128d", index, index);
                    big_env[index] = big_env_storage[index];
                }
                big_env[32] = "PATH=/bin";
                big_env[33] = NULL;
                pid_t big_child = -1;
                if (posix_spawnp(&big_child, "libc-test", &actions2, &attr, spawnp_args,
                                 big_env) == 0) {
                    int big_status = 0;
                    (void)waitpid(big_child, &big_status, 0);
                } else {
                    big_ok = 0;
                }
                int big_canary_ok = 1;
                for (size_t index = 0; index < canary_len; index++) {
                    if (canary[index] != 0xA5) {
                        big_canary_ok = 0;
                        break;
                    }
                }
                if (!big_canary_ok) {
                    big_ok = 0;
                }
                (void)posix_spawnattr_destroy(&attr);
            } else {
                spawnp_ok = 0;
            }
            (void)posix_spawn_file_actions_destroy(&actions2);
            (void)close(sv2[0]);
            (void)close(sv2[1]);
        } else {
            spawnp_ok = 0;
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                spawnp_ok = 0;
                break;
            }
        }
    }

    int inh_ok = canary != NULL;
    if (canary != NULL) {
        int sv4[2] = {-1, -1};
        int saved_in = dup(0);
        int saved_out = dup(1);
        if (saved_in >= 0 && saved_out >= 0 && socketpair(AF_UNIX, SOCK_STREAM, 0, sv4) == 0) {
            posix_spawn_file_actions_t empty_actions;
            posix_spawnattr_t empty_attr;
            int ready = posix_spawn_file_actions_init(&empty_actions) == 0;
            if (ready) {
                ready = posix_spawnattr_init(&empty_attr) == 0;
            }
            if (ready) {
                (void)dup2(sv4[0], 0);
                (void)dup2(sv4[0], 1);
                pid_t inh_child = -1;
                char *inh_args[] = {"libc-test", "spawn-child", NULL};
                if (posix_spawn(&inh_child, "/bin/libc-test", &empty_actions, &empty_attr,
                                inh_args, environ) == 0) {
                    int inh_status = 0;
                    (void)waitpid(inh_child, &inh_status, 0);
                } else {
                    inh_ok = 0;
                }
                (void)dup2(saved_in, 0);
                (void)dup2(saved_out, 1);
                (void)posix_spawnattr_destroy(&empty_attr);
            } else {
                inh_ok = 0;
            }
            (void)posix_spawn_file_actions_destroy(&empty_actions);
            (void)close(sv4[0]);
            (void)close(sv4[1]);
        } else {
            inh_ok = 0;
        }
        if (saved_in >= 0) {
            (void)close(saved_in);
        }
        if (saved_out >= 0) {
            (void)close(saved_out);
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                inh_ok = 0;
                break;
            }
        }
    }

    int dup_ok = canary != NULL;
    if (canary != NULL) {
        int sv5[2] = {-1, -1};
        if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv5) == 0) {
            int dup_socket = dup(sv5[0]);
            posix_spawn_file_actions_t dup_actions;
            if (dup_socket >= 0 && posix_spawn_file_actions_init(&dup_actions) == 0) {
                (void)posix_spawn_file_actions_adddup2(&dup_actions, dup_socket, 0);
                (void)posix_spawn_file_actions_adddup2(&dup_actions, dup_socket, 1);
                pid_t dup_child = -1;
                char *dup_args[] = {"libc-test", "spawn-child", NULL};
                char *dup_env[] = {NULL};
                if (posix_spawn(&dup_child, "/bin/libc-test", &dup_actions, NULL, dup_args,
                                dup_env) == 0) {
                    int dup_status = 0;
                    (void)waitpid(dup_child, &dup_status, 0);
                } else {
                    dup_ok = 0;
                }
                (void)posix_spawn_file_actions_destroy(&dup_actions);
            } else {
                dup_ok = 0;
            }
            if (dup_socket >= 0) {
                (void)close(dup_socket);
            }
            (void)close(sv5[0]);
            (void)close(sv5[1]);
        } else {
            dup_ok = 0;
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                dup_ok = 0;
                break;
            }
        }
    }

    int same_ok = canary != NULL;
    if (canary != NULL) {
        int sv6[2] = {-1, -1};
        int saved_in = dup(0);
        int saved_out = dup(1);
        if (saved_in >= 0 && saved_out >= 0 && socketpair(AF_UNIX, SOCK_STREAM, 0, sv6) == 0) {
            posix_spawn_file_actions_t same_actions;
            posix_spawnattr_t same_attr;
            int ready = posix_spawn_file_actions_init(&same_actions) == 0;
            if (ready) {
                ready = posix_spawnattr_init(&same_attr) == 0;
            }
            if (ready) {
                (void)dup2(sv6[0], 0);
                (void)dup2(sv6[0], 1);
                (void)posix_spawn_file_actions_adddup2(&same_actions, 0, 0);
                (void)posix_spawn_file_actions_adddup2(&same_actions, 1, 1);
                pid_t same_child = -1;
                char *same_args[] = {"libc-test", "spawn-child", NULL};
                if (posix_spawn(&same_child, "/bin/libc-test", &same_actions, &same_attr,
                                same_args, environ) == 0) {
                    int same_status = 0;
                    (void)waitpid(same_child, &same_status, 0);
                } else {
                    same_ok = 0;
                }
                (void)dup2(saved_in, 0);
                (void)dup2(saved_out, 1);
                (void)posix_spawnattr_destroy(&same_attr);
            } else {
                same_ok = 0;
            }
            (void)posix_spawn_file_actions_destroy(&same_actions);
            (void)close(sv6[0]);
            (void)close(sv6[1]);
        } else {
            same_ok = 0;
        }
        if (saved_in >= 0) {
            (void)close(saved_in);
        }
        if (saved_out >= 0) {
            (void)close(saved_out);
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                same_ok = 0;
                break;
            }
        }
    }

    int chdir_ok = canary != NULL;
    if (canary != NULL) {
        posix_spawn_file_actions_t chdir_actions;
        if (posix_spawn_file_actions_init(&chdir_actions) == 0) {
            (void)posix_spawn_file_actions_addchdir_np(&chdir_actions, "/");
            pid_t chdir_child = -1;
            char *chdir_args[] = {"libc-test", "spawn-child", NULL};
            if (posix_spawn(&chdir_child, "/bin/libc-test", &chdir_actions, NULL, chdir_args,
                            environ) == 0) {
                int chdir_status = 0;
                (void)waitpid(chdir_child, &chdir_status, 0);
            } else {
                chdir_ok = 0;
            }
            (void)posix_spawn_file_actions_destroy(&chdir_actions);
        } else {
            chdir_ok = 0;
        }
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                chdir_ok = 0;
                break;
            }
        }
    }

    int heap_ok = canary != NULL;
    if (canary != NULL) {
        char *heap_args[3] = {NULL, NULL, NULL};
        char *heap_env[2] = {NULL, NULL};
        heap_args[0] = strdup("libc-test");
        heap_args[1] = strdup("spawn-child");
        heap_env[0] = strdup("PATH=/bin");
        if (heap_args[0] != NULL && heap_args[1] != NULL && heap_env[0] != NULL) {
            pid_t heap_child = -1;
            if (posix_spawn(&heap_child, "/bin/libc-test", NULL, NULL, heap_args, heap_env) == 0) {
                int heap_status = 0;
                (void)waitpid(heap_child, &heap_status, 0);
            } else {
                heap_ok = 0;
            }
        } else {
            heap_ok = 0;
        }
        free(heap_args[0]);
        free(heap_args[1]);
        free(heap_env[0]);
        for (size_t index = 0; index < canary_len; index++) {
            if (canary[index] != 0xA5) {
                heap_ok = 0;
                break;
            }
        }
    }

    int chunks_ok = 1;
    for (int index = 0; index < CHUNK_COUNT; index++) {
        if (chunks[index] == NULL) {
            chunks_ok = 0;
            continue;
        }
        for (int byte = 0; byte < CHUNK_SIZE; byte++) {
            if (chunks[index][byte] != 0x5A) {
                chunks_ok = 0;
                break;
            }
        }
    }

    printf("EXEC-TIMING parent fork=%llu exec_ret=%llu wait=%llu pid=%d status=%d\n",
           (unsigned long long)fork_ns, (unsigned long long)exec_ret_ns,
           (unsigned long long)wait_ns, (int)child, status);
    if (child > 0) {
        printf("EXEC-TIMING fork_ns=%llu\n", (unsigned long long)(exec_ret_ns - fork_ns));
        printf("EXEC-TIMING total_ns=%llu\n", (unsigned long long)(wait_ns - fork_ns));
    }
    printf("EXEC-TIMING canary fork=%s spawn=%s socket=%s spawnp=%s big=%s inh=%s dup=%s same=%s chdir=%s heap=%s chunks=%s\n",
           fork_ok ? "ok" : "bad", spawn_ok ? "ok" : "bad", socket_ok ? "ok" : "bad",
           spawnp_ok ? "ok" : "bad", big_ok ? "ok" : "bad", inh_ok ? "ok" : "bad",
           dup_ok ? "ok" : "bad", same_ok ? "ok" : "bad", chdir_ok ? "ok" : "bad",
           heap_ok ? "ok" : "bad", chunks_ok ? "ok" : "bad");
    fflush(stdout);
    for (int index = 0; index < CHUNK_COUNT; index++) {
        free(chunks[index]);
    }
    free(canary);

    // K548 rev349: the shell-like environment for the Rust std Command canary (the S4 brush
    // shell's exported env and cwd). The canary's Command::current_dir is /home/raven too.
    (void)setenv("HOME", "/home/raven", 1);
    (void)setenv("USER", "raven", 1);
    (void)setenv("LOGNAME", "raven", 1);
    (void)setenv("SHELL", "/bin/brush", 1);
    (void)setenv("PWD", "/home/raven", 1);
    (void)setenv("TERM", "xterm-256color", 1);
    (void)setenv("SSH_CONNECTION", "10.0.2.2 12345 10.0.2.15 22", 1);
    (void)setenv("SSH_CLIENT", "10.0.2.2 12345 22", 1);
    (void)setenv("SSH_TTY", "/dev/pts/0", 1);
    (void)setenv("LS_COLORS", "di=1;34:ln=1;36:so=1;35:pi=33:ex=1;32:bd=1;33:cd=1;33", 1);
    (void)setenv("GREP_COLORS", "mt=01;31:ms=01;32:mc=01;33:fn=35:ln=32:bn=32:se=36", 1);
    (void)setenv("PS1", "\\[\\e[38;5;183m\\]raven# \\[\\e[0m\\]", 1);
    (void)setenv("HISTFILE", "/tmp/brush_history", 1);
    (void)setenv("HISTCONTROL", "ignoreboth", 1);
    (void)setenv("HISTTIMEFORMAT", "%F %T ", 1);
    (void)setenv("HISTSIZE", "1000", 1);
    (void)setenv("HISTFILESIZE", "1000", 1);
    (void)setenv("RAVEN_PROFILE", "loaded", 1);
    (void)setenv("CLICOLOR", "1", 1);
    (void)setenv("RAVEN_BOBTHESHELL_LOADED", "1", 1);

    // K548 rev348: run the Rust std Command canary in a child of this tested process. Its own
    // output (EXEC-CANARY) is the first divergence check above the libc spawn copy.
    pid_t rust_canary = fork();
    if (rust_canary == 0) {
        char *rust_args[] = {"exec-canary", NULL};
        (void)execve("/bin/exec-canary", rust_args, environ);
        _exit(127);
    }
    if (rust_canary > 0) {
        int rust_status = 0;
        (void)waitpid(rust_canary, &rust_status, 0);
    }
    return child < 0 ? 1 : 0;
}
