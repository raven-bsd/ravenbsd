/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K548: poll(2) on the fd kinds the guest ssh client uses.
 *
 * The connection is an AF_UNIX socketpair end, stdout/stderr are a pipe or a pty slave.  A
 * write-only pty slave carries WRITE but not READ; the kernel EVQ source lookup must observe it
 * without returning EACCES.  Rev 330 adds the sshd server-side close shape (the session child
 * exits, the pty master reports HUP after EOF within a bounded ppoll) and the ppoll signal-mask
 * rows.  Rev 331 adds the same close shape for the master opened through the public `pty:` clone
 * path `/dev/ptmx`, so the test covers the SchemeFile master and not only `posix_openpt`.  The
 * qtest expects every line below and forbids a failure line. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int fail(const char *what, int fd, int err) {
    printf("posix-poll: fail %s fd=%d errno=%d\n", what, fd, err);
    fflush(stdout);
    return 1;
}

/* Return poll(2) revents, or -errno on failure. */
static int poll_revents(int fd, short events, int timeout_ms) {
    struct pollfd ready;
    int result;

    ready.fd = fd;
    ready.events = events;
    ready.revents = 0;
    result = poll(&ready, 1, timeout_ms);
    if (result < 0)
        return -errno;
    return ready.revents;
}

static int elapsed_ms(const struct timespec *before, const struct timespec *after) {
    long sec = after->tv_sec - before->tv_sec;
    long nsec = after->tv_nsec - before->tv_nsec;

    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    return (int)(sec * 1000L + nsec / 1000000L);
}

/* K548 rev 330: the sshd server-side pty close shape. The session child writes and exits while
 * the parent has already dropped its slave copy, so the child's exit closes the last slave. The
 * master must then report a hangup, and the wait must be bounded (the SSH ppoll timeout). */
static int pty_ssh_close(void) {
    struct pollfd ready;
    struct timespec timeout;
    struct timespec before;
    struct timespec after;
    char name[128];
    char byte = 0;
    int master;
    int slave;
    int status = 0;
    int ready_count;
    const char *slave_name;
    pid_t child;

    master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0)
        return fail("ssh-master", -1, errno);
    if (grantpt(master) != 0 || unlockpt(master) != 0)
        return fail("ssh-grant-unlock", master, errno);
    slave_name = ptsname(master);
    if (slave_name == NULL)
        return fail("ssh-ptsname", master, errno);
    (void)snprintf(name, sizeof(name), "%s", slave_name);
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0)
        return fail("ssh-slave", -1, errno);

    child = fork();
    if (child == 0) {
        (void)close(master);
        if (write(slave, "H", 1) != 1)
            _exit(2);
        (void)close(slave);
        _exit(0);
    }
    if (child < 0)
        return fail("ssh-fork", -1, errno);
    /* sshd closes its own slave copy right after the fork; the session child owns the last one. */
    (void)close(slave);

    if (waitpid(child, &status, 0) != child)
        return fail("ssh-waitpid", child, errno);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("ssh-child-status", status, 0);
    /* Drain the byte, then observe the master EOF. */
    if (read(master, &byte, 1) != 1 || byte != 'H')
        return fail("ssh-byte", (long)byte, errno);
    if (read(master, &byte, 1) != 0)
        return fail("ssh-eof", (long)byte, errno);

    /* After EOF the master is hung up: a bounded ppoll must return POLLHUP, not time out. */
    ready.fd = master;
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    (void)clock_gettime(CLOCK_MONOTONIC, &before);
    ready_count = ppoll(&ready, 1, &timeout, NULL);
    (void)clock_gettime(CLOCK_MONOTONIC, &after);
    if (ready_count <= 0)
        return fail("ssh-hup-poll", ready_count, errno);
    if ((ready.revents & POLLHUP) == 0)
        return fail("ssh-hup-revents", ready.revents, 0);
    printf("posix-poll: pty-ssh-close hup=1 elapsed-ms=%d\n",
           elapsed_ms(&before, &after));
    (void)close(master);
    return 0;
}

/* K548 rev 331: the sshd master as the public `pty:` clone device. `/dev/ptmx` is the clone
 * path; the pty server allocates the pair and fstat reports the pair/role inode, so the slave is
 * `/dev/pts/<pair>`. This is the SchemeFile master with a remembered public path, not the
 * pathless `posix_openpt` install. */
static int open_scheme_master(char *name, size_t capacity) {
    struct stat info;
    unsigned long pair;
    int master;

    master = open("/dev/ptmx", O_RDWR | O_NOCTTY);
    if (master < 0)
        return -1;
    if (fstat(master, &info) != 0 || info.st_ino < 3) {
        (void)close(master);
        errno = EIO;
        return -1;
    }
    pair = (unsigned long)((info.st_ino - 1) / 2);
    (void)snprintf(name, capacity, "/dev/pts/%lu", pair);
    return master;
}

