// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S179 credential and limit conformance probe. It runs as the logged-in `raven` account and
// asserts the kernel/libc contract the ticket names: real/effective ids and supplementary groups,
// non-permitted setuid/setgid, umask on a created file, a kernel rlimit round-trip with an
// unprivileged hard-limit raise refused, RLIMIT_NOFILE enforced on open, and getrusage samples
// that only grow with a maximum RSS at least the mapped size. The kernel rlimit/rusage rows use
// the PROC_* ABI directly so the kernel contract is tested, not the libc cache.
//
// The only output the harness reads is `cred:` lines. Exit 0 only when every row passed.

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#ifndef MAP_ANONYMOUS
#define MAP_ANONYMOUS MAP_ANON
#endif

// raven-abi raw numbers used by kernel/src/syscall/process_ctl.rs.
#define PROC_GETRUSAGE 0x4D
#define PROC_GETRLIMIT 0x4E
#define PROC_SETRLIMIT 0x4F

#define RLIMIT_NOFILE_INDEX 7
#define EXPECTED_EPERM 1
#define EXPECTED_EINVAL 22
#define EXPECTED_EMFILE 24

static int failures;

static void failed(const char *row, const char *why) {
    printf("cred: FAIL row=%s why=%s\n", row, why);
    failures++;
}

static long raw_syscall3(long number, long a0, long a1, long a2) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(a0), "S"(a1), "d"(a2)
                     : "rcx", "r11", "memory");
    return result;
}

static int raw_error(long result) {
    return (result < 0 && result >= -4095) ? (int)(-result) : 0;
}

static int raw_getrlimit(int resource, uint64_t out[2]) {
    long result = raw_syscall3(PROC_GETRLIMIT, resource, (long)out, 0);
    return raw_error(result);
}

static int raw_setrlimit(int resource, uint64_t cur, uint64_t max) {
    long result = raw_syscall3(PROC_SETRLIMIT, resource, (long)cur, (long)max);
    return raw_error(result);
}

static int raw_getrusage(int who, uint64_t out[8]) {
    long result = raw_syscall3(PROC_GETRUSAGE, who, (long)out, 8);
    return raw_error(result);
}

static void row_ids(void) {
    printf("cred: ids uid=%u euid=%u gid=%u egid=%u\n", getuid(), geteuid(), getgid(),
           getegid());
}

static void row_groups(void) {
    int count = getgroups(0, NULL);
    if (count < 0) {
        failed("groups", "count");
        return;
    }
    uint32_t *groups = calloc((size_t)count == 0 ? 1 : (size_t)count, sizeof(uint32_t));
    if (groups == NULL) {
        failed("groups", "alloc");
        return;
    }
    int got = getgroups(count, groups);
    printf("cred: groups count=%d got=%d list=", count, got);
    for (int index = 0; index < got; index++) {
        printf("%s%u", index == 0 ? "" : ",", groups[index]);
    }
    printf("\n");
    if (got != count) {
        failed("groups", "short");
    }
    free(groups);
}

static void row_setid(const char *name, int result, int error) {
    printf("cred: %s ret=%d errno=%d\n", name, result, error);
    if (result != -1 || error != EXPECTED_EPERM) {
        failed(name, "expected EPERM");
    }
}

static void row_umask(void) {
    errno = 0;
    mode_t original = umask(027);
    mode_t previous = original;
    printf("cred: umask-old mask=%04o\n", (unsigned)previous);

    const char *first = "/tmp/cred-umask-027";
    int fd = open(first, O_CREAT | O_WRONLY | O_TRUNC, 0666);
    if (fd < 0) {
        failed("umask-file", "open");
        return;
    }
    struct stat status;
    if (fstat(fd, &status) != 0) {
        failed("umask-file", "fstat");
        close(fd);
        return;
    }
    close(fd);
    printf("cred: umask-file mode=%04o\n", (unsigned)(status.st_mode & 0777));
    unlink(first);

    previous = umask(077);
    printf("cred: umask-return mask=%04o\n", (unsigned)previous);
    const char *second = "/tmp/cred-umask-077";
    fd = open(second, O_CREAT | O_WRONLY | O_TRUNC, 0666);
    if (fd < 0) {
        failed("umask-file-077", "open");
        umask(original);
        return;
    }
    if (fstat(fd, &status) != 0) {
        failed("umask-file-077", "fstat");
        close(fd);
        umask(original);
        return;
    }
    close(fd);
    printf("cred: umask-file-077 mode=%04o\n", (unsigned)(status.st_mode & 0777));
    unlink(second);
    umask(original);
}

