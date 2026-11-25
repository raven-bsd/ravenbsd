/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * S172 descriptor conformance probe (test-only).
 *
 * Runs the POSIX descriptor contract on the release image: one open file description shared by
 * dup/dup2/fcntl(F_DUPFD), O_APPEND positioning, open/creat/O_TRUNC/O_EXCL flags, per-descriptor
 * (FD_CLOEXEC) versus per-open-file (O_APPEND/O_NONBLOCK) flags, O_CLOEXEC/F_SETFD across exec,
 * EOF, short reads, EINTR with a handler and only after a partial transfer, EAGAIN with
 * O_NONBLOCK, pipe EPIPE/SIGPIPE, and fstat identity for duplicated descriptors.
 *
 * The donor contract is FreeBSD sys/kern/kern_descrip.c (kern_dup, fdcloseexec, kern_fcntl),
 * sys/kern/sys_generic.c (dofileread/dofilewrite partial-transfer EINTR suppression),
 * sys/kern/vfs_vnops.c (IO_APPEND from f_flag O_APPEND) and sys/kern/sys_pipe.c (EPIPE and
 * SIGPIPE).  No donor expression is copied; the rows assert the observable contract.
 *
 * Two controlled mutations encode what a broken implementation would believe and therefore turn
 * their rows red against the correct product:
 *   dup-copies-offset   -- a dup is expected to own an independent file offset
 *   cloexec-survives    -- a FD_CLOEXEC descriptor is expected to survive exec
 */

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include "conformance.h"

#define TOPIC "descriptors"
#define SELF "/bin/descriptor-conformance"
#define PATH_MAX_LOCAL 128

static volatile sig_atomic_t sigpipe_seen;
static volatile sig_atomic_t sigalrm_seen;

static void note_sigpipe(int signal_number) {
    (void)signal_number;
    sigpipe_seen++;
}

static void note_sigalrm(int signal_number) {
    (void)signal_number;
    sigalrm_seen++;
}

static void make_path(char *buffer, unsigned size, const char *suffix) {
    (void)snprintf(buffer, size, "/tmp/descriptor-%ld-%s", (long)getpid(), suffix);
}

static int install_handler(int signal_number, void (*handler)(int)) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = handler;
    sigemptyset(&action.sa_mask);
    return sigaction(signal_number, &action, NULL);
}

static int write_all(int fd, const void *data, size_t length) {
    const char *cursor = data;
    size_t done = 0;
    while (done < length) {
        ssize_t count = write(fd, cursor + done, length - done);
        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)count;
    }
    return 0;
}

static int read_file(const char *path, char *buffer, size_t size) {
    int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return -1;
    }
    ssize_t count = read(fd, buffer, size);
    close(fd);
    return count < 0 ? -1 : (int)count;
}

static int mutation_dup_copies_offset;
static int mutation_cloexec_survives;

/* --- child side of the exec rows --------------------------------------------------------- */

static int fd_is_open(int fd) {
    return fcntl(fd, F_GETFD) >= 0;
}

static int child_cloexec(int argc, char **argv) {
    if (argc < 7) {
        printf("CONFORM %s child=cloexec bad-argc=%d\n", TOPIC, argc);
        return 90;
    }
    int keep = atoi(argv[2]);
    int cloexec = atoi(argv[3]);
    int setfd = atoi(argv[4]);
    int plain = atoi(argv[5]);
    int dupfd = atoi(argv[6]);
    int mutation = argc > 7 && strcmp(argv[7], "--mutation-cloexec-survives") == 0;

    int keep_open = fd_is_open(keep);
    int cloexec_open = fd_is_open(cloexec);
    int setfd_open = fd_is_open(setfd);
    int plain_open = fd_is_open(plain);
    int dupfd_open = fd_is_open(dupfd);

    char byte = 0;
    int keep_readable = keep_open && read(keep, &byte, 1) == 1 && byte == 'K';
    int plain_readable = plain_open && read(plain, &byte, 1) == 1 && byte == 'P';

    /* Correct product: the plain descriptors survive, the CLOEXEC ones do not.  The mutation
     * expects a CLOEXEC descriptor to survive, so the correct product fails it. */
    int expect_cloexec_open = mutation ? 1 : 0;
    int ok = keep_open && keep_readable && plain_open && plain_readable &&
             cloexec_open == expect_cloexec_open &&
             setfd_open == expect_cloexec_open &&
             dupfd_open == expect_cloexec_open;

    printf("CONFORM %s child=cloexec mutation=%d keep=%d/%d cloexec=%d setfd=%d plain=%d/%d "
           "dupfd=%d ok=%d\n",
           TOPIC, mutation, keep_open, keep_readable, cloexec_open, setfd_open,
           plain_open, plain_readable, dupfd_open, ok);
    fflush(stdout);
    return ok ? 0 : 12;
}

