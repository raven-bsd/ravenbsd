// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<string.h>` exports (`02-api-surface` I19 I20 P33 X01 X02 X03 X04 X06 L10 B05).
//!
//! The hot list of `hot.toml` is written as a call-free loop in its own file (R-1/R-15); every
//! other body forwards to `raven_libc_str::raw`.

pub(crate) mod bcopy;
pub(crate) mod gnu_basename;
pub(crate) mod memccpy;
pub(crate) mod memchr;
pub(crate) mod memcmp;
pub(crate) mod memcpy;
pub(crate) mod memmem;
pub(crate) mod memmove;
pub(crate) mod mempcpy;
pub(crate) mod memrchr;
pub(crate) mod memset;
pub(crate) mod memset_s;
pub(crate) mod rawmemchr;
pub(crate) mod strcasecmp;
pub(crate) mod strcat;
pub(crate) mod strchr;
pub(crate) mod strcmp;
pub(crate) mod strcpy;
pub(crate) mod strdup;
pub(crate) mod strerror;
pub(crate) mod strlcpy;
pub(crate) mod strlen;
pub(crate) mod strmode;
pub(crate) mod strnlen;
pub(crate) mod strsep;
pub(crate) mod strsignal;
pub(crate) mod strspn;
pub(crate) mod strstr;
pub(crate) mod strtok;
pub(crate) mod strverscmp;
pub(crate) mod swab;
pub(crate) mod timingsafe_bcmp;
