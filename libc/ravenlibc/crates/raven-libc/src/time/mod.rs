// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<time.h>` exports over `raven-libc-time` and the platform's `TimeSys` seam.
//!
//! The TZ environment hand-over and the process lock are C61's start path; this pass binds the
//! UTC-capable functions and the platform clocks. `gmtime`/`localtime` share the C89 static
//! result cell (single-threaded until C57's thread support lands).

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_void};
use core::sync::atomic::Ordering;

use raven_libc_abi::structs::Tm as AbiTm;
use raven_libc_time::tz::convert;
use raven_libc_time::{TzCache, TzState};

use crate::ffi::platform;
use crate::personality_common::Global;
use crate::time::sys::ShellTime;

pub(crate) mod sys;

static UTC_ZONE: [u8; 4] = *b"UTC\0";

struct StaticTm(UnsafeCell<AbiTm>);

// SAFETY: the shell is single-threaded until C57; the C89 result object is overwritten by the
// next `gmtime`/`localtime` call, as the C contract allows. C57 replaces this cell with TLS.
unsafe impl Sync for StaticTm {}

static STATIC_TM: StaticTm = StaticTm(UnsafeCell::new(AbiTm::ZERO));

fn static_tm(converted: &raven_libc_time::Tm) -> *mut AbiTm {
    let target = STATIC_TM.0.get();
    // SAFETY: no other reference is live for the duration of this call (single-threaded); the
    // cell is only written by the next conversion.
    unsafe {
        let value = &mut *target;
        fill_abi_tm(value, converted);
        value
    }
}

fn fill_abi_tm(out: &mut AbiTm, tm: &raven_libc_time::Tm) {
    out.tm_sec = tm.tm_sec;
    out.tm_min = tm.tm_min;
    out.tm_hour = tm.tm_hour;
    out.tm_mday = tm.tm_mday;
    out.tm_mon = tm.tm_mon;
    out.tm_year = tm.tm_year;
    out.tm_wday = tm.tm_wday;
    out.tm_yday = tm.tm_yday;
    out.tm_isdst = tm.tm_isdst;
    out.tm_gmtoff = tm.tm_gmtoff;
    out.tm_zone = UTC_ZONE.as_ptr().cast_mut().cast::<c_void>();
}

fn read_abi_tm(src: &AbiTm) -> raven_libc_time::Tm {
    raven_libc_time::Tm {
        tm_sec: src.tm_sec,
        tm_min: src.tm_min,
        tm_hour: src.tm_hour,
        tm_mday: src.tm_mday,
        tm_mon: src.tm_mon,
        tm_year: src.tm_year,
        tm_wday: src.tm_wday,
        tm_yday: src.tm_yday,
        tm_isdst: src.tm_isdst,
        tm_gmtoff: src.tm_gmtoff,
        tm_zone: raven_libc_time::tz::NO_ZONE,
    }
}

static TZ_CACHE: Global<Option<TzCache>> = Global::new(None);

fn tz_env<'a>() -> Option<&'a [u8]> {
    // SAFETY: `getenv` follows the C string contract; the returned environment string lives for
    // the process lifetime.
    let value = unsafe { crate::stdlib::getenv::getenv(b"TZ\0".as_ptr().cast::<c_char>()) };
    if value.is_null() {
        None
    } else {
        // SAFETY: `getenv` returned a NUL-terminated environment value.
        Some(unsafe { crate::ffi::cstr::slice(value) })
    }
}

/// Refresh the process TZ cache when the environment changed.
fn sync_tz_locked(cache: &mut TzCache) {
    let env = tz_env();
    if cache.needs_reload(env, 0) {
        cache.reload(&ShellTime, env, 0);
    }
    let names = cache.tzname();
    tzname::tzname[0].store(
        names[0].as_ptr().cast_mut().cast::<c_char>(),
        Ordering::Release,
    );
    tzname::tzname[1].store(
        names[1].as_ptr().cast_mut().cast::<c_char>(),
        Ordering::Release,
    );
    timezone::timezone.store(cache.timezone(), Ordering::Release);
    daylight::daylight.store(cache.daylight(), Ordering::Release);
}

/// A snapshot of the active local-time zone.
fn local_tz() -> TzState {
    TZ_CACHE.with(|slot| {
        let cache = slot.get_or_insert_with(TzCache::new);
        sync_tz_locked(cache);
        cache.state().clone()
    })
}

/// `tzset`.
#[unsafe(no_mangle)]
pub extern "C" fn tzset() {
    TZ_CACHE.with(|slot| {
        let cache = slot.get_or_insert_with(TzCache::new);
        sync_tz_locked(cache);
    });
}

mod daylight;
mod localtime;
mod nanosleep;
mod sleep;
mod strftime;
mod time;
mod timezone;
mod tzname;
