/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#ifndef FIXTURE_ILLUMOS_QUEUE_H
#define FIXTURE_ILLUMOS_QUEUE_H
#define STAILQ_HEAD(name, type) struct name { struct type *stqh_first; }
#endif
