// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Broad conversion matrix: one small test per documented case.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(non_snake_case)] // The conversion letters are the test names.

use raven_libc_time::locale::CLocale;
use raven_libc_time::strftime::{StrftimeCtx, strftime};
use raven_libc_time::strptime::{StrptimeCtx, strptime};
use raven_libc_time::tz::consts::{
    TM_YEAR_BASE, TZ_MAX_CHARS, TZ_MAX_LEAPS, TZ_MAX_TIMES, TZ_MAX_TYPES,
};
use raven_libc_time::tz::posix_tz::tzparse;
use raven_libc_time::tz::{NO_ZONE, Tm, TzState};
use raven_libc_time::{ClockId, TimeSys, Timespec};

macro_rules! tz_ok {
    ($name:ident, $text:expr) => {
        #[test]
        fn $name() {
            assert!(
                tzparse($text, None).is_ok(),
                "{}",
                String::from_utf8_lossy($text)
            );
        }
    };
}

macro_rules! tz_bad {
    ($name:ident, $text:expr) => {
        #[test]
        fn $name() {
            assert!(
                tzparse($text, None).is_err(),
                "{}",
                String::from_utf8_lossy($text)
            );
        }
    };
}

macro_rules! tz_offset {
    ($name:ident, $text:expr, $offset:expr) => {
        #[test]
        fn $name() {
            let state = tzparse($text, None).expect("TZ");
            assert_eq!(state.ttis[0].utoff, $offset);
        }
    };
}

tz_offset!(tz_utc, b"UTC0", 0);
tz_offset!(tz_gmt, b"GMT0", 0);
tz_offset!(tz_est, b"EST5", -5 * 3600);
tz_offset!(tz_cst, b"CST6", -6 * 3600);
tz_offset!(tz_mst, b"MST7", -7 * 3600);
tz_offset!(tz_pst, b"PST8", -8 * 3600);
tz_offset!(tz_jst, b"JST-9", 9 * 3600);
tz_offset!(tz_cet, b"CET-1", 3600);
tz_offset!(tz_half, b"<+0530>-5:30", 5 * 3600 + 30 * 60);
tz_offset!(tz_neg, b"UTC-5", 5 * 3600);
tz_offset!(tz_angle_plus, b"<+03>-3", 3 * 3600);
tz_offset!(tz_angle_minus, b"<-03>3", -3 * 3600);
tz_offset!(tz_hour_minute, b"XXX-2:30", 2 * 3600 + 30 * 60);
tz_offset!(
    tz_hour_minute_second,
    b"XXX-2:30:15",
    2 * 3600 + 30 * 60 + 15
);
tz_offset!(tz_zero, b"XXX0", 0);

tz_ok!(tz_est5edt, b"EST5EDT");
tz_ok!(tz_est5edt_rules, b"EST5EDT,M3.2.0,M11.1.0");
tz_ok!(tz_est5edt_explicit, b"EST5EDT4,M3.2.0,M11.1.0");
tz_ok!(tz_europe, b"CET-1CEST,M3.5.0,M10.5.0/3");
tz_ok!(tz_negative_rule, b"<-03>3<-02>,M3.5.0/-2,M10.5.0/-1");
tz_ok!(tz_julian, b"EST5EDT,J60,J300");
tz_ok!(tz_day_of_year, b"EST5EDT,59,300");
tz_ok!(tz_large_rule, b"EST5EDT,M3.2.0/167:59:59,M11.1.0/2:00");
tz_ok!(tz_lower_name, b"est5edt,M3.2.0,M11.1.0");
tz_ok!(tz_digits_name, b"<+03>-3<+04>,M3.2.0,M11.1.0");

tz_bad!(tz_bad_empty, b"");
tz_bad!(tz_bad_name_only, b"A");
tz_bad!(tz_bad_angle_empty, b"<>");
tz_bad!(tz_bad_no_offset, b"EST");
tz_bad!(tz_bad_month, b"EST5EDT,M13.1.0,M11.1.0");
tz_bad!(tz_bad_week, b"EST5EDT,M3.6.0,M11.1.0");
tz_bad!(tz_bad_day, b"EST5EDT,M3.1.7,M11.1.0");
tz_bad!(tz_bad_missing_end, b"EST5EDT,M3.2.0");
tz_bad!(tz_bad_extra, b"EST5EDT,M3.2.0,M11.1.0,x");
tz_bad!(tz_bad_offset, b"EST999");
tz_bad!(tz_bad_name_long, b"<AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA>0");

macro_rules! clock_id_case {
    ($name:ident, $id:expr, $mapped:expr) => {
        #[test]
        fn $name() {
            assert_eq!(raven_libc_time::clock::clock_id($id).is_some(), $mapped);
        }
    };
}

clock_id_case!(clock_0, 0, true);
clock_id_case!(clock_1, 1, true);
clock_id_case!(clock_2, 2, false);
clock_id_case!(clock_3, 3, false);
clock_id_case!(clock_4, 4, true);
clock_id_case!(clock_5, 5, true);
clock_id_case!(clock_6, 6, true);
clock_id_case!(clock_7, 7, true);
clock_id_case!(clock_8, 8, false);
clock_id_case!(clock_9, 9, false);
clock_id_case!(clock_10, 10, false);
clock_id_case!(clock_11, 11, false);
clock_id_case!(clock_negative, -1, false);
clock_id_case!(clock_large, 1000, false);

