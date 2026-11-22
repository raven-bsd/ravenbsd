// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K478 VFS path fuzz. It runs as a one-shot svcd service after vfsd/initfsd are ready and
// issues malformed paths through the kernel OPEN path (libc `open` and raw OPEN) and the vfsd
// CALL path (libc `lstat`/`readlink`). The chroot rows run last: after chroot("/etc") the test
// tries to reach the real /init (outside) by `..`, a fresh OPEN and a cached-root lstat. Any
// success is printed as ESCAPE and exits non-zero; a non-escape unexpected result is a deviation
// and is reported but does not hide the survival check.

#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#define OPEN_READ_FLAG 1 /* raven wire O_RDONLY for a raw OPEN */
#define MAX_HASH_ROWS 128

static long raven_debug_write(const char *bytes, size_t len) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t length asm("rsi") = len;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(length)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)raven_debug_write(text, strlen(text));
}

static void sayf(const char *format, ...) {
    char line[320];
    va_list args;
    va_start(args, format);
    int length = vsnprintf(line, sizeof(line), format, args);
    va_end(args);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static unsigned long table_hash = 0xcbf29ce484222325UL;
static int rows;
static int deviations;
static int escapes;

static unsigned long fnv(unsigned long hash, const void *bytes, size_t length) {
    const unsigned char *data = bytes;
    for (size_t index = 0; index < length; index++) {
        hash ^= data[index];
        hash *= 0x100000001b3UL;
    }
    return hash;
}

static void row(const char *id, int ok, const char *detail) {
    sayf("VFS %s %s %s\n", id, ok ? "PASS" : "FAIL", detail);
    table_hash = fnv(table_hash, id, strlen(id));
    table_hash = fnv(table_hash, &ok, sizeof(ok));
    table_hash = fnv(table_hash, detail, strlen(detail));
    rows++;
    if (!ok) {
        deviations++;
    }
}

static void escape(const char *id, const char *path, const char *resolved) {
    sayf("vfs-fuzz: ESCAPE id=%s path=%s resolved=%s\n", id, path, resolved);
    table_hash = fnv(table_hash, id, strlen(id));
    escapes++;
}

static int errno_is(int err, const int *allowed, int count) {
    for (int index = 0; index < count; index++) {
        if (err == allowed[index]) {
            return 1;
        }
    }
    return 0;
}

static int open_probe(const char *path, int *saved_errno) {
    errno = 0;
    int fd = open(path, O_RDONLY);
    *saved_errno = errno;
    if (fd >= 0) {
        (void)close(fd);
    }
    return fd;
}

static int legal_open(void) {
    int err = 0;
    return open_probe("/etc/mounts", &err) >= 0;
}

static void expect_error(const char *id, const char *path, const int *allowed, int count) {
    int err = 0;
    int fd = open_probe(path, &err);
    if (fd >= 0) {
        char detail[200];
        (void)snprintf(detail, sizeof(detail), "path=%s unexpected success", path);
        row(id, 0, detail);
    } else {
        int ok = errno_is(err, allowed, count);
        char detail[200];
        (void)snprintf(detail, sizeof(detail), "path=%s errno=%d", path, err);
        row(id, ok, detail);
    }
    if (!legal_open()) {
        row("next-legal", 0, "open(/etc/mounts) after the vector");
    }
}

static long raw_open(const char *path, size_t length, int flags) {
    register long number asm("rax") = 0x61; /* Syscall::OPEN */
    register const char *pointer asm("rdi") = path;
    register size_t size asm("rsi") = length;
    register long wire_flags asm("rdx") = flags;
    register long mode asm("r10") = 0;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(size), "r"(wire_flags), "r"(mode)
                     : "rcx", "r11", "memory");
    return number;
}

static long raw_close(long handle) {
    register long number asm("rax") = 0x64; /* Syscall::CLOSE */
    register long value asm("rdi") = handle;
    __asm__ volatile("syscall" : "+r"(number) : "r"(value) : "rcx", "r11", "memory");
    return number;
}

static void raw_expect_error(const char *id, const char *path, size_t length, const int *allowed,
                             int count) {
    long result = raw_open(path, length, OPEN_READ_FLAG);
    if (result >= 0) {
        (void)raw_close(result);
        char detail[220];
        (void)snprintf(detail, sizeof(detail), "raw path length=%zu unexpected success", length);
        row(id, 0, detail);
    } else {
        int err = (int)-result;
        int ok = errno_is(err, allowed, count);
        char detail[220];
        (void)snprintf(detail, sizeof(detail), "raw length=%zu errno=%d", length, err);
        row(id, ok, detail);
    }
    if (!legal_open()) {
        row("next-legal", 0, "open(/etc/mounts) after the raw vector");
    }
}

