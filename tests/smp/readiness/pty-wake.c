// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495 rev156: three-way pty-master ppoll discriminator for the SSH close hang.
//
// The K548 trace showed sshd-session blocked in ppoll before collect_children/waitpid and the
// channel close. This fixture isolates the three wake sources on a pty master:
//
//   R1  child writes, closes the slave explicitly and exits; no SIGCHLD handler. The parent's
//       ppoll(master) must become ready (POLLIN or POLLHUP) within the bound, and the re-poll
//       must see HUP. ppoll must not return EINTR without a handler.
//   R2  same child, but a caught SIGCHLD handler writes a self-pipe. The handler must run and
//       ppoll must return POSIX-correct: EINTR (handler ran) or the self-pipe/master ready.
//   R3  child writes and exits without an explicit slave close (exit closes it). The master must
//       still report HUP/readable; if R1 passes and R3 fails, the process-exit close path is the
//       missing wake.
//
// The rows print one line each and the binary exits 1 if any required wake is absent.

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#define PTY_WAKE_BOUND_MS 2000
#define PTY_WAKE_REPOLL_MS 500

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

static void say_line(const char *id, int ok, const char *detail) {
    char line[240];
    int length = snprintf(line, sizeof(line), "PTY-WAKE %s %s %s\n", id, detail,
                          ok ? "ok" : "FAIL");
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static void sleep_ms(long milliseconds) {
    struct timespec delay;
    delay.tv_sec = milliseconds / 1000;
    delay.tv_nsec = (milliseconds % 1000) * 1000000L;
    while (nanosleep(&delay, &delay) != 0 && errno == EINTR) {
    }
}

static int open_master(char *name, size_t size) {
    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0) {
        return -1;
    }
    if (grantpt(master) != 0 || unlockpt(master) != 0) {
        (void)close(master);
        return -1;
    }
    const char *slave_name = ptsname(master);
    if (slave_name == NULL) {
        (void)close(master);
        return -1;
    }
    (void)fcntl(master, F_SETFL, O_NONBLOCK);
    (void)snprintf(name, size, "%s", slave_name);
    return master;
}

static int make_raw(int fd) {
    struct termios settings;
    if (tcgetattr(fd, &settings) != 0) {
        return -1;
    }
    settings.c_lflag &= (tcflag_t)~(ICANON | ECHO | ISIG);
    settings.c_iflag &= (tcflag_t)~(ICRNL | IXON);
    return tcsetattr(fd, TCSANOW, &settings);
}

static int poll_ms(int fd, int events, int milliseconds, struct timespec *elapsed) {
    struct pollfd pfd;
    struct timespec timeout;
    struct timespec before;
    struct timespec after;
    int ready;

    pfd.fd = fd;
    pfd.events = (short)events;
    pfd.revents = 0;
    timeout.tv_sec = milliseconds / 1000;
    timeout.tv_nsec = (milliseconds % 1000) * 1000000L;
    (void)clock_gettime(CLOCK_MONOTONIC, &before);
    errno = 0;
    ready = ppoll(&pfd, 1, &timeout, NULL);
    (void)clock_gettime(CLOCK_MONOTONIC, &after);
    elapsed->tv_sec = after.tv_sec - before.tv_sec;
    elapsed->tv_nsec = after.tv_nsec - before.tv_nsec;
    if (elapsed->tv_nsec < 0) {
        elapsed->tv_sec--;
        elapsed->tv_nsec += 1000000000L;
    }
    if (ready > 0) {
        return pfd.revents;
    }
    return ready;
}

static int drain_master(int fd) {
    char buffer[64];
    for (int attempt = 0; attempt < 8; attempt++) {
        ssize_t count = read(fd, buffer, sizeof(buffer));
        if (count == 0) {
            return 1; // EOF/HUP
        }
        if (count < 0) {
            return errno == EAGAIN || errno == EWOULDBLOCK ? 0 : -1;
        }
    }
    return 0;
}