/* --- rows --------------------------------------------------------------------------------- */

static void row_dup_offset(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    char first[2] = {0};
    char second[2] = {0};
    char expect[3] = "bc";
    struct stat a;
    struct stat b;

    make_path(path, sizeof(path), "dup");
    (void)unlink(path);
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        conform_detail(detail, sizeof(detail), "open_errno=%d", errno);
        conform_check(state, "dup-offset", 0, detail);
        return;
    }
    int ok = write_all(fd, "abcdef", 6) == 0;
    off_t seek = lseek(fd, 1, SEEK_SET);
    int dupfd = ok && seek == 1 ? dup(fd) : -1;
    ssize_t first_count = dupfd >= 0 ? read(dupfd, first, 2) : -1;
    off_t after_dup = dupfd >= 0 ? lseek(fd, 0, SEEK_CUR) : -1;
    ssize_t second_count = fd >= 0 ? read(fd, second, 2) : -1;
    int same_object = dupfd >= 0 && fstat(fd, &a) == 0 && fstat(dupfd, &b) == 0 &&
                      a.st_dev == b.st_dev && a.st_ino == b.st_ino &&
                      a.st_size == b.st_size && S_ISREG(a.st_mode) &&
                      (fcntl(fd, F_GETFL, 0) & ~O_APPEND) ==
                          (fcntl(dupfd, F_GETFL, 0) & ~O_APPEND);

    /* A shared file offset means the original fd continues where the duplicate stopped. */
    off_t expect_offset = mutation_dup_copies_offset ? 1 : 3;
    ok = ok && first_count == 2 && memcmp(first, expect, 2) == 0 &&
         second_count == 2 && memcmp(second, "de", 2) == 0 && after_dup == expect_offset &&
         same_object;

    conform_detail(detail, sizeof(detail),
                   "fd=%d dup=%d r1=%zd/%c%c r2=%zd/%c%c offset=%lld expect_offset=%lld "
                   "ino_match=%d mutation=%d",
                   fd, dupfd, first_count, first[0] ? first[0] : '.',
                   first[1] ? first[1] : '.', second_count, second[0] ? second[0] : '.',
                   second[1] ? second[1] : '.', (long long)after_dup,
                   (long long)expect_offset, same_object, mutation_dup_copies_offset);
    conform_check(state, "dup-offset", ok, detail);
    if (dupfd >= 0) {
        close(dupfd);
    }
    close(fd);
    (void)unlink(path);
}

