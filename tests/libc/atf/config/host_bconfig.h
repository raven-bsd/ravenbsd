/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * Hand-written equivalents of the bconfig.h macros atf-c tests, for the Linux host.
 * Never the NetBSD-generated bconfig.h (ticket C71, 4.4).
 */
#ifndef RL_HOST_BCONFIG_H
#define RL_HOST_BCONFIG_H

#define HAVE_SETENV 1
#define HAVE_UNSETENV 1
#define HAVE_PUTENV 1
#define HAVE_GETCWD_DYN 1
#define HAVE_GNU_GETOPT 1
/* glibc has no optreset; HAVE_OPTRESET stays undefined. */

#define PACKAGE "atf"
#define PACKAGE_NAME "Automated Testing Framework"
#define PACKAGE_VERSION "0.20"
#define PACKAGE_STRING "Automated Testing Framework 0.20"
#define PACKAGE_BUGREPORT "atf-devel@NetBSD.org"
#define PACKAGE_COPYRIGHT "Copyright (c) 2007-2012 The NetBSD Foundation, Inc."
#define PACKAGE_TARNAME "atf"
#define PACKAGE_URL "https://github.com/jmmv/atf/"
#define VERSION "0.20"

#endif /* !defined(RL_HOST_BCONFIG_H) */