static int drain_search(int fd, const char *needle, int *seen, char *head, size_t head_size) {
    char buffer[128];
    int found = 0;
    size_t used = 0;

    if (head != NULL && head_size > 0) {
        head[0] = '\0';
    }
    for (int attempt = 0; attempt < 256; attempt++) {
        ssize_t count = read(fd, buffer, sizeof(buffer) - 1);
        if (count == 0) {
            *seen = found;
            return 1; // EOF/HUP
        }
        if (count < 0) {
            *seen = found;
            return errno == EAGAIN || errno == EWOULDBLOCK ? 0 : -1;
        }
        if (head != NULL) {
            for (ssize_t index = 0; index < count && used + 1 < head_size; index++) {
                unsigned char byte = (unsigned char)buffer[index];
                head[used++] = byte >= 32 && byte < 127 ? (char)byte : '.';
            }
            head[used] = '\0';
        }
        buffer[count] = '\0';
        if (strstr(buffer, needle) != NULL) {
            found = 1;
        }
    }
    *seen = found;
    return 0;
}

static volatile sig_atomic_t sigchld_count;
static int sigchld_pipe[2] = {-1, -1};

static void sigchld_handler(int signal_number) {
    int saved = errno;
    sigchld_count++;
    if (sigchld_pipe[1] >= 0) {
        char byte = 1;
        ssize_t ignored = write(sigchld_pipe[1], &byte, 1);
        (void)ignored;
    }
    errno = saved;
    (void)signal_number;
}

static int install_sigchld(void) {
    struct sigaction action;

    if (pipe(sigchld_pipe) != 0) {
        return -1;
    }
    (void)fcntl(sigchld_pipe[0], F_SETFL, O_NONBLOCK);
    (void)fcntl(sigchld_pipe[1], F_SETFL, O_NONBLOCK);
    memset(&action, 0, sizeof(action));
    action.sa_handler = sigchld_handler;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0; // no SA_RESTART: the blocked ppoll must see EINTR
    return sigaction(SIGCHLD, &action, NULL);
}