static void row_dup2_dupfd(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    char byte = 0;
    char text[4] = {0};

    make_path(path, sizeof(path), "dup2");
    (void)unlink(path);
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    int target = open("/dev/null", O_RDONLY);
    int ok = fd >= 0 && target >= 0 && write_all(fd, "abcdef", 6) == 0 &&
             lseek(fd, 2, SEEK_SET) == 2 && dup2(fd, target) == target;
    ssize_t from_target = ok ? read(target, text, 2) : -1;
    off_t after_target = ok ? lseek(fd, 0, SEEK_CUR) : -1;
    int same = ok && dup2(fd, fd) == fd;
    int min_fd = ok ? fcntl(fd, F_DUPFD, 100) : -1;
    ssize_t from_min = min_fd >= 100 ? read(min_fd, &byte, 1) : -1;
    off_t after_min = ok ? lseek(fd, 0, SEEK_CUR) : -1;
    int invalid_min = ok ? fcntl(fd, F_DUPFD, 4096) : -2;
    int invalid_errno = errno;
    int bad_old = ok ? dup2(-1, target) : -2;
    int bad_old_errno = errno;
    int target_alive = target >= 0 && fd_is_open(target);

    ok = ok && from_target == 2 && memcmp(text, "cd", 2) == 0 && after_target == 4 && same &&
         min_fd >= 100 && from_min == 1 && byte == 'e' && after_min == 5 &&
         invalid_min == -1 && invalid_errno == EINVAL && bad_old == -1 &&
         bad_old_errno == EBADF && target_alive;

    conform_detail(detail, sizeof(detail),
                   "dup2=%zd/%c%c offset=%lld same=%d min=%d min_read=%zd/%c min_offset=%lld "
                   "invalid_min=%d errno=%d dup2_bad=%d errno=%d target_alive=%d",
                   from_target, text[0] ? text[0] : '.', text[1] ? text[1] : '.',
                   (long long)after_target, same, min_fd, from_min, byte ? byte : '.',
                   (long long)after_min, invalid_min, invalid_errno, bad_old, bad_old_errno,
                   target_alive);
    conform_check(state, "dup2-dupfd", ok, detail);
    if (min_fd >= 100) {
        close(min_fd);
    }
    if (target >= 0) {
        close(target);
    }
    if (fd >= 0) {
        close(fd);
    }
    (void)unlink(path);
}

static void row_append(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    char content[16] = {0};

    make_path(path, sizeof(path), "append");
    (void)unlink(path);
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    int ok = fd >= 0 && write_all(fd, "12345", 5) == 0 && lseek(fd, 0, SEEK_SET) == 0;
    int append = ok ? open(path, O_WRONLY | O_APPEND) : -1;
    ssize_t first = append >= 0 ? write(append, "XY", 2) : -1;
    off_t after_first = append >= 0 ? lseek(append, 0, SEEK_CUR) : -1;
    off_t rewound = append >= 0 ? lseek(append, 0, SEEK_SET) : -1;
    ssize_t second = append >= 0 && rewound == 0 ? write(append, "Z", 1) : -1;
    int length = read_file(path, content, sizeof(content) - 1);

    ok = ok && first == 2 && after_first == 7 && rewound == 0 && second == 1 &&
         length == 8 && memcmp(content, "12345XYZ", 8) == 0;

    conform_detail(detail, sizeof(detail),
                   "write1=%zd offset1=%lld seek=%lld write2=%zd length=%d content=%s",
                   first, (long long)after_first, (long long)rewound, second, length,
                   length >= 0 ? content : "?");
    conform_check(state, "append", ok, detail);
    if (append >= 0) {
        close(append);
    }
    if (fd >= 0) {
        close(fd);
    }
    (void)unlink(path);
}

static void row_open_flags(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    struct stat info;

    make_path(path, sizeof(path), "flags");
    (void)unlink(path);
    int created = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    int ok = created >= 0 && write_all(created, "abcdef", 6) == 0;
    if (created >= 0) {
        close(created);
    }
    int existing = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    int existing_errno = errno;
    int truncated = creat(path, 0600);
    int after_creat = truncated >= 0 && fstat(truncated, &info) == 0 && info.st_size == 0;
    if (truncated >= 0) {
        close(truncated);
    }
    int reopened = open(path, O_WRONLY);
    ok = ok && write_all(reopened, "xy", 2) == 0;
    if (reopened >= 0) {
        close(reopened);
    }
    int re_truncated = open(path, O_RDWR | O_TRUNC);
    int after_trunc = re_truncated >= 0 && fstat(re_truncated, &info) == 0 && info.st_size == 0;
    if (re_truncated >= 0) {
        close(re_truncated);
    }

    ok = ok && existing == -1 && existing_errno == EEXIST && after_creat && after_trunc;

    conform_detail(detail, sizeof(detail),
                   "created=%d existing=%d errno=%d creat_size0=%d trunc_size0=%d",
                   created, existing, existing_errno, after_creat, after_trunc);
    conform_check(state, "open-flags", ok, detail);
    (void)unlink(path);
}

