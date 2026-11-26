// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S180 test-only IPC conformance probe. It exercises the POSIX/FreeBSD contract: SCM_RIGHTS with
// the received descriptor usable after the sender closes and with the rights mask honored,
// SO_PEERCRED exact pid/uid/gid, MSG_CMSG_CLOEXEC closing on exec, FIFO blocking open and
// non-blocking behavior, and socketpair message boundaries for SOCK_SEQPACKET vs SOCK_STREAM.
//
// The N05 rows from the ticket's 2026-11-24 re-evaluation drive the raw duplex ABI (0x110) as the
// unprivileged caller the probe runs as: an unprivileged caller must not run the global cycle
// collector, repeated triggers must stay bounded, and a concurrent collection must never discard a
// reachable queued handle. It changes no product code.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#ifndef MSG_CMSG_CLOEXEC
#define MSG_CMSG_CLOEXEC 0x40000000
#endif

// Raw duplex ABI used by the N05 collector rows. The numbers are kernel-owned
// (kernel/src/syscall/duplex.rs) and stay out of raven-abi until the reviewer promotes them.
#define RAVEN_HANDLE_CLOSE 0x50
#define RAVEN_DUPLEX_PAIR 0x110
#define RAVEN_DUPLEX_SENDMSG 0x112
#define RAVEN_DUPLEX_RECVMSG 0x113
#define RAVEN_DUPLEX_DIAGNOSTIC 0x80000000u
#define RAVEN_DUPLEX_COLLECT 0x40000000u
#define RAVEN_DUPLEX_NONBLOCK 04000
#define RAVEN_SOL_SOCKET 1u
#define RAVEN_SCM_RIGHTS 1u
#define RAVEN_CONTROL_BYTES 20u

struct ucred_local {
    pid_t pid;
    uid_t uid;
    gid_t gid;
};

struct duplex_stats {
    uint64_t pairs;
    uint64_t ends;
    uint64_t pending;
    uint64_t controls;
};

struct duplex_message {
    uint64_t data;
    uint64_t data_len;
    uint64_t control;
    uint64_t control_len;
    uint32_t flags;
    uint32_t handle_count;
};

struct duplex_control {
    uint32_t len;
    uint32_t level;
    uint32_t type;
    uint32_t count;
    uint32_t handles[1];
};

_Static_assert(sizeof(struct duplex_stats) == 32, "DuplexStats must stay 32 bytes");
_Static_assert(sizeof(struct duplex_message) == 40, "DuplexMessage must stay 40 bytes");
_Static_assert(sizeof(struct duplex_control) == RAVEN_CONTROL_BYTES,
               "one SCM_RIGHTS handle header must stay 20 bytes");

static int failures;

static void row(const char *name, int ok, const char *detail) {
    printf("S180-IPC row=%s status=%s detail=%s\n", name, ok ? "PASS" : "FAIL", detail);
    fflush(stdout);
    if (!ok) {
        failures++;
    }
}

// A failed setup step records the exact errno so the release-image finding names the defect.
static void fail_row(const char *name, const char *what, int error) {
    char detail[192];
    snprintf(detail, sizeof detail, "%s errno=%d", what, error);
    row(name, 0, detail);
}

// Raw two-argument call: number in rax, arguments in rdi/rsi, result in rax; a negative result in
// -errno range is the kernel's error return (raven-abi Errno::from_ret).
static long raven_call(long number, long arg0, long arg1) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(arg0), "S"(arg1)
                     : "rcx", "r11", "memory");
    return result;
}

static int raven_errno(long result) {
    return result < 0 && result >= -4095 ? (int)-result : 0;
}

static int raw_pair(uint32_t flags, uint32_t handles[2]) {
    long result = raven_call(RAVEN_DUPLEX_PAIR, (long)handles, (long)flags);
    if (result < 0) {
        errno = raven_errno(result);
        return -1;
    }
    return 0;
}

// Run one diagnostic query; `collect` also runs one collector pass.
static int raw_collect(int collect, struct duplex_stats *stats) {
    uint32_t flags = RAVEN_DUPLEX_DIAGNOSTIC | (collect ? RAVEN_DUPLEX_COLLECT : 0u);
    memset(stats, 0, sizeof *stats);
    long result = raven_call(RAVEN_DUPLEX_PAIR, (long)stats, (long)flags);
    if (result < 0) {
        errno = raven_errno(result);
        return -1;
    }
    return 0;
}