static int run_child_and_poll(int explicit_close, int handler) {
    char name[128];
    char detail[200];
    struct timespec elapsed;
    int master;
    int slave;
    int status = 0;
    int revents;
    int repoll;
    int hup;
    pid_t child;

    master = open_master(name, sizeof(name));
    if (master < 0) {
        (void)snprintf(detail, sizeof(detail), "open_master errno=%d", errno);
        return -1;
    }
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        (void)snprintf(detail, sizeof(detail), "open_slave errno=%d", errno);
        (void)close(master);
        return -1;
    }
    (void)make_raw(slave);

    child = fork();
    if (child == 0) {
        (void)close(master);
        if (handler) {
            sleep_ms(100); // let the parent enter ppoll before SIGCHLD
        }
        (void)write(slave, "wake\n", 5);
        if (explicit_close) {
            (void)close(slave);
        }
        _exit(0);
    }
    if (child < 0) {
        (void)snprintf(detail, sizeof(detail), "fork errno=%d", errno);
        (void)close(slave);
        (void)close(master);
        return -1;
    }
    (void)close(slave);

    if (handler) {
        struct pollfd fds[2];
        struct timespec timeout;

        fds[0].fd = master;
        fds[0].events = POLLIN | POLLHUP;
        fds[0].revents = 0;
        fds[1].fd = sigchld_pipe[0];
        fds[1].events = POLLIN;
        fds[1].revents = 0;
        timeout.tv_sec = PTY_WAKE_BOUND_MS / 1000;
        timeout.tv_nsec = (PTY_WAKE_BOUND_MS % 1000) * 1000000L;
        errno = 0;
        revents = ppoll(fds, 2, &timeout, NULL);
        int saved = errno;
        int pipe_ready = (revents > 0 && (fds[1].revents & POLLIN) != 0) ? 1 : 0;
        int master_ready = (revents > 0 && (fds[0].revents & (POLLIN | POLLHUP)) != 0) ? 1 : 0;
        int eintr = (revents == -1 && saved == EINTR) ? 1 : 0;
        // Reap the child; a second SIGCHLD may interrupt waitpid, POSIX says retry.
        do {
            (void)waitpid(child, &status, 0);
        } while (errno == EINTR);
        (void)close(master);
        if (sigchld_count < 1) {
            (void)snprintf(detail, sizeof(detail), "n=%d errno=%d sig=%d", revents, saved,
                           (int)sigchld_count);
            return -1;
        }
        if (!eintr && !pipe_ready && !master_ready) {
            (void)snprintf(detail, sizeof(detail), "n=%d errno=%d sig=%d pipe=%d master=%d",
                           revents, saved, (int)sigchld_count, pipe_ready, master_ready);
            return -1;
        }
        (void)snprintf(detail, sizeof(detail), "n=%d errno=%d sig=%d pipe=%d master=%d eintr=%d",
                       revents, saved, (int)sigchld_count, pipe_ready, master_ready, eintr);
        sleep_ms(150);
        (void)say_line("R2", 1, detail);
        return 0;
    }

    revents = poll_ms(master, POLLIN | POLLHUP, PTY_WAKE_BOUND_MS, &elapsed);
    int saved = errno;
    if (revents < 0) {
        (void)snprintf(detail, sizeof(detail), "n=%d errno=%d bound=%dms", revents, saved,
                       PTY_WAKE_BOUND_MS);
        (void)close(master);
        (void)waitpid(child, &status, 0);
        return -1;
    }
    if (revents == 0 || (revents & (POLLIN | POLLHUP)) == 0) {
        (void)snprintf(detail, sizeof(detail), "n=%d errno=%d revents=0x%x timeout", revents, saved,
                       revents);
        (void)close(master);
        (void)waitpid(child, &status, 0);
        return -1;
    }
    int drained = drain_master(master);
    repoll = poll_ms(master, POLLIN | POLLHUP, PTY_WAKE_REPOLL_MS, &elapsed);
    hup = (repoll > 0 && (repoll & POLLHUP) != 0) || drained == 1;
    // The first ppoll must not be EINTR without a handler.
    int first_eintr = (revents == -1 && saved == EINTR) ? 1 : 0;
    (void)waitpid(child, &status, 0);
    (void)close(master);
    if (!hup || first_eintr) {
        (void)snprintf(detail, sizeof(detail), "n=%d revents=0x%x repoll=0x%x hup=%d eintr=%d",
                       revents, revents, repoll, hup, first_eintr);
        return -1;
    }
    (void)snprintf(detail, sizeof(detail), "n=%d revents=0x%x repoll=0x%x hup=%d eintr=%d",
                   revents, revents, repoll, hup, first_eintr);
    sleep_ms(150);
    (void)say_line(explicit_close ? "R1" : "R3", 1, detail);
    return 0;
}

