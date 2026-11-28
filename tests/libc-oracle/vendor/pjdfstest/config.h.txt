/* SPDX-License-Identifier: BSD-2-Clause */
/*
 * Minimal pjdfstest configuration for RavenBSD.
 *
 * The upstream configure.ac checks optional POSIX interfaces; the open/00.t action set needs
 * only the base entries, so no HAVE_* macro is defined. NDEBUG is supplied by the build because
 * ravenlibc declares __assert_fail but does not export it; the helper's two internal asserts are
 * not on the open/create path.
 */
#define _GNU_SOURCE 1
