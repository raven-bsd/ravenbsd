/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * Values taken from the ravenlibc headers for a target build: stdlib.h declares setenv,
 * unsetenv and putenv, unistd.h declares getcwd. Raven has no optreset and no GNU getopt '+'
 * extension, so those two stay undefined.
 */
#ifndef RL_TARGET_BCONFIG_H
#define RL_TARGET_BCONFIG_H

#define HAVE_SETENV 1
#define HAVE_UNSETENV 1
#define HAVE_PUTENV 1
#define HAVE_GETCWD_DYN 1

#define PACKAGE "atf"
#define PACKAGE_NAME "Automated Testing Framework"
#define PACKAGE_VERSION "0.20"
#define PACKAGE_STRING "Automated Testing Framework 0.20"
#define PACKAGE_BUGREPORT "atf-devel@NetBSD.org"
#define PACKAGE_COPYRIGHT "Copyright (c) 2007-2012 The NetBSD Foundation, Inc."
#define PACKAGE_TARNAME "atf"
#define PACKAGE_URL "https://github.com/jmmv/atf/"
#define VERSION "0.20"

#endif /* !defined(RL_TARGET_BCONFIG_H) */
