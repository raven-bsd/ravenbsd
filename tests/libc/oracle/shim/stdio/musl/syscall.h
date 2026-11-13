/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <syscall.h> for the musl stdio oracle, written from scratch.  Every system call
 * is forwarded to the function-pointer table the Rust harness installs, so the oracle and the
 * Rust subject run over the same in-memory file system.  Errors are the negative errno values
 * musl's raw syscalls use; __syscall_ret turns them into -1 plus errno.
 */
#ifndef RL_STDIO_MUSL_SYSCALL_H
#define RL_STDIO_MUSL_SYSCALL_H

#include <errno.h>
#include <stddef.h>
#include <sys/uio.h>
#include <fcntl.h>

#define SYS_read 0
#define SYS_write 1
#define SYS_open 2
#define SYS_close 3
#define SYS_fstat 5
#define SYS_lseek 8
#define SYS_ioctl 16
#define SYS_readv 19
#define SYS_writev 20
#define SYS_fcntl 72
#define SYS_rename 82
#define SYS_rmdir 84
#define SYS_unlink 87
#define SYS_dup3 292
#define SYS_pipe2 293

struct rl_stdio_hooks {
	long (*read)(int, void *, unsigned long);
	long (*write)(int, const void *, unsigned long);
	long (*read2)(int, void *, unsigned long, void *, unsigned long);
	long (*write2)(int, const void *, unsigned long, const void *, unsigned long);
	long (*seek)(int, long, int);
	long (*close)(int);
	long (*ioctl)(int, unsigned long, void *);
	long (*fcntl)(int, int, long);
	long (*open)(const char *, int, int);
	long (*unlink)(const char *);
	long (*rmdir)(const char *);
	long (*rename)(const char *, const char *);
	void (*trace)(int, int, long, long);
};

__attribute__((__weak__)) struct rl_stdio_hooks rl_stdio_hooks;

static inline long __rl_syscall(long n, long a1, long a2, long a3, long a4, long a5, long a6)
{
	(void)a4;
	(void)a5;
	(void)a6;
	switch (n) {
	case SYS_read:
		return rl_stdio_hooks.read ? rl_stdio_hooks.read((int)a1, (void *)a2, (unsigned long)a3) : -ENOSYS;
	case SYS_write:
		return rl_stdio_hooks.write ? rl_stdio_hooks.write((int)a1, (const void *)a2, (unsigned long)a3) : -ENOSYS;
	case SYS_readv: {
		struct iovec *iov = (struct iovec *)a2;
		long count = a3;
		if (count >= 2 && rl_stdio_hooks.read2)
			return rl_stdio_hooks.read2((int)a1, iov[0].iov_base, iov[0].iov_len,
						   iov[1].iov_base, iov[1].iov_len);
		if (count >= 1 && rl_stdio_hooks.read)
			return rl_stdio_hooks.read((int)a1, iov[0].iov_base, iov[0].iov_len);
		return -ENOSYS;
	}
	case SYS_writev: {
		struct iovec *iov = (struct iovec *)a2;
		long count = a3;
		if (count >= 2 && rl_stdio_hooks.write2)
			return rl_stdio_hooks.write2((int)a1, iov[0].iov_base, iov[0].iov_len,
						    iov[1].iov_base, iov[1].iov_len);
		if (count >= 1 && rl_stdio_hooks.write)
			return rl_stdio_hooks.write((int)a1, iov[0].iov_base, iov[0].iov_len);
		return -ENOSYS;
	}
	case SYS_lseek:
		return rl_stdio_hooks.seek ? rl_stdio_hooks.seek((int)a1, a2, (int)a3) : -ENOSYS;
	case SYS_close:
		return rl_stdio_hooks.close ? rl_stdio_hooks.close((int)a1) : -ENOSYS;
	case SYS_ioctl:
		return rl_stdio_hooks.ioctl ? rl_stdio_hooks.ioctl((int)a1, (unsigned long)a2, (void *)a3) : -ENOTTY;
	case SYS_fcntl:
		return rl_stdio_hooks.fcntl ? rl_stdio_hooks.fcntl((int)a1, (int)a2, a3) : -ENOSYS;
	case SYS_open:
		return rl_stdio_hooks.open ? rl_stdio_hooks.open((const char *)a1, (int)a2, (int)a3) : -ENOSYS;
	case SYS_unlink:
		return rl_stdio_hooks.unlink ? rl_stdio_hooks.unlink((const char *)a1) : -ENOSYS;
	case SYS_rmdir:
		return rl_stdio_hooks.rmdir ? rl_stdio_hooks.rmdir((const char *)a1) : -ENOSYS;
	case SYS_rename:
		return rl_stdio_hooks.rename ? rl_stdio_hooks.rename((const char *)a1, (const char *)a2) : -ENOSYS;
	default:
		return -ENOSYS;
	}
}

#define __syscall1(n, a) __rl_syscall((n), (long)(a), 0, 0, 0, 0, 0)
#define __syscall2(n, a, b) __rl_syscall((n), (long)(a), (long)(b), 0, 0, 0, 0)
#define __syscall3(n, a, b, c) __rl_syscall((n), (long)(a), (long)(b), (long)(c), 0, 0, 0)
#define __syscall4(n, a, b, c, d) __rl_syscall((n), (long)(a), (long)(b), (long)(c), (long)(d), 0, 0)
#define __syscall5(n, a, b, c, d, e) __rl_syscall((n), (long)(a), (long)(b), (long)(c), (long)(d), (long)(e), 0)
#define __syscall6(n, a, b, c, d, e, f) __rl_syscall((n), (long)(a), (long)(b), (long)(c), (long)(d), (long)(e), (long)(f))

#define __SYSCALL_NARGS_X(a, b, c, d, e, f, g, h, n, ...) n
#define __SYSCALL_NARGS(...) __SYSCALL_NARGS_X(__VA_ARGS__, 7, 6, 5, 4, 3, 2, 1, 0, )
#define __SYSCALL_CONCAT_X(a, b) a##b
#define __SYSCALL_CONCAT(a, b) __SYSCALL_CONCAT_X(a, b)
#define __SYSCALL_DISP(b, ...) __SYSCALL_CONCAT(b, __SYSCALL_NARGS(__VA_ARGS__))(__VA_ARGS__)
#define __syscall(...) __SYSCALL_DISP(__syscall, __VA_ARGS__)

static inline long __syscall_ret(unsigned long r)
{
	if (r > (unsigned long)-4096) {
		errno = (int)-r;
		return -1;
	}
	return (long)r;
}

#define syscall(...) __syscall_ret(__syscall(__VA_ARGS__))

#define __sys_open3(n, p, fl, mo) __syscall3(SYS_open, p, (fl), mo)
#define __sys_open2(n, p, fl) __syscall3(SYS_open, p, (fl), 0)
#define __sys_open(...) __SYSCALL_DISP(__sys_open, , __VA_ARGS__)
#define sys_open(...) __syscall_ret(__sys_open(__VA_ARGS__))

static inline off_t __lseek(int fd, off_t offset, int whence)
{
	return (off_t)__syscall_ret((unsigned long)__syscall3(SYS_lseek, fd, offset, whence));
}

static inline int __dup3(int oldfd, int newfd, int flags)
{
	return (int)__syscall_ret((unsigned long)__syscall3(SYS_dup3, oldfd, newfd, flags));
}

#endif
