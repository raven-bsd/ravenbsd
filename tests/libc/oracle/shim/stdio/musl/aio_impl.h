/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement "aio_impl.h" for the musl stdio oracle: the close hook is the identity.
 */
#ifndef RL_STDIO_MUSL_AIO_IMPL_H
#define RL_STDIO_MUSL_AIO_IMPL_H

static inline int __aio_close(int fd)
{
	return fd;
}

#endif