static void component_path(char *out, size_t capacity, size_t components, size_t component_len) {
    size_t at = 0;
    out[at++] = '/';
    for (size_t index = 0; index < components && at + component_len + 2 < capacity; index++) {
        for (size_t byte = 0; byte < component_len && at + 2 < capacity; byte++) {
            out[at++] = 'a';
        }
        if (index + 1 < components) {
            out[at++] = '/';
        }
    }
    out[at] = '\0';
}

static void lstat_probe(const char *id, const char *path, const int *allowed, int count) {
    struct stat status;
    errno = 0;
    int result = lstat(path, &status);
    int err = errno;
    char detail[220];
    if (result == 0) {
        int ok = errno_is(0, allowed, count);
        (void)snprintf(detail, sizeof(detail), "path=%s success mode=0%o", path,
                       (unsigned)status.st_mode);
        row(id, ok, detail);
    } else {
        int ok = errno_is(err, allowed, count);
        (void)snprintf(detail, sizeof(detail), "path=%s errno=%d", path, err);
        row(id, ok, detail);
    }
    if (!legal_open()) {
        row("next-legal", 0, "open(/etc/mounts) after the lstat vector");
    }
}

static void readlink_probe(const char *id, const char *path, const int *allowed, int count) {
    char target[64];
    errno = 0;
    ssize_t result = readlink(path, target, sizeof(target));
    int err = errno;
    char detail[220];
    int ok = result < 0 && errno_is(err, allowed, count);
    (void)snprintf(detail, sizeof(detail), "path=%s result=%ld errno=%d", path, (long)result, err);
    row(id, ok, detail);
    if (!legal_open()) {
        row("next-legal", 0, "open(/etc/mounts) after the readlink vector");
    }
}

static int read_elf(int fd) {
    unsigned char magic[4] = {0, 0, 0, 0};
    ssize_t got = read(fd, magic, sizeof(magic));
    return got == 4 && magic[0] == 0x7f && magic[1] == 'E' && magic[2] == 'L' && magic[3] == 'F';
}

/// Open for a read probe that must survive the read; returns the fd and fills `*elf`.
static int open_magic(const char *path, int *saved_errno, int *elf) {
    errno = 0;
    int fd = open(path, O_RDONLY);
    *saved_errno = errno;
    if (fd < 0) {
        *elf = 0;
        return -1;
    }
    *elf = read_elf(fd);
    return fd;
}