/* K548 rev 331: the same sshd close shape as `pty_ssh_close`, but the master was opened through
 * the `pty:` scheme's clone path. The SchemeFile branch must report the hangup too. */
static int pty_scheme_close(void) {
    struct pollfd ready;
    struct timespec timeout;
    struct timespec before;
    struct timespec after;
    char name[128];
    char byte = 0;
    int master;
    int slave;
    int status = 0;
    int ready_count;
    pid_t child;

    master = open_scheme_master(name, sizeof(name));
    if (master < 0)
        return fail("scheme-master", -1, errno);
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0)
        return fail("scheme-slave", -1, errno);

    child = fork();
    if (child == 0) {
        (void)close(master);
        if (write(slave, "S", 1) != 1)
            _exit(2);
        (void)close(slave);
        _exit(0);
    }
    if (child < 0)
        return fail("scheme-fork", -1, errno);
    (void)close(slave);

    if (waitpid(child, &status, 0) != child)
        return fail("scheme-waitpid", child, errno);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("scheme-child-status", status, 0);
    if (read(master, &byte, 1) != 1 || byte != 'S')
        return fail("scheme-byte", (long)byte, errno);
    if (read(master, &byte, 1) != 0)
        return fail("scheme-eof", (long)byte, errno);

    ready.fd = master;
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    (void)clock_gettime(CLOCK_MONOTONIC, &before);
    ready_count = ppoll(&ready, 1, &timeout, NULL);
    (void)clock_gettime(CLOCK_MONOTONIC, &after);
    if (ready_count <= 0)
        return fail("scheme-hup-poll", ready_count, errno);
    if ((ready.revents & POLLHUP) == 0)
        return fail("scheme-hup-revents", ready.revents, 0);
    printf("posix-poll: pty-scheme-close hup=1 elapsed-ms=%d\n",
           elapsed_ms(&before, &after));
    (void)close(master);
    return 0;
}

/* K548 rev 207: the master role across execve. The sshd session child is an exec'd image, so
 * the master OFD must keep `pty_master` in the `FdMap`; otherwise the read EOF cannot mark the
 * hangup and poll never reports POLLHUP. The parent opens the scheme master, forks and execs
 * this same binary with the master as fd 3, then closes the slave. The exec'd child reads EOF
 * and requires POLLHUP within a bounded ppoll. */
static int pty_exec_child_row(void) {
    struct pollfd ready;
    struct timespec timeout;
    char byte = 0;
    int master = 3;
    int ready_count;
    int saw_eof = 0;

    /* The slave was closed before the fork, so this read is a level EOF. O_NONBLOCK keeps a
     * retry path from adding a blocking wait; the bounded loop is the row's own limit. */
    int flags = fcntl(master, F_GETFL, 0);
    if (flags >= 0)
        (void)fcntl(master, F_SETFL, flags | O_NONBLOCK);
    printf("posix-poll: pty-exec-start\n");

    for (int spins = 0; spins < 2000; spins++) {
        ssize_t count = read(master, &byte, 1);
        if (count == 0) {
            saw_eof = 1;
            break;
        }
        if (count > 0) {
            usleep(1000);
            continue;
        }
        if (errno == EAGAIN || errno == EINTR) {
            usleep(1000);
            continue;
        }
        return fail("exec-read", master, errno);
    }
    if (!saw_eof)
        return fail("exec-eof", master, 0);

    ready.fd = master;
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    ready_count = ppoll(&ready, 1, &timeout, NULL);
    if (ready_count <= 0)
        return fail("exec-hup-poll", ready_count, errno);
    if ((ready.revents & POLLHUP) == 0)
        return fail("exec-hup-revents", ready.revents, 0);
    printf("posix-poll: pty-exec-close hup=1\n");
    return 0;
}

static int pty_exec_close(void) {
    char name[128];
    int master;
    int slave;
    int status = 0;
    pid_t child;

    master = open_scheme_master(name, sizeof(name));
    if (master < 0)
        return fail("exec-master", -1, errno);
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0)
        return fail("exec-slave", -1, errno);

    /* Close the last slave before the fork, so the hangup is already a level when the exec'd
     * child reads; the role, not the close race, is then the only thing under test. */
    (void)close(slave);

    child = fork();
    if (child == 0) {
        if (master != 3) {
            if (dup2(master, 3) != 3)
                _exit(12);
            (void)close(master);
        }
        execl("/bin/libc-test", "libc-test", "exec-row-child", (char *)NULL);
        _exit(127);
    }
    if (child < 0)
        return fail("exec-fork", -1, errno);

    (void)close(master);
    if (waitpid(child, &status, 0) != child)
        return fail("exec-waitpid", child, errno);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("exec-child-status", status, 0);
    printf("posix-poll: pty-exec-parent child=%d\n", (int)child);
    return 0;
}