fn tm() -> Tm {
    Tm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 1,
        tm_mon: 0,
        tm_year: 70,
        tm_wday: 4,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: NO_ZONE,
    }
}

fn format(spec: &str) -> usize {
    let state = TzState::utc();
    let mut out = [0u8; 128];
    strftime(
        &state,
        &CLocale,
        &mut out,
        spec.as_bytes(),
        &tm(),
        &StrftimeCtx {
            tzname: [b"UTC", b"UTC"],
            localtime: None,
        },
    )
}

macro_rules! strftime_nonzero {
    ($name:ident, $spec:expr) => {
        #[test]
        fn $name() {
            assert!(format($spec) > 0, "{}", $spec);
        }
    };
}

strftime_nonzero!(strftime_a, "%a");
strftime_nonzero!(strftime_a_full, "%A");
strftime_nonzero!(strftime_b, "%b");
strftime_nonzero!(strftime_b_full, "%B");
strftime_nonzero!(strftime_c, "%c");
strftime_nonzero!(strftime_C, "%C");
strftime_nonzero!(strftime_d, "%d");
strftime_nonzero!(strftime_D, "%D");
strftime_nonzero!(strftime_e, "%e");
strftime_nonzero!(strftime_F, "%F");
strftime_nonzero!(strftime_g, "%g");
strftime_nonzero!(strftime_G, "%G");
strftime_nonzero!(strftime_h, "%h");
strftime_nonzero!(strftime_H, "%H");
strftime_nonzero!(strftime_I, "%I");
strftime_nonzero!(strftime_j, "%j");
strftime_nonzero!(strftime_k, "%k");
strftime_nonzero!(strftime_l, "%l");
strftime_nonzero!(strftime_m, "%m");
strftime_nonzero!(strftime_M, "%M");
strftime_nonzero!(strftime_n, "%n");
strftime_nonzero!(strftime_p, "%p");
strftime_nonzero!(strftime_P, "%P");
strftime_nonzero!(strftime_r, "%r");
strftime_nonzero!(strftime_R, "%R");
strftime_nonzero!(strftime_s, "%s");
strftime_nonzero!(strftime_S, "%S");
strftime_nonzero!(strftime_t, "%t");
strftime_nonzero!(strftime_T, "%T");
strftime_nonzero!(strftime_u, "%u");
strftime_nonzero!(strftime_U, "%U");
strftime_nonzero!(strftime_v, "%v");
strftime_nonzero!(strftime_V, "%V");
strftime_nonzero!(strftime_w, "%w");
strftime_nonzero!(strftime_W, "%W");
strftime_nonzero!(strftime_x, "%x");
strftime_nonzero!(strftime_X, "%X");
strftime_nonzero!(strftime_y, "%y");
strftime_nonzero!(strftime_Y, "%Y");
strftime_nonzero!(strftime_z, "%z");
strftime_nonzero!(strftime_Z, "%Z");
strftime_nonzero!(strftime_plus, "%+");
strftime_nonzero!(strftime_literal, "literal");
strftime_nonzero!(strftime_percent, "%%");

macro_rules! strptime_ok {
    ($name:ident, $buf:expr, $fmt:expr) => {
        #[test]
        fn $name() {
            let state = TzState::utc();
            let mut tm = tm();
            tm.tm_year = -1;
            let ctx = StrptimeCtx::default();
            assert!(
                strptime(
                    &state,
                    &CLocale,
                    $buf.as_bytes(),
                    $fmt.as_bytes(),
                    &mut tm,
                    &ctx
                )
                .is_some(),
                "{} / {}",
                $buf,
                $fmt
            );
        }
    };
}

strptime_ok!(strptime_date, "1970-01-01", "%Y-%m-%d");
strptime_ok!(strptime_time, "00:00:00", "%H:%M:%S");
strptime_ok!(strptime_day, "Thu", "%a");
strptime_ok!(strptime_day_full, "Thursday", "%A");
strptime_ok!(strptime_month, "Jan", "%b");
strptime_ok!(strptime_month_full, "January", "%B");
strptime_ok!(strptime_hour, "12", "%H");
strptime_ok!(strptime_hour12, "12", "%I");
strptime_ok!(strptime_minute, "59", "%M");
strptime_ok!(strptime_second, "59", "%S");
strptime_ok!(strptime_yday, "060", "%j");
strptime_ok!(strptime_weekday, "4", "%w");
strptime_ok!(strptime_weekday_iso, "4", "%u");
strptime_ok!(strptime_week_sun, "01", "%U");
strptime_ok!(strptime_week_mon, "01", "%W");
strptime_ok!(strptime_year_two, "70", "%y");
strptime_ok!(strptime_century, "19", "%C");
strptime_ok!(strptime_zone, "UTC", "%Z");
strptime_ok!(strptime_zone_numeric, "+0000", "%z");
strptime_ok!(strptime_zone_z, "Z", "%z");
strptime_ok!(strptime_epoch, "0", "%s");
strptime_ok!(strptime_pm, "PM", "%p");
strptime_ok!(strptime_ampm_alias, "PM", "%P");

