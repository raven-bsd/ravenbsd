/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#ifndef FIXTURE_OBSD_QUEUE_H
#define FIXTURE_OBSD_QUEUE_H
#define SIMPLEQ_HEAD(name, type) struct name { struct type *sqh_first; }
#endif