static int raw_close(uint32_t handle) {
    long result = raven_call(RAVEN_HANDLE_CLOSE, (long)handle, 0);
    if (result < 0) {
        errno = raven_errno(result);
        return -1;
    }
    return 0;
}

static int raw_send_handle(uint32_t end, uint32_t handle, char byte) {
    struct duplex_control control;
    struct duplex_message message;
    memset(&control, 0, sizeof control);
    memset(&message, 0, sizeof message);
    control.len = RAVEN_CONTROL_BYTES;
    control.level = RAVEN_SOL_SOCKET;
    control.type = RAVEN_SCM_RIGHTS;
    control.count = 1;
    control.handles[0] = handle;
    message.data = (uint64_t)(uintptr_t)&byte;
    message.data_len = 1;
    message.control = (uint64_t)(uintptr_t)&control;
    message.control_len = RAVEN_CONTROL_BYTES;
    long result = raven_call(RAVEN_DUPLEX_SENDMSG, (long)end, (long)&message);
    if (result < 0) {
        errno = raven_errno(result);
        return -1;
    }
    return (int)result;
}

// Receive one byte and its single SCM_RIGHTS handle. Returns bytes or -1; got_handle is 1 only
// when the kernel installed exactly one handle.
static int raw_recv_handle(uint32_t end, char *byte, uint32_t *handle, int *got_handle) {
    struct duplex_control control;
    struct duplex_message message;
    memset(&control, 0, sizeof control);
    memset(&message, 0, sizeof message);
    *got_handle = 0;
    message.data = (uint64_t)(uintptr_t)byte;
    message.data_len = 1;
    message.control = (uint64_t)(uintptr_t)&control;
    message.control_len = RAVEN_CONTROL_BYTES;
    long result = raven_call(RAVEN_DUPLEX_RECVMSG, (long)end, (long)&message);
    if (result < 0) {
        errno = raven_errno(result);
        return -1;
    }
    if (message.handle_count == 1 && control.len == RAVEN_CONTROL_BYTES &&
        control.level == RAVEN_SOL_SOCKET && control.type == RAVEN_SCM_RIGHTS &&
        control.count == 1) {
        *handle = control.handles[0];
        *got_handle = 1;
    }
    return (int)result;
}

static int send_fd(int sock, int fd) {
    struct msghdr message;
    struct iovec iov;
    char byte = 'X';
    char control[CMSG_SPACE(sizeof(int))];
    memset(&message, 0, sizeof message);
    memset(control, 0, sizeof control);
    iov.iov_base = &byte;
    iov.iov_len = 1;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof control;
    struct cmsghdr *cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    cmsg->cmsg_len = CMSG_LEN(sizeof(int));
    memcpy(CMSG_DATA(cmsg), &fd, sizeof fd);
    return (int)sendmsg(sock, &message, 0);
}

static int recv_fd(int sock, int flags, int *fd_out) {
    struct msghdr message;
    struct iovec iov;
    char byte = 0;
    char control[CMSG_SPACE(sizeof(int))];
    memset(&message, 0, sizeof message);
    memset(control, 0, sizeof control);
    iov.iov_base = &byte;
    iov.iov_len = 1;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof control;
    if (recvmsg(sock, &message, flags) < 0) {
        return -1;
    }
    struct cmsghdr *cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL || cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS ||
        cmsg->cmsg_len < CMSG_LEN(sizeof(int))) {
        return -1;
    }
    memcpy(fd_out, CMSG_DATA(cmsg), sizeof *fd_out);
    return 0;
}

