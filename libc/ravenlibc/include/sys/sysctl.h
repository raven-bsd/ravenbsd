/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 1989, 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * This code is derived from software contributed to Berkeley by
 * Mike Karels at Berkeley Software Design, Inc.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from freebsd:sys/sys/sysctl.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
 */

#ifndef _SYS_SYSCTL_H_
#define _SYS_SYSCTL_H_

#include <sys/cdefs.h>
#include <sys/types.h>
#include <stddef.h>

#define CTL_MAXNAME 24

#define CTL_SYSCTL 0
#define CTL_KERN 1
#define CTL_VM 2
#define CTL_VFS 3
#define CTL_NET 4
#define CTL_DEBUG 5
#define CTL_HW 6
#define CTL_MACHDEP 7
#define CTL_USER 8
#define CTL_P1003_1B 9

#define KERN_OSTYPE 1
#define KERN_OSRELEASE 2
#define KERN_VERSION 4
#define KERN_ARGMAX 8
#define KERN_HOSTNAME 10

#define HW_MACHINE 1
#define HW_MODEL 2
#define HW_NCPU 3
#define HW_PHYSMEM 5
#define HW_PAGESIZE 7

__BEGIN_DECLS
int sysctl(const int *, unsigned int, void *, size_t *, const void *, size_t);
int sysctlbyname(const char *, void *, size_t *, const void *, size_t);
int sysctlnametomib(const char *, int *, size_t *);
__END_DECLS

#endif /* _SYS_SYSCTL_H_ */
