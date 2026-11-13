/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement xlocale_private.h for the FreeBSD printf oracle, written from
 * scratch.  The locale object is opaque to the oracle; <locale.h> supplies the
 * numeric data (see the replacement locale.h).
 */
#ifndef RL_FMT_XLOCALE_PRIVATE_H
#define RL_FMT_XLOCALE_PRIVATE_H

#include <locale.h>
#include <stddef.h>
#include <wchar.h>

#define __get_locale() ((locale_t)0)
#define FIX_LOCALE(loc) ((void)0)
#ifndef MB_CUR_MAX
#define MB_CUR_MAX 4
#endif
#ifndef MB_LEN_MAX
#define MB_LEN_MAX 4
#endif

static inline int __strerror_rl(int errnum, char *buf, size_t buflen,
    locale_t loc)
{
	char tmp[32];
	size_t n = 0, i = 0, j;
	unsigned v;
	char digits[12];
	unsigned k = 0;

	(void)loc;
	if (buflen == 0)
		return (-1);
	tmp[n++] = 'e';
	tmp[n++] = 'r';
	tmp[n++] = 'r';
	tmp[n++] = 'n';
	tmp[n++] = 'o';
	tmp[n++] = ' ';
	if (errnum < 0 && n < sizeof(tmp))
		tmp[n++] = '-';
	v = (errnum < 0) ? (unsigned)(-errnum) : (unsigned)errnum;
	do {
		digits[k++] = (char)('0' + v % 10U);
		v /= 10U;
	} while (v != 0U);
	while (k != 0U && n < sizeof(tmp))
		tmp[n++] = digits[--k];
	for (j = 0; j < n && i + 1 < buflen; j++)
		buf[i++] = tmp[j];
	buf[i] = '\0';
	return (0);
}

#define mbsrtowcs_l(dst, src, len, ps, loc) mbsrtowcs((dst), (src), (len), (ps))
#define wcsrtombs_l(dst, src, len, ps, loc) wcsrtombs((dst), (src), (len), (ps))
#define mbrtowc_l(p, s, n, ps, loc) mbrtowc((p), (s), (n), (ps))
#define wcrtomb_l(s, wc, ps, loc) wcrtomb((s), (wc), (ps))
#define nl_langinfo_l(item, loc) nl_langinfo(item)

#endif
