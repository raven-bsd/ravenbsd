/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/cpio.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _CPIO_H
#define _CPIO_H
#include <sys/cdefs.h>

#define MAGIC "070707"

#define C_IRUSR  000400
#define C_IWUSR  000200
#define C_IXUSR  000100
#define C_IRGRP  000040
#define C_IWGRP  000020
#define C_IXGRP  000010
#define C_IROTH  000004
#define C_IWOTH  000002
#define C_IXOTH  000001

#define C_ISUID  004000
#define C_ISGID  002000
#define C_ISVTX  001000

#define C_ISBLK  060000
#define C_ISCHR  020000
#define C_ISDIR  040000
#define C_ISFIFO 010000
#define C_ISSOCK 0140000
#define C_ISLNK  0120000
#define C_ISCTG  0110000
#define C_ISREG  0100000

#endif
