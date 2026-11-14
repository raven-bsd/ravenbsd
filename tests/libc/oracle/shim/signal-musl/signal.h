/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * Oracle shim for the musl sigset family: a 128-byte sigset_t and _NSIG 65, written from
 * scratch. The donor C files only need the bits array and the prototypes.
 */
#ifndef RAVEN_ORACLE_SIGNAL_MUSL_H
#define RAVEN_ORACLE_SIGNAL_MUSL_H

#include <stddef.h>

#define _NSIG 65

typedef struct {
	unsigned long __bits[128 / sizeof(long)];
} sigset_t;

int sigaddset(sigset_t *set, int sig);
int sigdelset(sigset_t *set, int sig);
int sigemptyset(sigset_t *set);
int sigfillset(sigset_t *set);
int sigismember(const sigset_t *set, int sig);
int sigisemptyset(const sigset_t *set);
int sigandset(sigset_t *dest, const sigset_t *left, const sigset_t *right);
int sigorset(sigset_t *dest, const sigset_t *left, const sigset_t *right);

#endif
