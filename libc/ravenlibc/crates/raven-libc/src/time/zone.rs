// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process time-zone cache and the POSIX `tzname`/`timezone`/`daylight` data symbols.
//!
//! `raven-libc-time` owns the TZif/POSIX parser and the reentrant cache; the shell reads
//! `TZ`, reloads the cache and publishes the C globals. The shell is single-threaded until
//! C57, so the cache and the three data symbols live in plain `static mut` storage.

use core::ffi::c_char;

use raven_libc_time::TzCache;

use super::sys::ShellTime;

/// The abbreviation used by `gmtime` and by a `strftime` on a gmtime result.
static GMT_BYTES: [u8; 4] = *b"GMT\0";

static mut CACHE: Option<TzCache> = None;

fn cache() -> &'static mut TzCache {
    // SAFETY: one process cache; the shell is single-threaded until C57.
    unsafe { (&mut *(&raw mut CACHE)).get_or_insert_with(TzCache::new) }
}

fn c_string(name: &[u8]) -> *mut c_char {
    if name.is_empty() {
        GMT_BYTES.as_ptr().cast_mut().cast::<c_char>()
    } else {
        name.as_ptr().cast_mut().cast::<c_char>()
    }
}

fn publish(cache: &TzCache) {
    let names = cache.tzname();
    let standard = names.first().copied().unwrap_or(b"GMT");
    let daylight_name = names.get(1).copied().unwrap_or(standard);
    super::tzname::publish([c_string(standard), c_string(daylight_name)]);
    super::timezone::publish(cache.timezone());
    super::daylight::publish(cache.daylight());
}

/// Reload the cache unconditionally for `tzset`.
pub(crate) fn reload() {
    let tz = crate::stdlib::env_support::get(b"TZ");
    let cache = cache();
    cache.reload(&ShellTime, tz, 0);
    publish(cache);
}

/// Reload the cache when `TZ` changed since the last load.
pub(crate) fn ensure() {
    let tz = crate::stdlib::env_support::get(b"TZ");
    let cache = cache();
    if cache.needs_reload(tz, 0) {
        cache.reload(&ShellTime, tz, 0);
        publish(cache);
    }
}

/// Run `f` with the current cache after the lazy `TZ` check.
pub(crate) fn with_cache<R>(f: impl FnOnce(&TzCache) -> R) -> R {
    ensure();
    f(cache())
}

/// The `tm_zone` pointer for the current zone and DST state.
pub(crate) fn abbreviation(cache: &TzCache, isdst: bool) -> *mut c_char {
    let names = cache.tzname();
    let index = if isdst { 1 } else { 0 };
    c_string(names.get(index).copied().unwrap_or(b"GMT"))
}

/// The `GMT` pointer used by `gmtime` results.
pub(crate) fn gmt_pointer() -> *mut c_char {
    GMT_BYTES.as_ptr().cast_mut().cast::<c_char>()
}
