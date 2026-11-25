/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/socket.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef	_SYS_SOCKET_H
#define	_SYS_SOCKET_H
#include <sys/cdefs.h>
#include <bits/abi_socket.h>
__BEGIN_DECLS

#define __NEED_socklen_t
#define __NEED_sa_family_t
#define __NEED_size_t
#define __NEED_ssize_t
#define __NEED_uid_t
#define __NEED_pid_t
#define __NEED_gid_t
#define __NEED_struct_iovec

#include <bits/alltypes.h>

#include <bits/socket.h>

/* RavenBSD: DR-69 FreeBSD sys/sys/socket.h:444. The padding at 12, 20, 28 and 40 is
 * implicit; the donor has no named pad members. */
struct msghdr {
	void *msg_name;
	socklen_t msg_namelen;
	struct iovec *msg_iov;
	int msg_iovlen;
	void *msg_control;
	socklen_t msg_controllen;
	int msg_flags;
};

/* RavenBSD: DR-69 FreeBSD sys/sys/socket.h:496; 12 bytes, no named pad member. */
struct cmsghdr {
	socklen_t cmsg_len;
	int cmsg_level;
	int cmsg_type;
};

#if __BSD_VISIBLE
struct ucred {
	pid_t pid;
	uid_t uid;
	gid_t gid;
};
#endif

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
/* RavenBSD: DR-69 FreeBSD sys/sys/socket.h:680; ssize_t msg_len. */
struct mmsghdr {
	struct msghdr msg_hdr;
	ssize_t msg_len;
};

struct timespec;

ssize_t sendmmsg (int, struct mmsghdr *, size_t, int);
ssize_t recvmmsg (int, struct mmsghdr *__restrict, size_t, int, const struct timespec *__restrict);
#endif

struct linger {
	int l_onoff;
	int l_linger;
};

#ifndef SOCK_STREAM
#endif

#ifndef SOCK_CLOEXEC
#endif

#ifndef SO_DEBUG
#endif

#ifndef SO_RCVTIMEO
#endif

#ifndef SO_TIMESTAMP
#endif

#ifndef SOL_SOCKET
#endif

/* RavenBSD: DR-69 FreeBSD sys/sys/socket.h:564. `CMSG_DATA` starts at
 * `_ALIGN(sizeof(struct cmsghdr))`, which is 16 on a 64-bit target even though the
 * structure itself is 12 bytes. `CMSG_LEN(n)` is 16 + n. */
#define __CMSG_ALIGN(n) (((n) + sizeof(void *) - 1) & ~(sizeof(void *) - 1))
#define __CMSG_LEN(cmsg) __CMSG_ALIGN((cmsg)->cmsg_len)
#define __CMSG_NEXT(cmsg) ((unsigned char *)(cmsg) + __CMSG_LEN(cmsg))
#define __MHDR_END(mhdr) ((unsigned char *)(mhdr)->msg_control + (mhdr)->msg_controllen)

#define CMSG_DATA(cmsg) ((unsigned char *)(cmsg) + __CMSG_ALIGN(sizeof(struct cmsghdr)))
#define CMSG_NXTHDR(mhdr, cmsg) ((cmsg) == 0 ? CMSG_FIRSTHDR(mhdr) : \
	__CMSG_LEN(cmsg) + __CMSG_ALIGN(sizeof(struct cmsghdr)) > \
	__MHDR_END(mhdr) - (unsigned char *)(cmsg) \
	? 0 : (struct cmsghdr *)__CMSG_NEXT(cmsg))
#define CMSG_FIRSTHDR(mhdr) ((size_t) (mhdr)->msg_controllen >= sizeof (struct cmsghdr) ? (struct cmsghdr *) (mhdr)->msg_control : (struct cmsghdr *) 0)

#define CMSG_ALIGN(len) __CMSG_ALIGN(len)
#define CMSG_SPACE(len) (__CMSG_ALIGN (sizeof (struct cmsghdr)) + __CMSG_ALIGN (len))
#define CMSG_LEN(len)   (__CMSG_ALIGN (sizeof (struct cmsghdr)) + (len))

/* RavenBSD: DR-69 FreeBSD sys/sys/socket.h:336. */
struct sockaddr {
	unsigned char sa_len;
	unsigned char sa_family;
	char sa_data[14];
};

/* RavenBSD: DR-69 FreeBSD sys/sys/_sockaddr_storage.h. */
struct sockaddr_storage {
	unsigned char ss_len;
	unsigned char ss_family;
	char __ss_pad1[sizeof(long long) - sizeof(unsigned char) - sizeof(unsigned char)];
	long long __ss_align;
	char __ss_pad2[128 - sizeof(unsigned char) - sizeof(unsigned char)
	    - (sizeof(long long) - sizeof(unsigned char) - sizeof(unsigned char))
	    - sizeof(long long)];
};

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int socket (int, int, int);
int socketpair (int, int, int, int [2]);

int shutdown (int, int);

int bind (int, const struct sockaddr *, socklen_t);
int connect (int, const struct sockaddr *, socklen_t);
int listen (int, int);
int accept (int, struct sockaddr *__restrict, socklen_t *__restrict);
#if __BSD_VISIBLE
int accept4(int, struct sockaddr *__restrict, socklen_t *__restrict, int);
#endif

int getsockname (int, struct sockaddr *__restrict, socklen_t *__restrict);
int getpeername (int, struct sockaddr *__restrict, socklen_t *__restrict);
#if __BSD_VISIBLE
int getpeereid (int, uid_t *, gid_t *);
#endif

ssize_t send (int, const void *, size_t, int);
ssize_t recv (int, void *, size_t, int);
ssize_t sendto (int, const void *, size_t, int, const struct sockaddr *, socklen_t);
ssize_t recvfrom (int, void *__restrict, size_t, int, struct sockaddr *__restrict, socklen_t *__restrict);
ssize_t sendmsg (int, const struct msghdr *, int);
ssize_t recvmsg (int, struct msghdr *, int);

int getsockopt (int, int, int, void *__restrict, socklen_t *__restrict);
int setsockopt (int, int, int, const void *, socklen_t);

int sockatmark (int);

#endif
__END_DECLS
#endif