static void test_rights(int mutation) {
    int sv[2];
    int wfd;
    int sfd;
    int rfd = -1;
    char path[64];
    char buffer[8];
    char detail[256];
    snprintf(path, sizeof path, "/tmp/s180-rights-%d", (int)getpid());
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        fail_row("rights", "socketpair failed", errno);
        return;
    }
    wfd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (wfd < 0) {
        fail_row("rights", "open seed failed", errno);
        close(sv[0]);
        close(sv[1]);
        return;
    }
    (void)write(wfd, "K180", 4);
    // The normal sender holds a read-only descriptor: the received descriptor must be read-only
    // too. The mutation sends a genuinely writable descriptor while the row keeps the same
    // read-only assertion, so a run that dropped (ignored) the rights mask turns the row red.
    // The mutation reuses the writable seed fd: an O_RDWR reopen of the same /tmp file is a
    // separate VFS gap, not a rights-mask result.
    if (mutation) {
        sfd = wfd;
        if (lseek(sfd, 0, SEEK_SET) < 0) {
            fail_row("rights", "mutation lseek failed", errno);
            close(wfd);
            close(sv[0]);
            close(sv[1]);
            unlink(path);
            return;
        }
    } else {
        close(wfd);
        sfd = open(path, O_RDONLY);
    }
    if (sfd < 0) {
        fail_row("rights", "open sender failed", errno);
        close(sv[0]);
        close(sv[1]);
        unlink(path);
        return;
    }
    errno = 0;
    int sent = send_fd(sv[0], sfd);
    int send_errno = errno;
    close(sfd);
    errno = 0;
    int received = sent == 1 ? recv_fd(sv[1], 0, &rfd) : -1;
    int recv_errno = errno;
    memset(buffer, 0, sizeof buffer);
    errno = 0;
    ssize_t got = rfd >= 0 ? read(rfd, buffer, 4) : -1;
    int read_errno = errno;
    errno = 0;
    ssize_t wrote = rfd >= 0 ? write(rfd, "Y", 1) : -1;
    int write_errno = errno;
    int ok = sent == 1 && received == 0 && got == 4 && memcmp(buffer, "K180", 4) == 0;
    // The sender was closed before the read above: the received descriptor is independent.
    if (ok) {
        // The normal sender is read-only; a writable descriptor reaching the receiver is the
        // mutation shape. Both runs assert the receiver's rights are read-only, so the mutation
        // row goes red while the normal row proves the mask is carried and enforced.
        ok = wrote == -1;
    }
    snprintf(detail, sizeof detail,
             "sent=%d send_errno=%d recv=%d recv_errno=%d read=%zd read_errno=%d write=%zd "
             "write_errno=%d mutation=%d",
             sent, send_errno, received, recv_errno, got, read_errno, wrote, write_errno,
             mutation);
    row("rights", ok, detail);
    if (rfd >= 0) {
        close(rfd);
    }
    close(sv[0]);
    close(sv[1]);
    unlink(path);
}

static void test_credentials(void) {
    int sv[2];
    struct ucred_local cred;
    socklen_t length = sizeof cred;
    char detail[128];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        fail_row("credentials", "socketpair failed", errno);
        return;
    }
    memset(&cred, 0, sizeof cred);
    errno = 0;
    int result = getsockopt(sv[1], SOL_SOCKET, SO_PEERCRED, &cred, &length);
    int result_errno = errno;
    int ok = result == 0 && cred.pid == getpid() && cred.uid == getuid() && cred.gid == getgid();
    snprintf(detail, sizeof detail, "rc=%d errno=%d pid=%d/%d uid=%u/%u gid=%u/%u", result,
             result_errno, (int)cred.pid, (int)getpid(), cred.uid, getuid(), cred.gid, getgid());
    row("credentials", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

static void test_cloexec(const char *program) {
    int sv[2];
    int sfd;
    int rfd = -1;
    char path[64];
    char fd_text[16];
    char detail[160];
    snprintf(path, sizeof path, "/tmp/s180-cloexec-%d", (int)getpid());
    sfd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (sfd < 0) {
        fail_row("cloexec", "open seed failed", errno);
        return;
    }
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        fail_row("cloexec", "socketpair failed", errno);
        close(sfd);
        return;
    }
    close(sfd);
    sfd = open(path, O_RDONLY);
    if (sfd < 0) {
        fail_row("cloexec", "open sender failed", errno);
        close(sv[0]);
        close(sv[1]);
        return;
    }
    errno = 0;
    int sent = send_fd(sv[0], sfd);
    int send_errno = errno;
    close(sfd);
    errno = 0;
    int received = sent == 1 ? recv_fd(sv[1], MSG_CMSG_CLOEXEC, &rfd) : -1;
    int recv_errno = errno;
    int fd_flags = rfd >= 0 ? fcntl(rfd, F_GETFD) : -1;
    int cloexec = fd_flags >= 0 && (fd_flags & FD_CLOEXEC) != 0;
    snprintf(fd_text, sizeof fd_text, "%d", rfd);
    pid_t child = fork();
    int status = -1;
    if (child == 0) {
        execl(program, program, "--check-fd", fd_text, (char *)NULL);
        _exit(127);
    }
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    int ok = received == 0 && cloexec && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    snprintf(detail, sizeof detail,
             "sent=%d send_errno=%d recv=%d recv_errno=%d fd=%d flags=0x%x cloexec=%d child=%d",
             sent, send_errno, received, recv_errno, rfd, fd_flags, cloexec, status);
    row("cloexec", ok, detail);
    if (rfd >= 0) {
        close(rfd);
    }
    close(sv[0]);
    close(sv[1]);
    unlink(path);
}