static volatile sig_atomic_t sigchld_seen;

static void on_sigchld(int signo) {
    (void)signo;
    sigchld_seen = 1;
}

/* K548 rev 330: ppoll must apply the caller's signal mask for the duration of the wait.
 *
 * Row A: no signal is pending, so a finite ppoll with an unblocking mask must sleep to its
 * timeout and return 0 (a naive sigprocmask+wait swap would self-kick and return EINTR).
 * Row B: the child exits during the wait, so the unblocked SIGCHLD must interrupt ppoll and its
 * handler must have run before ppoll returns. */
/* K548 rev208: the OpenSSH privsep shape. The parent's handler is already registered, then the
 * session process is forked; it re-installs SIGCHLD and waits on its own child. SIG_SETUP is
 * per thread, so the forked thread must register the kernel signal entry again or the child
 * exit never wakes the ppoll. */
static int ppoll_fork_child_row(void) {
    struct sigaction action;
    struct pollfd ready;
    struct timespec timeout;
    sigset_t block;
    sigset_t saved;
    sigset_t unblocked;
    int pipefd[2];
    int status = 0;
    int ready_count;
    pid_t child;

    memset(&action, 0, sizeof(action));
    action.sa_handler = on_sigchld;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sigaction(SIGCHLD, &action, NULL) != 0)
        return fail("fork-mask-sigaction", -1, errno);

    /* The parent left SIGCHLD blocked; the session starts from the caller's unblocked mask. */
    sigemptyset(&unblocked);
    sigaddset(&unblocked, SIGCHLD);
    if (sigprocmask(SIG_UNBLOCK, &unblocked, NULL) != 0)
        return fail("fork-mask-unblock", -1, errno);
    sigemptyset(&block);
    sigaddset(&block, SIGCHLD);
    if (sigprocmask(SIG_BLOCK, &block, &saved) != 0)
        return fail("fork-mask-block", -1, errno);
    if (pipe(pipefd) != 0)
        return fail("fork-mask-pipe", -1, errno);

    sigchld_seen = 0;
    child = fork();
    if (child == 0) {
        struct timespec delay;
        delay.tv_sec = 0;
        delay.tv_nsec = 50 * 1000 * 1000L;
        (void)nanosleep(&delay, NULL);
        _exit(0);
    }
    if (child < 0)
        return fail("fork-mask-fork", -1, errno);

    ready.fd = pipefd[0];
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    ready_count = ppoll(&ready, 1, &timeout, &saved);
    if (ready_count != -1 || errno != EINTR) {
        int saved_errno = errno;
        (void)waitpid(child, &status, 0);
        return fail("fork-mask-poll", ready_count, saved_errno);
    }
    if (!sigchld_seen) {
        (void)waitpid(child, &status, 0);
        return fail("fork-mask-handler", 0, 0);
    }
    if (waitpid(child, &status, 0) != child)
        return fail("fork-mask-waitpid", child, errno);
    (void)close(pipefd[0]);
    (void)close(pipefd[1]);
    return 0;
}

static int ppoll_mask_rows(void) {
    struct sigaction action;
    struct pollfd ready;
    struct timespec timeout;
    struct timespec before;
    struct timespec after;
    struct timespec delay;
    sigset_t block;
    sigset_t saved;
    int pipefd[2];
    int status = 0;
    int ready_count;
    pid_t child;

    memset(&action, 0, sizeof(action));
    action.sa_handler = on_sigchld;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sigaction(SIGCHLD, &action, NULL) != 0)
        return fail("mask-sigaction", -1, errno);
    sigemptyset(&block);
    sigaddset(&block, SIGCHLD);
    if (sigprocmask(SIG_BLOCK, &block, &saved) != 0)
        return fail("mask-block", -1, errno);
    if (pipe(pipefd) != 0)
        return fail("mask-pipe", -1, errno);

    /* Row A: idle source, no child, no pending signal. */
    ready.fd = pipefd[0];
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 0;
    timeout.tv_nsec = 200 * 1000 * 1000L;
    (void)clock_gettime(CLOCK_MONOTONIC, &before);
    ready_count = ppoll(&ready, 1, &timeout, &saved);
    (void)clock_gettime(CLOCK_MONOTONIC, &after);
    if (ready_count != 0)
        return fail("mask-idle", ready_count, errno);
    if (elapsed_ms(&before, &after) < 150)
        return fail("mask-idle-early", elapsed_ms(&before, &after), 0);

    /* Row B: the child exits while ppoll waits with SIGCHLD unblocked. */
    sigchld_seen = 0;
    child = fork();
    if (child == 0) {
        delay.tv_sec = 0;
        delay.tv_nsec = 50 * 1000 * 1000L;
        (void)nanosleep(&delay, NULL);
        _exit(0);
    }
    if (child < 0)
        return fail("mask-fork", -1, errno);
    ready.fd = pipefd[0];
    ready.events = POLLIN;
    ready.revents = 0;
    timeout.tv_sec = 2;
    timeout.tv_nsec = 0;
    ready_count = ppoll(&ready, 1, &timeout, &saved);
    if (ready_count != -1 || errno != EINTR)
        return fail("mask-child-poll", ready_count, errno);
    if (!sigchld_seen)
        return fail("mask-child-handler", 0, 0);
    if (waitpid(child, &status, 0) != child)
        return fail("mask-child-waitpid", child, errno);

    /* Row C: OpenSSH's privsep shape. The waiting process was forked after its parent
     * registered the handler, so it must register the per-thread kernel entry again. */
    sigchld_seen = 0;
    child = fork();
    if (child == 0)
        _exit(ppoll_fork_child_row() == 0 ? 0 : 1);
    if (child < 0)
        return fail("mask-session-fork", -1, errno);
    pid_t waited;
    do {
        waited = waitpid(child, &status, 0);
    } while (waited == -1 && errno == EINTR);
    if (waited != child)
        return fail("mask-session-waitpid", child, errno);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("mask-session-status", status, 0);
    printf("posix-poll: ppoll-mask fork ok\n");

    if (sigprocmask(SIG_SETMASK, &saved, NULL) != 0)
        return fail("mask-restore", -1, errno);
    printf("posix-poll: ppoll-mask idle+child ok\n");
    (void)close(pipefd[0]);
    (void)close(pipefd[1]);
    return 0;
}