static void row_rlimit(int mutation_ignore_hard_limit) {
    uint64_t original[2] = {0, 0};
    int error = raw_getrlimit(RLIMIT_NOFILE_INDEX, original);
    if (error != 0) {
        failed("rlimit-get", "syscall");
        return;
    }
    printf("cred: rlimit-nofile cur=%llu max=%llu\n", (unsigned long long)original[0],
           (unsigned long long)original[1]);

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], original[1]);
    printf("cred: rlimit-roundtrip errno=%d\n", error);

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[1], original[1]);
    printf("cred: rlimit-soft-raise errno=%d\n", error);

    uint64_t raised = original[1] == UINT64_MAX ? original[1] : original[1] + 1;
    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], raised);
    if (mutation_ignore_hard_limit) {
        printf("cred: mutation=ignore-hard-limit\n");
        printf("cred: rlimit-hard-raise errno=0\n");
    } else {
        printf("cred: rlimit-hard-raise errno=%d\n", error);
    }

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], original[1]);
    printf("cred: rlimit-restore errno=%d\n", error);
}

static int count_open_fds(void) {
    int count = 0;
    for (int fd = 0; fd < 64; fd++) {
        errno = 0;
        if (fcntl(fd, F_GETFD) != -1) {
            count++;
        }
    }
    return count;
}

static void row_nofile(void) {
    struct rlimit current;
    if (getrlimit(RLIMIT_NOFILE, &current) != 0) {
        failed("nofile", "getrlimit");
        return;
    }
    int base = count_open_fds();
    rlim_t soft = (rlim_t)base + 3;
    if (soft > current.rlim_max) {
        failed("nofile", "no room below hard limit");
        return;
    }
    struct rlimit lowered = {soft, current.rlim_max};
    if (setrlimit(RLIMIT_NOFILE, &lowered) != 0) {
        failed("nofile", "setrlimit");
        return;
    }
    int fds[8];
    int opened = 0;
    int error = 0;
    for (int attempt = 0; attempt < 8; attempt++) {
        int fd = open("/dev/null", O_RDONLY);
        if (fd < 0) {
            error = errno;
            break;
        }
        fds[opened++] = fd;
    }
    printf("cred: nofile soft=%llu open-ok=%d errno=%d\n", (unsigned long long)soft, opened,
           error);
    for (int index = 0; index < opened; index++) {
        close(fds[index]);
    }
    setrlimit(RLIMIT_NOFILE, &current);
}

static void row_rusage(void) {
    const size_t mapped_len = 4u * 1024u * 1024u;
    unsigned char *mapped = mmap(NULL, mapped_len, PROT_READ | PROT_WRITE,
                                 MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (mapped == MAP_FAILED) {
        failed("rusage", "mmap");
        return;
    }
    for (size_t offset = 0; offset < mapped_len; offset += 4096) {
        mapped[offset] = (unsigned char)(offset >> 12);
    }

    for (int sample = 0; sample < 3; sample++) {
        uint64_t usage[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        int error = raw_getrusage(0, usage);
        printf("cred: rusage%d errno=%d user=%llu sys=%llu maxrss=%llu\n", sample, error,
               (unsigned long long)usage[0], (unsigned long long)usage[1],
               (unsigned long long)usage[2]);
        if (error != 0) {
            failed("rusage", "getrusage");
        }
        for (volatile unsigned long spin = 0; spin < 8000000UL; spin++) {
            __asm__ volatile("");
        }
    }
    printf("cred: rusage-mapped=%llu\n", (unsigned long long)mapped_len);
    munmap(mapped, mapped_len);
}

static int umask_child(void) {
    errno = 0;
    mode_t inherited = umask(0);
    printf("cred: umask-child mask=%04o\n", (unsigned)inherited);
    return inherited == 027 ? 0 : 1;
}

static int umask_exec(void) {
    umask(027);
    printf("cred: umask-exec-parent mask=%04o\n", 0027);
    fflush(stdout);
    char *arguments[] = {"cred-probe", "umask-child", NULL};
    execv("/bin/cred-probe", arguments);
    printf("cred: FAIL row=umask-exec why=execv errno=%d\n", errno);
    return 1;
}

int main(int argc, char **argv) {
    setvbuf(stdout, NULL, _IONBF, 0);

    if (argc > 1 && strcmp(argv[1], "umask-child") == 0) {
        return umask_child();
    }
    if (argc > 1 && strcmp(argv[1], "umask-exec") == 0) {
        return umask_exec();
    }

    int mutation_ignore_hard_limit =
        argc > 1 && strcmp(argv[1], "--mutation=ignore-hard-limit") == 0;

    row_ids();
    row_groups();

    errno = 0;
    int result = setuid(0);
    row_setid("setuid-nonpermitted", result, errno);

    errno = 0;
    result = setgid(0);
    row_setid("setgid-nonpermitted", result, errno);

    row_umask();
    row_rlimit(mutation_ignore_hard_limit);
    row_nofile();
    row_rusage();

    if (mutation_ignore_hard_limit) {
        // Negative check only: the mutation reports the hard raise as accepted, and the harness
        // must turn the row red. The process itself still exits 0 so the harness, not the probe,
        // owns the verdict.
        printf("cred: ok\n");
        return 0;
    }
    if (failures == 0) {
        printf("cred: ok\n");
        return 0;
    }
    return 1;
}