static void row_fd_flags(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];

    make_path(path, sizeof(path), "fdflags");
    (void)unlink(path);
    int first = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    int second = first >= 0 ? open(path, O_RDWR) : -1;
    int first_flags = first >= 0 ? fcntl(first, F_GETFL, 0) : -1;
    int second_flags = second >= 0 ? fcntl(second, F_GETFL, 0) : -1;
    int set = first >= 0 ? fcntl(first, F_SETFL, first_flags | O_APPEND) : -1;
    int first_after = first >= 0 ? fcntl(first, F_GETFL, 0) : -1;
    int second_after = second >= 0 ? fcntl(second, F_GETFL, 0) : -1;
    int duplicate = first >= 0 ? dup(first) : -1;
    int duplicate_flags = duplicate >= 0 ? fcntl(duplicate, F_GETFL, 0) : -1;
    int descriptor_set = first >= 0 ? fcntl(first, F_SETFD, FD_CLOEXEC) : -1;
    int first_fd_flags = first >= 0 ? fcntl(first, F_GETFD, 0) : -1;
    int duplicate_fd_flags = duplicate >= 0 ? fcntl(duplicate, F_GETFD, 0) : -1;

    int ok = first >= 0 && second >= 0 && set == 0 &&
             (first_after & O_APPEND) != 0 && (second_after & O_APPEND) == 0 &&
             (second_flags & O_APPEND) == 0 && duplicate >= 0 &&
             (duplicate_flags & O_APPEND) != 0 && descriptor_set == 0 &&
             (first_fd_flags & FD_CLOEXEC) != 0 && (duplicate_fd_flags & FD_CLOEXEC) == 0;

    conform_detail(detail, sizeof(detail),
                   "open1=0x%x open2=0x%x after1=0x%x after2=0x%x dup=0x%x fd1=0x%x dup_fd=0x%x",
                   first_flags, second_flags, first_after, second_after, duplicate_flags,
                   first_fd_flags, duplicate_fd_flags);
    conform_check(state, "fd-flags", ok, detail);
    if (duplicate >= 0) {
        close(duplicate);
    }
    if (second >= 0) {
        close(second);
    }
    if (first >= 0) {
        close(first);
    }
    (void)unlink(path);
}

