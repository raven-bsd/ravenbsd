/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#ifndef FIXTURE_FBSD_QUEUE_H
#define FIXTURE_FBSD_QUEUE_H
#define TAILQ_HEAD(name, type) struct name { struct type *tqh_first; }
#endif