static void test_fifo(void) {
    char path[64];
    char detail[192];
    char byte = 0;
    snprintf(path, sizeof path, "/tmp/s180-fifo-%d", (int)getpid());
    unlink(path);
    if (mkfifo(path, 0600) < 0) {
        fail_row("fifo", "mkfifo failed", errno);
        return;
    }
    int rfd = open(path, O_RDONLY | O_NONBLOCK);
    errno = 0;
    ssize_t read_result = rfd >= 0 ? read(rfd, &byte, 1) : -1;
    int read_errno = errno;
    int wfd = open(path, O_WRONLY | O_NONBLOCK);
    if (wfd >= 0) {
        close(wfd);
    }
    if (rfd >= 0) {
        close(rfd);
    }
    errno = 0;
    int no_reader = open(path, O_WRONLY | O_NONBLOCK);
    int open_errno = errno;
    if (no_reader >= 0) {
        close(no_reader);
    }

    pid_t child = fork();
    int status = -1;
    if (child == 0) {
        int cr = open(path, O_RDONLY);
        if (cr < 0) {
            _exit(2);
        }
        char received = 0;
        if (read(cr, &received, 1) != 1) {
            _exit(3);
        }
        _exit(received == 'F' ? 0 : 4);
    }
    int pw = open(path, O_WRONLY);
    if (pw >= 0) {
        (void)write(pw, "F", 1);
        close(pw);
    }
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    int nonblocking = rfd >= 0 && read_result == -1 &&
                      (read_errno == EAGAIN || read_errno == EWOULDBLOCK) && no_reader == -1 &&
                      (open_errno == ENXIO || open_errno == EAGAIN);
    int blocking = WIFEXITED(status) && WEXITSTATUS(status) == 0;
    snprintf(detail, sizeof detail,
             "reader=%d read=%zd read_errno=%d no_reader=%d open_errno=%d child=%d",
             rfd, read_result, read_errno, no_reader, open_errno, status);
    row("fifo", nonblocking && blocking, detail);
    unlink(path);
}

