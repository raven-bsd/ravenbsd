/* SPDX-License-Identifier: MIT
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * S746 review fixture: the old musl-profile open flags, kept as an isolated
 * negative double. It is never installed and never included by product code.
 */
#ifndef _S746_FCNTL_MUSL_FIXTURE_H
#define _S746_FCNTL_MUSL_FIXTURE_H

#define O_CLOEXEC 02000000
#define O_NONBLOCK 04000

#endif /* !_S746_FCNTL_MUSL_FIXTURE_H */