int main(int argc, char **argv) {
    int pair[2];
    int pipefd[2];
    int master;
    int slave;
    char *name;
    int revents;

    setvbuf(stdout, NULL, _IONBF, 0);

    if (argc > 1 && strcmp(argv[1], "exec-row-child") == 0)
        return pty_exec_child_row();

    if (pty_exec_close() != 0)
        return 1;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair", -1, errno);
    revents = poll_revents(pair[1], POLLOUT, 1000);
    if (revents < 0)
        return fail("socket-pollout", pair[1], -revents);
    if ((revents & POLLOUT) == 0)
        return fail("socket-pollout-revents", pair[1], revents);
    if (write(pair[0], "x", 1) != 1)
        return fail("socket-write", pair[0], errno);
    revents = poll_revents(pair[1], POLLIN, 1000);
    if (revents < 0)
        return fail("socket-pollin", pair[1], -revents);
    if ((revents & POLLIN) == 0)
        return fail("socket-pollin-revents", pair[1], revents);
    printf("posix-poll: socket ok\n");

    if (pipe(pipefd) != 0)
        return fail("pipe", -1, errno);
    revents = poll_revents(pipefd[1], POLLOUT, 1000);
    if (revents < 0)
        return fail("pipe-pollout", pipefd[1], -revents);
    if ((revents & POLLOUT) == 0)
        return fail("pipe-pollout-revents", pipefd[1], revents);
    if (write(pipefd[1], "y", 1) != 1)
        return fail("pipe-write", pipefd[1], errno);
    revents = poll_revents(pipefd[0], POLLIN, 1000);
    if (revents < 0)
        return fail("pipe-pollin", pipefd[0], -revents);
    if ((revents & POLLIN) == 0)
        return fail("pipe-pollin-revents", pipefd[0], revents);
    printf("posix-poll: pipe ok\n");

    master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0)
        return fail("posix_openpt", -1, errno);
    if (grantpt(master) != 0)
        return fail("grantpt", master, errno);
    if (unlockpt(master) != 0)
        return fail("unlockpt", master, errno);
    name = ptsname(master);
    if (name == NULL)
        return fail("ptsname", master, errno);
    /* The ssh client's stdout can be a write-only pty slave: WRITE but no READ. */
    slave = open(name, O_WRONLY | O_NOCTTY);
    if (slave < 0)
        return fail("open-slave", -1, errno);
    revents = poll_revents(slave, POLLOUT, 1000);
    if (revents < 0)
        return fail("pty-pollout", slave, -revents);
    if ((revents & POLLOUT) == 0)
        return fail("pty-pollout-revents", slave, revents);
    printf("posix-poll: pty ok\n");

    if (pty_ssh_close() != 0)
        return 1;
    if (pty_scheme_close() != 0)
        return 1;
    if (ppoll_mask_rows() != 0)
        return 1;

    /* Print the verdict before the last closes: the pty server writes its close-time counters
     * on the same console, and a late write can splice into the verdict line. */
    printf("posix-poll: ok\n");
    (void)fflush(stdout);

    close(slave);
    close(master);
    close(pipefd[0]);
    close(pipefd[1]);
    close(pair[0]);
    close(pair[1]);
    return 0;
}