static int run_exec_child_and_poll(void) {
    char name[128];
    char detail[200];
    char data[64];
    struct timespec elapsed;
    int master;
    int slave;
    int status = 0;
    int revents;
    int repoll;
    int hup;
    int saw_wake = 0;
    pid_t child;

    master = open_master(name, sizeof(name));
    if (master < 0) {
        return -1;
    }
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        (void)close(master);
        return -1;
    }
    (void)make_raw(slave);

    child = fork();
    if (child == 0) {
        (void)close(master);
        (void)dup2(slave, 0);
        (void)dup2(slave, 1);
        (void)dup2(slave, 2);
        if (slave > 2) {
            (void)close(slave);
        }
        execl("/bin/brush", "brush", "-c", "echo wake", (char *)NULL);
        _exit(127);
    }
    if (child < 0) {
        (void)close(slave);
        (void)close(master);
        return -1;
    }
    (void)close(slave);

    revents = poll_ms(master, POLLIN | POLLHUP, PTY_WAKE_BOUND_MS, &elapsed);
    int saved = errno;
    int drained = drain_search(master, "wake", &saw_wake, data, sizeof(data));
    repoll = poll_ms(master, POLLIN | POLLHUP, PTY_WAKE_REPOLL_MS, &elapsed);
    hup = (repoll > 0 && (repoll & POLLHUP) != 0) || drained == 1;
    (void)waitpid(child, &status, 0);
    (void)close(master);
    int exited = WIFEXITED(status) ? 1 : 0;
    int code = exited ? WEXITSTATUS(status) : -1;
    int signaled = WIFSIGNALED(status) ? 1 : 0;
    int termsig = signaled ? WTERMSIG(status) : 0;
    if (revents <= 0 || (revents & (POLLIN | POLLHUP)) == 0 || !saw_wake || !hup) {
        (void)snprintf(detail, sizeof(detail),
                       "n=%d errno=%d revents=0x%x repoll=0x%x wake=%d hup=%d exit=%d code=%d sig=%d data=\"%s\"",
                       revents, saved, revents, repoll, saw_wake, hup, exited, code,
                       signaled ? 128 + termsig : 0, data);
        (void)say_line("R4", 0, detail);
        return -1;
    }
    (void)snprintf(detail, sizeof(detail),
                   "n=%d revents=0x%x repoll=0x%x wake=%d hup=%d exit=%d code=%d sig=%d data=\"%s\"",
                   revents, revents, repoll, saw_wake, hup, exited, code,
                   signaled ? 128 + termsig : 0, data);
    sleep_ms(150);
    (void)say_line("R4", 1, detail);
    return 0;
}

static int run_multi_fd_and_poll(void) {
    char name[128];
    char detail[200];
    struct pollfd fds[2];
    struct timespec timeout;
    struct timespec elapsed;
    int pair[2];
    int master;
    int slave;
    int status = 0;
    int ready;
    int repoll;
    int hup;
    int saw_wake = 0;
    pid_t child;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0) {
        return -1;
    }
    master = open_master(name, sizeof(name));
    if (master < 0) {
        (void)close(pair[0]);
        (void)close(pair[1]);
        return -1;
    }
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        (void)close(master);
        (void)close(pair[0]);
        (void)close(pair[1]);
        return -1;
    }
    (void)make_raw(slave);

    child = fork();
    if (child == 0) {
        (void)close(master);
        (void)close(pair[0]);
        (void)close(pair[1]);
        (void)write(slave, "wake\n", 5);
        (void)close(slave);
        _exit(0);
    }
    if (child < 0) {
        (void)close(slave);
        (void)close(master);
        (void)close(pair[0]);
        (void)close(pair[1]);
        return -1;
    }
    (void)close(slave);

    fds[0].fd = master;
    fds[0].events = POLLIN | POLLHUP;
    fds[0].revents = 0;
    fds[1].fd = pair[0]; // quiet second source, like sshd's connection_in
    fds[1].events = POLLIN;
    fds[1].revents = 0;
    timeout.tv_sec = PTY_WAKE_BOUND_MS / 1000;
    timeout.tv_nsec = (PTY_WAKE_BOUND_MS % 1000) * 1000000L;
    errno = 0;
    ready = ppoll(fds, 2, &timeout, NULL);
    int saved = errno;
    int drained = drain_search(master, "wake", &saw_wake, NULL, 0);
    repoll = poll_ms(master, POLLIN | POLLHUP, PTY_WAKE_REPOLL_MS, &elapsed);
    hup = (repoll > 0 && (repoll & POLLHUP) != 0) || drained == 1;
    (void)waitpid(child, &status, 0);
    (void)close(master);
    (void)close(pair[0]);
    (void)close(pair[1]);
    if (ready <= 0 || (fds[0].revents & (POLLIN | POLLHUP)) == 0 || !saw_wake || !hup) {
        (void)snprintf(detail, sizeof(detail),
                       "n=%d errno=%d master=0x%x other=0x%x repoll=0x%x wake=%d hup=%d", ready,
                       saved, fds[0].revents, fds[1].revents, repoll, saw_wake, hup);
        (void)say_line("R5", 0, detail);
        return -1;
    }
    (void)snprintf(detail, sizeof(detail),
                   "n=%d master=0x%x other=0x%x repoll=0x%x wake=%d hup=%d", ready,
                   fds[0].revents, fds[1].revents, repoll, saw_wake, hup);
    sleep_ms(150);
    (void)say_line("R5", 1, detail);
    return 0;
}

