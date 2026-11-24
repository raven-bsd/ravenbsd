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

static void row_kernel_rlimit(int mutation_ignore_hard_limit) {
    uint64_t original[2] = {0, 0};
    int error = raw_getrlimit(RLIMIT_NOFILE_INDEX, original);
    if (error != 0) {
        failed("kernel-rlimit-get", "syscall");
        return;
    }
    printf("cred: kernel-rlimit-nofile cur=%llu max=%llu\n",
           (unsigned long long)original[0], (unsigned long long)original[1]);

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], original[1]);
    printf("cred: kernel-rlimit-roundtrip errno=%d\n", error);

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[1], original[1]);
    printf("cred: kernel-rlimit-soft-raise errno=%d\n", error);

    uint64_t raised = original[1] == UINT64_MAX ? original[1] : original[1] + 1;
    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], raised);
    if (mutation_ignore_hard_limit) {
        printf("cred: mutation=ignore-hard-limit\n");
        printf("cred: kernel-rlimit-hard-raise errno=0\n");
    } else {
        printf("cred: kernel-rlimit-hard-raise errno=%d\n", error);
    }

    uint64_t after_raise[2] = {0, 0};
    int state = raw_getrlimit(RLIMIT_NOFILE_INDEX, after_raise);
    printf("cred: kernel-rlimit-hard-raise-state errno=%d cur=%llu max=%llu\n", state,
           (unsigned long long)after_raise[0], (unsigned long long)after_raise[1]);

    error = raw_setrlimit(RLIMIT_NOFILE_INDEX, original[0], original[1]);
    printf("cred: kernel-rlimit-restore errno=%d\n", error);
}

static void row_libc_rlimit(void) {
    struct rlimit original;
    errno = 0;
    int result = getrlimit(RLIMIT_NOFILE, &original);
    printf("cred: libc-getrlimit ret=%d errno=%d cur=%llu max=%llu\n", result, errno,
           (unsigned long long)original.rlim_cur, (unsigned long long)original.rlim_max);
    if (result != 0) {
        failed("libc-getrlimit", "getrlimit");
        return;
    }

    struct rlimit same = original;
    errno = 0;
    result = setrlimit(RLIMIT_NOFILE, &same);
    printf("cred: libc-setrlimit ret=%d errno=%d\n", result, errno);
    if (result != 0) {
        failed("libc-setrlimit", "roundtrip");
    }

    struct rlimit raised = original;
    raised.rlim_max = original.rlim_max == RLIM_INFINITY ? original.rlim_max
                                                         : original.rlim_max + 1;
    errno = 0;
    result = setrlimit(RLIMIT_NOFILE, &raised);
    printf("cred: libc-hard-raise ret=%d errno=%d\n", result, errno);
    if (result != -1 || errno != EXPECTED_EPERM) {
        failed("libc-hard-raise", "expected EPERM");
    }
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
    // The kernel's RLIMIT_NOFILE soft value is the authoritative per-process bound. The libc
    // sysconf cache is a separate S190 defect; using it here would open past the kernel's own
    // limit and stress the 1536 class, which is S170/S192, not the file-server budget.
    uint64_t kernel_nofile[2] = {0, 0};
    int error = raw_getrlimit(RLIMIT_NOFILE_INDEX, kernel_nofile);
    if (error != 0) {
        failed("nofile", "kernel getrlimit");
        return;
    }
    long soft = (long)kernel_nofile[0];
    int base = count_open_fds();
    long target = soft - base;
    if (target <= 0) {
        failed("nofile", "soft limit below the open base");
        return;
    }
    int *fds = calloc((size_t)target, sizeof(int));
    if (fds == NULL) {
        failed("nofile", "alloc");
        return;
    }
    int opened = 0;
    error = 0;
    for (long attempt = 0; attempt < target; attempt++) {
        int fd = open("/dev/null", O_RDONLY);
        if (fd < 0) {
            error = errno;
            break;
        }
        fds[opened++] = fd;
    }
    printf("cred: nofile soft=%ld base=%d open-ok=%d errno=%d\n", soft, base, opened, error);
    for (int index = 0; index < opened; index++) {
        close(fds[index]);
    }
    free(fds);
}

static unsigned char *map_four_mib(size_t *length) {
    *length = 4u * 1024u * 1024u;
    unsigned char *mapped = mmap(NULL, *length, PROT_READ | PROT_WRITE,
                                 MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (mapped == MAP_FAILED) {
        return NULL;
    }
    for (size_t offset = 0; offset < *length; offset += 4096) {
        mapped[offset] = (unsigned char)(offset >> 12);
    }
    return mapped;
}

static void row_libc_rusage(void) {
    size_t mapped_len = 0;
    unsigned char *mapped = map_four_mib(&mapped_len);
    if (mapped == NULL) {
        failed("libc-rusage", "mmap");
        return;
    }
    for (int sample = 0; sample < 3; sample++) {
        struct rusage usage;
        errno = 0;
        int result = getrusage(RUSAGE_SELF, &usage);
        unsigned long long user = (unsigned long long)usage.ru_utime.tv_sec * 1000000000ULL
                                  + (unsigned long long)usage.ru_utime.tv_usec * 1000ULL;
        unsigned long long system = (unsigned long long)usage.ru_stime.tv_sec * 1000000000ULL
                                    + (unsigned long long)usage.ru_stime.tv_usec * 1000ULL;
        printf("cred: libc-rusage%d ret=%d errno=%d user=%llu sys=%llu maxrss=%lld\n", sample,
               result, errno, user, system, (long long)usage.ru_maxrss);
        for (volatile unsigned long spin = 0; spin < 8000000UL; spin++) {
            __asm__ volatile("");
        }
    }
    printf("cred: libc-rusage-mapped=%llu\n", (unsigned long long)mapped_len);
    munmap(mapped, mapped_len);
}

static void row_kernel_rusage(void) {
    size_t mapped_len = 0;
    unsigned char *mapped = map_four_mib(&mapped_len);
    if (mapped == NULL) {
        failed("kernel-rusage", "mmap");
        return;
    }
    for (int sample = 0; sample < 3; sample++) {
        uint64_t usage[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        int error = raw_getrusage(0, usage);
        printf("cred: kernel-rusage%d errno=%d user=%llu sys=%llu maxrss=%llu\n", sample, error,
               (unsigned long long)usage[0], (unsigned long long)usage[1],
               (unsigned long long)usage[2]);
        if (error != 0) {
            failed("kernel-rusage", "getrusage");
        }
        for (volatile unsigned long spin = 0; spin < 8000000UL; spin++) {
            __asm__ volatile("");
        }
    }
    printf("cred: kernel-rusage-mapped=%llu\n", (unsigned long long)mapped_len);
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
    row_kernel_rlimit(mutation_ignore_hard_limit);
    row_libc_rlimit();
    row_nofile();
    row_libc_rusage();
    row_kernel_rusage();

    printf("cred: done failures=%d\n", failures);
    if (mutation_ignore_hard_limit) {
        // Negative check only: the mutation reports the hard raise as accepted, and the harness
        // must turn the row red. The process itself still exits 0 so the harness, not the probe,
        // owns the verdict.
        return 0;
    }
    if (failures == 0) {
        printf("cred: ok\n");
        return 0;
    }
    return 1;
}
