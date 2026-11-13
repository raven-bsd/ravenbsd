/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#ifndef FIXTURE_NBSD_QUEUE_H
#define FIXTURE_NBSD_QUEUE_H
#define LIST_HEAD(name, type) struct name { struct type *lh_first; }
#endif