static int run_ctty_and_open(void) {
    char name[128];
    char detail[200];
    int master;
    int slave;
    int status = 0;
    pid_t child;

    master = open_master(name, sizeof(name));
    if (master < 0) {
        return -1;
    }
    slave = open(name, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        (void)close(master);
        return -1;
    }
    (void)make_raw(slave);

    child = fork();
    if (child == 0) {
        (void)close(master);
        if (setsid() < 0) {
            _exit(11);
        }
        if (ioctl(slave, TIOCSCTTY, 0) != 0) {
            _exit(12);
        }
        int ctty = open("/dev/tty", O_RDWR);
        if (ctty < 0) {
            _exit(13);
        }
        struct stat slave_stat;
        struct stat ctty_stat;
        if (fstat(slave, &slave_stat) != 0) {
            _exit(14);
        }
        if (fstat(ctty, &ctty_stat) != 0) {
            _exit(15);
        }
        if (slave_stat.st_dev != ctty_stat.st_dev || slave_stat.st_ino != ctty_stat.st_ino) {
            _exit(16);
        }
        (void)write(slave, "ctty\n", 5);
        _exit(0);
    }
    if (child < 0) {
        (void)close(slave);
        (void)close(master);
        return -1;
    }
    (void)close(slave);
    (void)waitpid(child, &status, 0);
    (void)close(master);
    int exited = WIFEXITED(status);
    int code = exited ? WEXITSTATUS(status) : -1;
    if (!exited || code != 0) {
        (void)snprintf(detail, sizeof(detail), "setsid/ioctl/open/fstat child code=%d", code);
        (void)say_line("R6", 0, detail);
        return -1;
    }
    (void)snprintf(detail, sizeof(detail), "setsid=ok ctty=open-ok fstat=same");
    sleep_ms(150);
    (void)say_line("R6", 1, detail);
    return 0;
}

int main(void) {
    int failures = 0;
    int result;

    // Let the one-shot service's console attach settle before the first marker; the early netd
    // startup line can otherwise split the first row.
    sleep_ms(500);
    say("PTY-WAKE start\n");
    result = run_child_and_poll(1, 0);
    if (result != 0) {
        say("PTY-WAKE R1 no pty HUP/readable without a handler FAIL\n");
        failures++;
    }
    if (install_sigchld() != 0) {
        say("PTY-WAKE R2 sigaction FAIL\n");
        failures++;
    } else {
        result = run_child_and_poll(1, 1);
        if (result != 0) {
            say("PTY-WAKE R2 no SIGCHLD/ppoll wake FAIL\n");
            failures++;
        }
    }
    result = run_child_and_poll(0, 0);
    if (result != 0) {
        say("PTY-WAKE R3 no pty HUP after exit-without-close FAIL\n");
        failures++;
    }
    result = run_exec_child_and_poll();
    if (result != 0) {
        say("PTY-WAKE R4 no pty wake after exec'd brush exit FAIL\n");
        failures++;
    }
    result = run_multi_fd_and_poll();
    if (result != 0) {
        say("PTY-WAKE R5 no pty HUP with a quiet second poll source FAIL\n");
        failures++;
    }
    result = run_ctty_and_open();
    if (result != 0) {
        say("PTY-WAKE R6 no controlling terminal / /dev/tty open FAIL\n");
        failures++;
    }
    if (failures != 0) {
        say("PTY-WAKE SUMMARY FAIL\n");
        return 1;
    }
    say("PTY-WAKE SUMMARY PASS 6/6\n");
    return 0;
}
