/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD oracle shim: the musl multibyte files include "locale_impl.h" only for MB_CUR_MAX.
 * This header gives them a settable process-wide value; the weak definition is emitted by every
 * translation unit and merged by the linker into one exported symbol (prefixed `oracle_` too).
 */
#ifndef RAVEN_ORACLE_LOCALE_IMPL_H
#define RAVEN_ORACLE_LOCALE_IMPL_H

#undef MB_CUR_MAX

#ifdef __cplusplus
extern "C" {
#endif
__attribute__((weak)) int mb_cur_max = 4;
#ifdef __cplusplus
}
#endif

#define MB_CUR_MAX mb_cur_max

#endif
