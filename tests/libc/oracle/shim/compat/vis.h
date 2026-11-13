/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD: the installed vis.h prototype set for the C65 compat-vis oracle (first-party). */
#ifndef _VIS_H_
#define _VIS_H_
#include <sys/types.h>
#define VIS_OCTAL 0x0001
#define VIS_CSTYLE 0x0002
#define VIS_SP 0x0004
#define VIS_TAB 0x0008
#define VIS_NL 0x0010
#define VIS_WHITE (VIS_SP | VIS_TAB | VIS_NL)
#define VIS_SAFE 0x0020
#define VIS_DQ 0x8000
#define VIS_NOSLASH 0x0040
#define VIS_HTTP1808 0x0080
#define VIS_HTTPSTYLE 0x0080
#define VIS_MIMESTYLE 0x0100
#define VIS_HTTP1866 0x0200
#define VIS_NOESCAPE 0x0400
#define _VIS_END 0x0800
#define VIS_GLOB 0x1000
#define VIS_SHELL 0x2000
#define VIS_META (VIS_WHITE | VIS_GLOB | VIS_SHELL)
#define VIS_NOLOCALE 0x4000
#define UNVIS_VALID 1
#define UNVIS_VALIDPUSH 2
#define UNVIS_NOCHAR 3
#define UNVIS_SYNBAD (-1)
#define UNVIS_ERROR (-2)
#define UNVIS_END _VIS_END
#ifdef __cplusplus
extern "C" {
#endif
char *vis(char *, int, int, int);
char *nvis(char *, size_t, int, int, int);
char *svis(char *, int, int, int, const char *);
char *snvis(char *, size_t, int, int, int, const char *);
int strvis(char *, const char *, int);
int stravis(char **, const char *, int);
int strnvis(char *, size_t, const char *, int);
int strsvis(char *, const char *, int, const char *);
int strsnvis(char *, size_t, const char *, int, const char *);
int strvisx(char *, const char *, size_t, int);
int strnvisx(char *, size_t, const char *, size_t, int);
int strenvisx(char *, size_t, const char *, size_t, int, int *);
int strsvisx(char *, const char *, size_t, int, const char *);
int strsnvisx(char *, size_t, const char *, size_t, int, const char *);
int strsenvisx(char *, size_t, const char *, size_t, int, const char *, int *);
int strunvis(char *, const char *);
int strnunvis(char *, size_t, const char *);
int strunvisx(char *, const char *, int);
int strnunvisx(char *, size_t, const char *, int);
int unvis(char *, int, int *, int);
#ifdef __cplusplus
}
#endif
#endif