static void row_cloexec(struct conform_state *state) {
    const char *names[5] = {"keep", "cloexec", "setfd", "plain", "dupfd"};
    char paths[5][PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    char byte = 0;
    int fds[5] = {-1, -1, -1, -1, -1};

    for (int index = 0; index < 4; index++) {
        make_path(paths[index], sizeof(paths[index]), names[index]);
        (void)unlink(paths[index]);
        fds[index] = open(paths[index], O_RDWR | O_CREAT | O_TRUNC, 0600);
        if (fds[index] < 0) {
            conform_detail(detail, sizeof(detail), "open_%s_errno=%d", names[index], errno);
            conform_check(state, "cloexec", 0, detail);
            return;
        }
    }
    int ok = write_all(fds[0], "K", 1) == 0 && write_all(fds[3], "P", 1) == 0 &&
             lseek(fds[0], 0, SEEK_SET) == 0 && lseek(fds[3], 0, SEEK_SET) == 0 &&
             fcntl(fds[1], F_SETFD, FD_CLOEXEC) == 0 &&
             fcntl(fds[2], F_SETFD, FD_CLOEXEC) == 0;
    fds[4] = fcntl(fds[3], F_DUPFD_CLOEXEC, 0);
    int flags_keep = fcntl(fds[0], F_GETFD, 0);
    int flags_cloexec = fcntl(fds[1], F_GETFD, 0);
    int flags_setfd = fcntl(fds[2], F_GETFD, 0);
    int flags_plain = fcntl(fds[3], F_GETFD, 0);
    int flags_dupfd = fds[4] >= 0 ? fcntl(fds[4], F_GETFD, 0) : -1;

    char keep_text[16];
    char cloexec_text[16];
    char setfd_text[16];
    char plain_text[16];
    char dupfd_text[16];
    char mutation_text[64];
    (void)snprintf(keep_text, sizeof(keep_text), "%d", fds[0]);
    (void)snprintf(cloexec_text, sizeof(cloexec_text), "%d", fds[1]);
    (void)snprintf(setfd_text, sizeof(setfd_text), "%d", fds[2]);
    (void)snprintf(plain_text, sizeof(plain_text), "%d", fds[3]);
    (void)snprintf(dupfd_text, sizeof(dupfd_text), "%d", fds[4]);
    mutation_text[0] = '\0';
    if (mutation_cloexec_survives) {
        (void)snprintf(mutation_text, sizeof(mutation_text), "%s",
                       "--mutation-cloexec-survives");
    }

    int status = -1;
    pid_t child = ok ? fork() : -1;
    if (child == 0) {
        char *arguments[10];
        int index = 0;
        arguments[index++] = (char *)"descriptor-conformance";
        arguments[index++] = (char *)"--exec-child";
        arguments[index++] = keep_text;
        arguments[index++] = cloexec_text;
        arguments[index++] = setfd_text;
        arguments[index++] = plain_text;
        arguments[index++] = dupfd_text;
        if (mutation_text[0] != '\0') {
            arguments[index++] = mutation_text;
        }
        arguments[index] = NULL;
        char *environment[] = {(char *)"PATH=/bin", (char *)"HOME=/tmp", NULL};
        execve(SELF, arguments, environment);
        _exit(127);
    }
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    int child_ok = child > 0 && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    (void)byte;

    ok = ok && child_ok &&
         (flags_keep & FD_CLOEXEC) == 0 &&
         (flags_cloexec & FD_CLOEXEC) != 0 &&
         (flags_setfd & FD_CLOEXEC) != 0 &&
         (flags_plain & FD_CLOEXEC) == 0 &&
         fds[4] >= 0 && (flags_dupfd & FD_CLOEXEC) != 0;

    conform_detail(detail, sizeof(detail),
                   "child=%d status=0x%x keep=0x%x cloexec=0x%x setfd=0x%x plain=0x%x "
                   "dupfd=%d/0x%x mutation=%d",
                   (int)child, status, flags_keep, flags_cloexec, flags_setfd, flags_plain,
                   fds[4], flags_dupfd, mutation_cloexec_survives);
    conform_check(state, "cloexec", ok, detail);

    for (int index = 0; index < 5; index++) {
        if (fds[index] >= 0) {
            close(fds[index]);
        }
        if (index < 4) {
            (void)unlink(paths[index]);
        }
    }
}

static void row_eof(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    char buffer[8] = {0};

    make_path(path, sizeof(path), "eof");
    (void)unlink(path);
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    int ok = fd >= 0 && write_all(fd, "abc", 3) == 0 && lseek(fd, 0, SEEK_SET) == 0;
    ssize_t first = ok ? read(fd, buffer, sizeof(buffer)) : -1;
    ssize_t second = ok ? read(fd, buffer, sizeof(buffer)) : -1;
    ssize_t third = ok ? read(fd, buffer, sizeof(buffer)) : -1;

    ok = ok && first == 3 && second == 0 && third == 0;

    conform_detail(detail, sizeof(detail), "read1=%zd read2=%zd read3=%zd", first, second,
                   third);
    conform_check(state, "eof", ok, detail);
    if (fd >= 0) {
        close(fd);
    }
    (void)unlink(path);
}

static void row_short_read(struct conform_state *state) {
    char detail[CONFORM_DETAIL_MAX];
    char buffer[32] = {0};
    int pipefd[2] = {-1, -1};

    int ok = pipe(pipefd) == 0 && write_all(pipefd[1], "12345678", 8) == 0;
    ssize_t first = ok ? read(pipefd[0], buffer, sizeof(buffer)) : -1;
    int closed = pipefd[1] >= 0 && close(pipefd[1]) == 0;
    ssize_t second = ok ? read(pipefd[0], buffer, sizeof(buffer)) : -1;

    ok = ok && closed && first == 8 && second == 0;

    conform_detail(detail, sizeof(detail), "read1=%zd read2=%zd", first, second);
    conform_check(state, "short-read", ok, detail);
    if (pipefd[0] >= 0) {
        close(pipefd[0]);
    }
    if (pipefd[1] >= 0) {
        close(pipefd[1]);
    }
}

static void row_eintr(struct conform_state *state) {
    char detail[CONFORM_DETAIL_MAX];
    char buffer[16] = {0};
    int empty[2] = {-1, -1};
    int partial[2] = {-1, -1};
    int handler = install_handler(SIGALRM, note_sigalrm) == 0;
    int ok = handler && pipe(empty) == 0 && pipe(partial) == 0;

    sigalrm_seen = 0;
    (void)alarm(1);
    errno = 0;
    ssize_t blocked = ok ? read(empty[0], buffer, sizeof(buffer)) : -1;
    int blocked_errno = errno;
    (void)alarm(0);

    ok = ok && write_all(partial[1], "12", 2) == 0;
    sigalrm_seen = 0;
    (void)alarm(1);
    errno = 0;
    ssize_t short_count = ok ? read(partial[0], buffer, sizeof(buffer)) : -1;
    int short_errno = errno;
    (void)alarm(0);

    ok = ok && blocked == -1 && blocked_errno == EINTR && short_count == 2 &&
         (short_errno == 0 || short_errno == EINTR) && sigalrm_seen >= 1;

    conform_detail(detail, sizeof(detail),
                   "empty=%zd errno=%d partial=%zd errno=%d signals=%d",
                   blocked, blocked_errno, short_count, short_errno, (int)sigalrm_seen);
    conform_check(state, "eintr", ok, detail);
    if (empty[0] >= 0) {
        close(empty[0]);
    }
    if (empty[1] >= 0) {
        close(empty[1]);
    }
    if (partial[0] >= 0) {
        close(partial[0]);
    }
    if (partial[1] >= 0) {
        close(partial[1]);
    }
}

static void row_nonblock(struct conform_state *state) {
    char detail[CONFORM_DETAIL_MAX];
    char buffer[8] = {0};
    int pipefd[2] = {-1, -1};

    int ok = pipe2(pipefd, O_NONBLOCK) == 0;
    int flags = ok ? fcntl(pipefd[0], F_GETFL, 0) : -1;
    errno = 0;
    ssize_t count = ok ? read(pipefd[0], buffer, sizeof(buffer)) : -1;
    int error = errno;
    int changed = ok ? fcntl(pipefd[0], F_SETFL, flags & ~O_NONBLOCK) : -1;
    int changed_flags = ok ? fcntl(pipefd[0], F_GETFL, 0) : -1;

    ok = ok && (flags & O_NONBLOCK) != 0 && count == -1 &&
         (error == EAGAIN || error == EWOULDBLOCK) && changed == 0 &&
         (changed_flags & O_NONBLOCK) == 0;

    conform_detail(detail, sizeof(detail), "flags=0x%x read=%zd errno=%d after=0x%x", flags,
                   count, error, changed_flags);
    conform_check(state, "nonblock", ok, detail);
    if (pipefd[0] >= 0) {
        close(pipefd[0]);
    }
    if (pipefd[1] >= 0) {
        close(pipefd[1]);
    }
}

static void row_epipe_sigpipe(struct conform_state *state) {
    char detail[CONFORM_DETAIL_MAX];
    int pipefd[2] = {-1, -1};
    int ignored[2] = {-1, -1};
    char byte = 'X';

    int handler = install_handler(SIGPIPE, note_sigpipe) == 0;
    int ok = handler && pipe(pipefd) == 0 && close(pipefd[0]) == 0;
    pipefd[0] = -1;
    sigpipe_seen = 0;
    errno = 0;
    ssize_t first = ok ? write(pipefd[1], &byte, 1) : -1;
    int first_errno = errno;
    int first_signal = (int)sigpipe_seen;

    int ignore = signal(SIGPIPE, SIG_IGN) != SIG_ERR;
    int ok_ignored = ignore && pipe(ignored) == 0 && close(ignored[0]) == 0;
    ignored[0] = -1;
    errno = 0;
    ssize_t second = ok_ignored ? write(ignored[1], &byte, 1) : -1;
    int second_errno = errno;

    ok = ok && first == -1 && first_errno == EPIPE && first_signal == 1 && ok_ignored &&
         second == -1 && second_errno == EPIPE;

    conform_detail(detail, sizeof(detail),
                   "write1=%zd errno=%d sigpipe=%d write2=%zd errno=%d",
                   first, first_errno, first_signal, second, second_errno);
    conform_check(state, "epipe-sigpipe", ok, detail);
    if (pipefd[1] >= 0) {
        close(pipefd[1]);
    }
    if (ignored[1] >= 0) {
        close(ignored[1]);
    }
}

static void row_fstat_identity(struct conform_state *state) {
    char path[PATH_MAX_LOCAL];
    char detail[CONFORM_DETAIL_MAX];
    struct stat first;
    struct stat second;
    struct stat grown;

    make_path(path, sizeof(path), "fstat");
    (void)unlink(path);
    int fd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    int ok = fd >= 0 && write_all(fd, "0123456789", 10) == 0;
    int dupfd = ok ? dup(fd) : -1;
    int before = dupfd >= 0 && fstat(fd, &first) == 0 && fstat(dupfd, &second) == 0;
    int grown_ok = dupfd >= 0 && write_all(dupfd, "!", 1) == 0 && fstat(fd, &grown) == 0;

    ok = ok && before && grown_ok && first.st_dev == second.st_dev &&
         first.st_ino == second.st_ino && first.st_size == 10 && S_ISREG(first.st_mode) &&
         grown.st_size == 11 && grown.st_ino == first.st_ino;

    conform_detail(detail, sizeof(detail),
                   "dev=%llu/%llu ino=%llu/%llu size=%lld grown=%lld regular=%d",
                   (unsigned long long)first.st_dev, (unsigned long long)second.st_dev,
                   (unsigned long long)first.st_ino, (unsigned long long)second.st_ino,
                   (long long)first.st_size, (long long)grown.st_size,
                   S_ISREG(first.st_mode));
    conform_check(state, "fstat-identity", ok, detail);
    if (dupfd >= 0) {
        close(dupfd);
    }
    if (fd >= 0) {
        close(fd);
    }
    (void)unlink(path);
}

/* --- sections ----------------------------------------------------------------------------- */

static int run_normal(void) {
    struct conform_state state;
    conform_begin(&state, TOPIC);
    mutation_dup_copies_offset = 0;
    mutation_cloexec_survives = 0;
    row_dup_offset(&state);
    row_dup2_dupfd(&state);
    row_append(&state);
    row_open_flags(&state);
    row_fd_flags(&state);
    row_cloexec(&state);
    row_eof(&state);
    row_short_read(&state);
    row_eintr(&state);
    row_nonblock(&state);
    row_epipe_sigpipe(&state);
    row_fstat_identity(&state);
    return conform_finish(&state);
}

static int run_mutation(const char *name) {
    struct conform_state state;
    memset(&state, 0, sizeof(state));
    state.topic = TOPIC;
    conform_mutation_begin(TOPIC, name);
    if (strcmp(name, "dup-copies-offset") == 0) {
        mutation_dup_copies_offset = 1;
        mutation_cloexec_survives = 0;
        row_dup_offset(&state);
    } else if (strcmp(name, "cloexec-survives") == 0) {
        mutation_dup_copies_offset = 0;
        mutation_cloexec_survives = 1;
        row_cloexec(&state);
    } else {
        conform_note(&state, "unknown_mutation=%s", name);
        conform_check(&state, name, 0, "unknown mutation");
    }
    mutation_dup_copies_offset = 0;
    mutation_cloexec_survives = 0;
    return conform_mutation_end(&state, name);
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--exec-child") == 0) {
        return child_cloexec(argc, argv);
    }
    int failures = run_normal();
    if (argc > 1 && strcmp(argv[1], "--all") == 0) {
        failures |= run_mutation("dup-copies-offset");
        failures |= run_mutation("cloexec-survives");
    }
    return failures == 0 ? 0 : 1;
}