static void chroot_rows(void) {
    const int enoent_only[] = {ENOENT};
    const int open_errors[] = {ENOENT, EACCES, EINVAL, EPERM, ENAMETOOLONG, ENOTDIR};
    struct stat status;
    int err = 0;
    int fd;
    int elf = 0;

    /* Populate the libc cached `file:/` root before the chroot, then chroot. */
    memset(&status, 0, sizeof(status));
    if (lstat("/etc/mounts", &status) == 0) {
        row("L01", 1, "lstat(/etc/mounts) cached root before chroot");
    } else {
        row("L01", 0, "lstat(/etc/mounts) before chroot failed");
    }
    if (chroot("/etc") != 0) {
        int chroot_err = errno;
        char detail[120];
        (void)snprintf(detail, sizeof(detail), "chroot(/etc) errno=%d", chroot_err);
        row("CH00", 0, detail);
        sayf("vfs-fuzz: coverage-gap chroot requires euid 0; errno=%d\n", chroot_err);
        return;
    }
    row("CH00", 1, "chroot(/etc)=0");

    fd = open_magic("/mounts", &err, &elf);
    row("CH01", fd >= 0, "inside open(/mounts) after chroot");
    if (fd >= 0) {
        row("CH01b", !elf, "inside /mounts is not an ELF image");
        (void)close(fd);
    }

    fd = open_magic("/../mounts", &err, &elf);
    row("CH02", fd >= 0, "clamped open(/../mounts) stays inside /etc");
    if (fd >= 0) {
        row("CH02b", !elf, "clamped /../mounts is the mount table, not /init");
        (void)close(fd);
    }

    fd = open_magic("/init", &err, &elf);
    if (fd >= 0) {
        char resolved[80];
        (void)snprintf(resolved, sizeof(resolved), "fresh-open ELF=%d", elf);
        escape("CH03", "/init", resolved);
        row("CH03", 0, "open(/init) escaped the chroot");
        (void)close(fd);
    } else {
        row("CH03", errno_is(err, enoent_only, 1), "open(/init) refused after chroot");
    }

    fd = open_magic("/../init", &err, &elf);
    if (fd >= 0) {
        char resolved[80];
        (void)snprintf(resolved, sizeof(resolved), "fresh-open ELF=%d", elf);
        escape("CH04", "/../init", resolved);
        row("CH04", 0, "open(/../init) escaped the chroot");
        (void)close(fd);
    } else {
        row("CH04", errno_is(err, enoent_only, 1), "open(/../init) refused after chroot");
    }

    long raw = raw_open("file:/init", strlen("file:/init"), OPEN_READ_FLAG);
    if (raw >= 0) {
        escape("CH05", "file:/init", "raw OPEN success");
        row("CH05", 0, "raw OPEN file:/init escaped the chroot");
        (void)raw_close(raw);
    } else {
        row("CH05", errno_is((int)-raw, enoent_only, 1), "raw OPEN file:/init refused");
    }

    raw = raw_open("file:/../init", strlen("file:/../init"), OPEN_READ_FLAG);
    if (raw >= 0) {
        escape("CH06", "file:/../init", "raw OPEN success");
        row("CH06", 0, "raw OPEN file:/../init escaped the chroot");
        (void)raw_close(raw);
    } else {
        row("CH06", errno_is((int)-raw, enoent_only, 1), "raw OPEN file:/../init refused");
    }

    memset(&status, 0, sizeof(status));
    errno = 0;
    if (lstat("/init", &status) == 0) {
        char resolved[80];
        (void)snprintf(resolved, sizeof(resolved), "lstat mode=0%o size=%ld",
                       (unsigned)status.st_mode, (long)status.st_size);
        escape("CH07", "lstat(/init)", resolved);
        row("CH07", 0, "cached-root lstat(/init) escaped the chroot");
    } else {
        int err2 = errno;
        row("CH07", errno_is(err2, open_errors, 6), "cached-root lstat(/init) refused");
    }

    memset(&status, 0, sizeof(status));
    errno = 0;
    if (lstat("/../init", &status) == 0) {
        char resolved[80];
        (void)snprintf(resolved, sizeof(resolved), "lstat mode=0%o size=%ld",
                       (unsigned)status.st_mode, (long)status.st_size);
        escape("CH08", "lstat(/../init)", resolved);
        row("CH08", 0, "cached-root lstat(/../init) escaped the chroot");
    } else {
        int err3 = errno;
        row("CH08", errno_is(err3, open_errors, 6), "cached-root lstat(/../init) refused");
    }

    memset(&status, 0, sizeof(status));
    errno = 0;
    if (lstat("/../../init", &status) == 0) {
        char resolved[80];
        (void)snprintf(resolved, sizeof(resolved), "lstat mode=0%o size=%ld",
                       (unsigned)status.st_mode, (long)status.st_size);
        escape("CH09", "lstat(/../../init)", resolved);
        row("CH09", 0, "cached-root lstat(/../../init) escaped the chroot");
    } else {
        int err4 = errno;
        row("CH09", errno_is(err4, open_errors, 6), "cached-root lstat(/../../init) refused");
    }

    memset(&status, 0, sizeof(status));
    errno = 0;
    if (lstat("/mounts", &status) == 0) {
        row("CH10", 1, "in-root lstat(/mounts) after chroot succeeds");
    } else {
        int err5 = errno;
        char detail[80];
        (void)snprintf(detail, sizeof(detail), "in-root lstat(/mounts) errno=%d", err5);
        row("CH10", 0, detail);
    }
}