macro_rules! timer_create_case {
    ($name:ident, $clock:expr) => {
        #[test]
        fn $name() {
            let mut table = raven_libc_time::timer::TimerTable::new(32);
            assert!(
                table
                    .create($clock, raven_libc_time::timer::Notify::None)
                    .is_ok()
            );
        }
    };
}

timer_create_case!(timer_create_realtime, 0);
timer_create_case!(timer_create_monotonic, 1);
timer_create_case!(timer_create_raw, 4);
timer_create_case!(timer_create_realtime_coarse, 5);
timer_create_case!(timer_create_monotonic_coarse, 6);
timer_create_case!(timer_create_boottime, 7);

#[test]
fn the_donor_limits_are_public() {
    assert_eq!(TZ_MAX_TIMES, 2000);
    assert_eq!(TZ_MAX_TYPES, 256);
    assert_eq!(TZ_MAX_CHARS, 256);
    assert_eq!(TZ_MAX_LEAPS, 50);
    assert_eq!(TM_YEAR_BASE, 1900);
}

#[test]
fn the_public_timespec_matches_the_seam() {
    let value = Timespec {
        tv_sec: 1,
        tv_nsec: 2,
    };
    assert_eq!(value.tv_sec, 1);
    assert_eq!(value.tv_nsec, 2);
}

#[test]
fn the_public_clock_enum_is_complete() {
    assert_ne!(ClockId::Realtime, ClockId::Monotonic);
}

#[test]
fn the_public_time_trait_is_object_safe() {
    fn takes(_sys: &dyn TimeSys) {}
    takes(&NoSys);
}

struct NoSys;

impl TimeSys for NoSys {
    fn clock_get(&self, _id: ClockId) -> Result<Timespec, raven_libc_time::Errno> {
        Err(raven_libc_time::Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
    fn nanosleep(&self, _ns: u64) -> Result<(), raven_libc_time::Errno> {
        Err(raven_libc_time::Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
    fn read_file(&self, _path: &[u8], _buf: &mut [u8]) -> Result<usize, raven_libc_time::Errno> {
        Err(raven_libc_time::Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
    fn issetugid(&self) -> bool {
        false
    }
}

macro_rules! strftime_flag {
    ($name:ident, $spec:expr) => {
        #[test]
        fn $name() {
            assert!(format($spec) > 0, "{}", $spec);
        }
    };
}

strftime_flag!(strftime_flag_dash_h, "%-H");
strftime_flag!(strftime_flag_underscore_h, "%_H");
strftime_flag!(strftime_flag_zero_h, "%0H");
strftime_flag!(strftime_flag_upper_a, "%^a");
strftime_flag!(strftime_flag_hash_p, "%#p");
strftime_flag!(strftime_flag_dash_j, "%-j");
strftime_flag!(strftime_flag_underscore_j, "%_j");
strftime_flag!(strftime_flag_zero_j, "%0j");

macro_rules! strptime_bad {
    ($name:ident, $buf:expr, $fmt:expr) => {
        #[test]
        fn $name() {
            let state = TzState::utc();
            let mut tm = tm();
            let ctx = StrptimeCtx::default();
            assert!(
                raven_libc_time::strptime::strptime(
                    &state,
                    &CLocale,
                    $buf.as_bytes(),
                    $fmt.as_bytes(),
                    &mut tm,
                    &ctx
                )
                .is_none(),
                "{} / {}",
                $buf,
                $fmt
            );
        }
    };
}

strptime_bad!(strptime_bad_hour, "24", "%H");
strptime_bad!(strptime_bad_month, "13", "%m");
strptime_bad!(strptime_bad_second, "62", "%S");
strptime_bad!(strptime_bad_literal, "12", "%H-%M");
strptime_bad!(strptime_bad_zone, "Bogus", "%z");

macro_rules! timer_arm_case {
    ($name:ident, $sec:expr, $nsec:expr) => {
        #[test]
        fn $name() {
            use raven_libc_time::timer::{ITimerSpec, Notify, Now, TimerTable};
            let mut table = TimerTable::new(32);
            let (id, _) = table.create(0, Notify::None).expect("create");
            let new = ITimerSpec {
                interval: Timespec::ZERO,
                value: Timespec {
                    tv_sec: $sec,
                    tv_nsec: $nsec,
                },
            };
            let now = Now {
                clock: Timespec::ZERO,
                mono_ns: 0,
            };
            assert!(table.settime(id, false, new, now).is_ok());
        }
    };
}

timer_arm_case!(timer_arm_one_ns, 0, 1);
timer_arm_case!(timer_arm_one_us, 0, 1_000);
timer_arm_case!(timer_arm_one_ms, 0, 1_000_000);
timer_arm_case!(timer_arm_one_sec, 1, 0);
timer_arm_case!(timer_arm_ten_sec, 10, 0);