static void test_seqpacket(void) {
    int sv[2];
    char first[8];
    char second[8];
    char detail[128];
    memset(first, 0, sizeof first);
    memset(second, 0, sizeof second);
    if (socketpair(AF_UNIX, SOCK_SEQPACKET, 0, sv) < 0) {
        fail_row("seqpacket", "socketpair SOCK_SEQPACKET failed", errno);
        return;
    }
    (void)send(sv[0], "A", 1, 0);
    (void)send(sv[0], "B", 1, 0);
    ssize_t n1 = recv(sv[1], first, sizeof first, 0);
    ssize_t n2 = recv(sv[1], second, sizeof second, 0);
    int ok = n1 == 1 && first[0] == 'A' && n2 == 1 && second[0] == 'B';
    snprintf(detail, sizeof detail, "first=%zd second=%zd", n1, n2);
    row("seqpacket", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

static void test_stream(void) {
    int sv[2];
    char buffer[8];
    char detail[128];
    memset(buffer, 0, sizeof buffer);
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        fail_row("stream", "socketpair SOCK_STREAM failed", errno);
        return;
    }
    (void)send(sv[0], "A", 1, 0);
    (void)send(sv[0], "B", 1, 0);
    ssize_t n = recv(sv[1], buffer, sizeof buffer, 0);
    int ok = n >= 1 && (n == 2 ? (buffer[0] == 'A' && buffer[1] == 'B') : buffer[0] == 'A');
    snprintf(detail, sizeof detail, "recv=%zd first=%02x", n, (unsigned char)buffer[0]);
    row("stream", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

// N05 authorisation: the probe runs as the unprivileged ssh user (uid 1000). Collection is a
// kernel-owned operation in the donor model (FreeBSD schedules unp_gc on taskqueue_thread); a
// caller without authority must not be able to run the global pass. The row holds a live endpoint
// so an authorised pass cannot be mistaken for a safe no-op; any refusal satisfies the row and the
// detail records the exact errno the product returned.
static void test_collector_auth(void) {
    uint32_t pair[2];
    struct duplex_stats stats;
    char detail[192];
    if (raw_pair(0, pair) < 0) {
        fail_row("collector-auth", "raw duplex_pair failed", errno);
        return;
    }
    errno = 0;
    int result = raw_collect(1, &stats);
    int call_errno = errno;
    int denied = result < 0;
    snprintf(detail, sizeof detail, "uid=%u collect=%d errno=%d pairs=%llu", (unsigned)getuid(),
             result, call_errno, (unsigned long long)stats.pairs);
    row("collector-auth", denied, detail);
    raw_close(pair[0]);
    raw_close(pair[1]);
}

// N05 bounded cost: repeated user-issued triggers must not grow without bound. 32 live pairs stay
// reachable for the whole run, so every pass scans a non-empty registry; the row asserts the
// counters still cover those pairs and the 100 calls stay inside one second. A product that
// refuses the unprivileged trigger trivially has no user-issued cost and passes.
static void test_collector_bounded(void) {
    enum { PAIRS = 32, CALLS = 100 };
    uint32_t live[PAIRS][2];
    struct duplex_stats stats;
    struct timespec start;
    struct timespec end;
    char detail[224];
    int created = 0;
    while (created < PAIRS) {
        if (raw_pair(0, live[created]) < 0) {
            break;
        }
        created++;
    }
    if (created != PAIRS) {
        fail_row("collector-bounded", "raw duplex_pair failed", errno);
        for (int index = 0; index < created; index++) {
            raw_close(live[index][0]);
            raw_close(live[index][1]);
        }
        return;
    }
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) {
        fail_row("collector-bounded", "clock_gettime failed", errno);
        for (int index = 0; index < PAIRS; index++) {
            raw_close(live[index][0]);
            raw_close(live[index][1]);
        }
        return;
    }
    int calls = 0;
    int denied = 0;
    int call_errno = 0;
    while (calls < CALLS) {
        if (raw_collect(1, &stats) < 0) {
            denied = 1;
            call_errno = errno;
            break;
        }
        calls++;
    }
    if (clock_gettime(CLOCK_MONOTONIC, &end) != 0) {
        fail_row("collector-bounded", "clock_gettime failed", errno);
        for (int index = 0; index < PAIRS; index++) {
            raw_close(live[index][0]);
            raw_close(live[index][1]);
        }
        return;
    }
    long long elapsed_ms = (long long)(end.tv_sec - start.tv_sec) * 1000 +
                           (long long)(end.tv_nsec - start.tv_nsec) / 1000000;
    int retained = denied || stats.pairs >= (uint64_t)created;
    int ok = denied || (calls == CALLS && retained && elapsed_ms < 1000);
    snprintf(detail, sizeof detail,
             "uid=%u pairs=%d calls=%d elapsed_ms=%lld retained=%d denied=%d errno=%d",
             (unsigned)getuid(), created, calls, elapsed_ms, retained, denied, call_errno);
    row("collector-bounded", ok, detail);
    for (int index = 0; index < PAIRS; index++) {
        raw_close(live[index][0]);
        raw_close(live[index][1]);
    }
}

// N05 concurrent collection: one child churns reachable handle transfers (pair, send its own
// endpoint, receive it back, close) while the parent runs collector passes. After the child exits,
// a reachable queued handle must still be delivered: the collector's mark/sweep is not an atomic
// snapshot, so a pass that clears the live receiver's queue is the defect this row measures.
static void test_collector_concurrent(void) {
    enum { CHURN = 64 };
    char detail[256];
    int sync[2];
    if (pipe(sync) != 0) {
        fail_row("collector-concurrent", "pipe failed", errno);
        return;
    }
    pid_t child = fork();
    if (child == 0) {
        close(sync[0]);
        for (int index = 0; index < CHURN; index++) {
            uint32_t pair[2];
            uint32_t handle = 0;
            int got_handle = 0;
            char byte = 'C';
            char received = 0;
            if (raw_pair(RAVEN_DUPLEX_NONBLOCK, pair) < 0) {
                _exit(2);
            }
            if (raw_send_handle(pair[0], pair[0], byte) != 1) {
                _exit(3);
            }
            if (raw_recv_handle(pair[1], &received, &handle, &got_handle) != 1 ||
                received != 'C' || !got_handle) {
                _exit(4);
            }
            raw_close(handle);
            raw_close(pair[0]);
            raw_close(pair[1]);
            if (index == 0) {
                char ready = 'R';
                if (write(sync[1], &ready, 1) != 1) {
                    _exit(5);
                }
            }
        }
        close(sync[1]);
        _exit(0);
    }
    if (child < 0) {
        fail_row("collector-concurrent", "fork failed", errno);
        close(sync[0]);
        close(sync[1]);
        return;
    }
    close(sync[1]);
    char ready = 0;
    int started = read(sync[0], &ready, 1) == 1 && ready == 'R';
    close(sync[0]);
    int calls = 0;
    int denied = 0;
    int call_errno = 0;
    int reaped = 0;
    int status = 0;
    while (!reaped) {
        struct duplex_stats stats;
        if (!denied && raw_collect(1, &stats) < 0) {
            denied = 1;
            call_errno = errno;
        } else if (!denied) {
            calls++;
        }
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            reaped = 1;
        } else if (waited < 0 && errno != EINTR) {
            break;
        }
        if (denied) {
            if (waitpid(child, &status, 0) == child) {
                reaped = 1;
            }
            break;
        }
    }
    uint32_t pair[2];
    if (raw_pair(RAVEN_DUPLEX_NONBLOCK, pair) < 0) {
        fail_row("collector-concurrent", "raw duplex_pair failed", errno);
        return;
    }
    char byte = 'S';
    if (raw_send_handle(pair[0], pair[0], byte) != 1) {
        fail_row("collector-concurrent", "raw duplex_sendmsg failed", errno);
        raw_close(pair[0]);
        raw_close(pair[1]);
        return;
    }
    struct duplex_stats stats;
    int collect_denied = raw_collect(1, &stats) < 0;
    int collect_errno = errno;
    uint32_t handle = 0;
    int got_handle = 0;
    char received = 0;
    int received_bytes = raw_recv_handle(pair[1], &received, &handle, &got_handle);
    int retained = received_bytes == 1 && received == 'S' && got_handle;
    int child_ok = reaped && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    int ok = child_ok && retained && started && (denied || calls > 0);
    snprintf(detail, sizeof detail,
             "child=%d started=%d collect_calls=%d denied=%d errno=%d collect_denied=%d "
             "collect_errno=%d recv=%d retained=%d",
             status, started, calls, denied, call_errno, collect_denied, collect_errno,
             received_bytes, retained);
    row("collector-concurrent", ok, detail);
    if (got_handle) {
        raw_close(handle);
    }
    raw_close(pair[0]);
    raw_close(pair[1]);
}

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "--all";
    if (strcmp(mode, "--check-fd") == 0) {
        int fd = argc > 2 ? atoi(argv[2]) : -1;
        return fcntl(fd, F_GETFD) < 0 ? 0 : 1;
    }
    int mutation = strcmp(mode, "--mutation-rights") == 0;
    if (strcmp(mode, "--all") != 0 && !mutation) {
        fprintf(stderr, "usage: %s [--all|--mutation-rights|--check-fd FD]\n", argv[0]);
        return 64;
    }
    test_rights(mutation);
    test_credentials();
    test_cloexec(argv[0]);
    test_fifo();
    test_seqpacket();
    test_stream();
    test_collector_auth();
    test_collector_bounded();
    test_collector_concurrent();
    printf("S180-IPC done failures=%d\n", failures);
    fflush(stdout);
    return failures == 0 ? 0 : 1;
}