int main(void) {
    static char path[4200];
    const int open_errors[] = {ENOENT, EINVAL, EACCES, ENOTDIR, ENAMETOOLONG, EFAULT, EPERM};
    const int empty_errors[] = {EINVAL, ENOENT};
    const int long_errors[] = {ENAMETOOLONG, ENOENT};
    const int nonutf_errors[] = {EINVAL, ENOENT};

    say("vfs-fuzz: start seed=0x4b343738 vfs\n");

    /* Legal controls. */
    int err = 0;
    int fd = open_probe("/etc/mounts", &err);
    row("P01", fd >= 0, "open(/etc/mounts) legal control");
    fd = open_probe("/", &err);
    row("P02", fd >= 0, "open(/) legal root directory");

    /* Empty, no scheme, two colons. */
    expect_error("P03", "", empty_errors, 2);
    {
        long raw = raw_open("", 0, OPEN_READ_FLAG);
        if (raw >= 0) {
            (void)raw_close(raw);
            row("P03r", 0, "raw empty path unexpected success");
        } else {
            row("P03r", errno_is((int)-raw, empty_errors, 2), "raw empty path");
        }
    }
    expect_error("P04", "noscheme", empty_errors, 2);
    expect_error("P05", "fi:le:/etc/mounts", open_errors, 7);
    expect_error("P06", "file:/etc:mounts", open_errors, 7);
    {
        char bytes[] = {'f', 'i', 'l', 'e', ':', '/', (char)0xff};
        raw_expect_error("P07", bytes, sizeof(bytes), nonutf_errors, 2);
    }
    {
        char bytes[] = "file:/etc/mounts\0junk";
        long raw = raw_open(bytes, sizeof(bytes) - 1, OPEN_READ_FLAG);
        if (raw >= 0) {
            (void)raw_close(raw);
            row("P08", 0, "embedded NUL resolved (path truncation)");
        } else {
            row("P08", errno_is((int)-raw, open_errors, 7), "embedded NUL rejected");
        }
        if (!legal_open()) {
            row("next-legal", 0, "open(/etc/mounts) after the embedded NUL");
        }
    }
    {
        /* No NUL terminator inside the buffer: the length is the whole path. */
        char bytes[] = "file:/etc/mounts";
        long raw = raw_open(bytes, sizeof(bytes) - 1, OPEN_READ_FLAG);
        if (raw >= 0) {
            (void)raw_close(raw);
            row("P09", 1, "raw length-only path without NUL accepted");
        } else {
            row("P09", 0, "raw length-only path without NUL rejected");
        }
    }

    /* Component counts and lengths. */
    component_path(path, sizeof(path), 1, 1);
    expect_error("P10", path, open_errors, 7);
    component_path(path, sizeof(path), 2, 1);
    expect_error("P11", path, open_errors, 7);
    component_path(path, sizeof(path), 16, 1);
    expect_error("P12", path, open_errors, 7);
    component_path(path, sizeof(path), 255, 1);
    expect_error("P13", path, long_errors, 2);
    component_path(path, sizeof(path), 1, 255);
    expect_error("P14", path, long_errors, 2);
    component_path(path, sizeof(path), 1, 4096);
    expect_error("P15", path, long_errors, 2);
    memset(path, 'a', sizeof(path) - 1);
    path[0] = '/';
    path[4095] = '\0';
    expect_error("P16", path, long_errors, 2);
    path[4095] = 'a';
    path[4096] = '\0';
    expect_error("P17", path, long_errors, 2);

    /* The kernel's scheme path limit is 256 bytes; a 257-byte full path is over it. */
    memset(path, 'a', 257);
    path[0] = '/';
    path[257] = '\0';
    raw_expect_error("P18", path, 257, long_errors, 2);

    /* `..` from the root: observed, not a deviation (root `/` is the whole namespace). */
    {
        const char *dots[] = {"file:/../../etc/mounts", "file:/./../etc/mounts", "file:/../..",
                              "initfs:../../..", "initfs:/../../init"};
        const char *ids[] = {"P19", "P20", "P21", "P22", "P23"};
        for (size_t index = 0; index < sizeof(dots) / sizeof(dots[0]); index++) {
            long result = raw_open(dots[index], strlen(dots[index]), OPEN_READ_FLAG);
            char detail[200];
            (void)snprintf(detail, sizeof(detail), "path=%s result=%s errno=%d", dots[index],
                           result >= 0 ? "handle" : "error",
                           result >= 0 ? 0 : (int)-result);
            row(ids[index], 1, detail);
            if (result >= 0) {
                (void)raw_close(result);
            }
        }
        if (!legal_open()) {
            row("next-legal", 0, "open(/etc/mounts) after the dot vectors");
        }
    }

    /* vfsd CALL paths through lstat. */
    lstat_probe("C01", "/etc/mounts", (const int[]){0}, 1);
    lstat_probe("C02", "/../../etc/mounts", (const int[]){0, ENOENT}, 2);
    memset(path, 'a', 300);
    path[0] = '/';
    path[300] = '\0';
    lstat_probe("C03", path, long_errors, 2);
    readlink_probe("C04", "/etc/mounts", (const int[]){EINVAL, EROFS}, 2);
    readlink_probe("C05", "/init", (const int[]){EINVAL, EROFS, ENOENT}, 3);

    /* Chroot rows last: after this the process root is /etc. */
    chroot_rows();

    sayf("vfs-fuzz: summary rows=%d deviations=%d escapes=%d hash=0x%016lx\n", rows, deviations,
         escapes, table_hash);
    if (escapes != 0) {
        say("vfs-fuzz: FAILED: chroot escape\n");
        return 1;
    }
    say("vfs-fuzz: alive\n");
    return 0;
}
