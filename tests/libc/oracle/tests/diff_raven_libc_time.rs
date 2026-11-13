// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential: `raven-libc-time` against the NetBSD tzcode originals.
//!
//! The family builds `localtime.c`, `asctime.c`, `difftime.c`, `strftime.c` and
//! `strptime.c` (see `recipes/raven-libc-time/oracle.toml`). The zone fixtures
//! are generated here as TZif v1-v4 files and handed to the C `tzalloc` and to
//! the Rust reader, so the test does not depend on the host zoneinfo tree.
//!
//! `tm_wday` is normalised for standalone POSIX `TZ` strings: the donor's
//! `timesub` narrows its (400-year-cycle) weekday to `int`, which overflows for
//! the `TIME_T_MIN`-based transition window that `tzparse` generates without a
//! TZif base. The C44 area file keeps `time_t` in `i64`, and glibc agrees with
//! the Rust value. TZif-backed zones (including the POSIX footer) compare the
//! weekday normally.

#![cfg(feature = "oracle-raven-libc-time")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(clippy::undocumented_unsafe_blocks)]

use std::ffi::{CStr, CString, c_char};
use std::sync::OnceLock;

use raven_libc_oracle::decls::raven_libc_time::{
    OracleTm, Timezone, oracle_asctime_r, oracle_difftime, oracle_gmtime_r, oracle_localtime_rz,
    oracle_mktime_z, oracle_strftime_z, oracle_strptime, oracle_timegm, oracle_timeoff,
    oracle_tzalloc, oracle_tzfree, oracle_tzset,
};
use raven_libc_oracle::diff_case;
use raven_libc_time::Errno;
use raven_libc_time::Timespec;
use raven_libc_time::locale::CLocale;
use raven_libc_time::strftime::{StrftimeCtx, strftime as rust_strftime};
use raven_libc_time::strptime::{StrptimeCtx, strptime as rust_strptime};
use raven_libc_time::tz::consts::TM_YEAR_BASE;
use raven_libc_time::tz::convert::{gmtime_r, localtime_r};
use raven_libc_time::tz::mktime::{
    mktime as rust_mktime, timegm as rust_timegm, timeoff as rust_timeoff,
};
use raven_libc_time::tz::posix_tz::tzparse;
use raven_libc_time::tz::tzif::{TZLOAD_TZSTRING, tzload};
use raven_libc_time::tz::{NO_ZONE, Tm, TzState, ZoneIdx};
use raven_libc_time::{ClockId, TimeSys};

// ---------------------------------------------------------------------------
// TZif fixtures

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn tzif_header(version: u8, timecnt: u32, typecnt: u32, charcnt: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"TZif");
    out.push(version);
    out.extend_from_slice(&[0u8; 15]);
    put_u32(&mut out, 0); // ttisutcnt
    put_u32(&mut out, 0); // ttisstdcnt
    put_u32(&mut out, 0); // leapcnt
    put_u32(&mut out, timecnt);
    put_u32(&mut out, typecnt);
    put_u32(&mut out, charcnt);
    out
}

fn tzif_block(
    times: &[i64],
    types: &[u8],
    ttis: &[(i32, u8, u8)],
    chars: &[u8],
    stored: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    for time in times {
        if stored == 4 {
            out.extend_from_slice(&(*time as i32).to_be_bytes());
        } else {
            out.extend_from_slice(&time.to_be_bytes());
        }
    }
    out.extend_from_slice(types);
    for (utoff, isdst, desig) in ttis {
        out.extend_from_slice(&utoff.to_be_bytes());
        out.push(*isdst);
        out.push(*desig);
    }
    out.extend_from_slice(chars);
    out
}

fn tzif_v1() -> Vec<u8> {
    let times = [-2_147_483_648i64, 0, 1_688_169_600, 2_145_916_800];
    let types = [0u8, 0, 1, 0];
    let ttis = [(-5 * 3600, 0u8, 0u8), (-4 * 3600, 1, 4)];
    let chars = b"EST\0EDT\0";
    let mut out = tzif_header(0, times.len() as u32, ttis.len() as u32, chars.len() as u32);
    out.extend_from_slice(&tzif_block(&times, &types, &ttis, chars, 4));
    out
}

fn tzif_v2plus(version: u8) -> Vec<u8> {
    let times = [-2_147_483_648i64, 0, 1_688_169_600, 2_145_916_800];
    let types = [0u8, 0, 1, 0];
    let ttis = [(-5 * 3600, 0u8, 0u8), (-4 * 3600, 1, 4)];
    let chars = b"EST\0EDT\0";
    let mut out = tzif_header(version, 0, 1, 4);
    out.extend_from_slice(&tzif_block(&[], &[], &[(0, 0, 0)], b"UTC\0", 4));
    out.extend_from_slice(&tzif_header(
        version,
        times.len() as u32,
        ttis.len() as u32,
        chars.len() as u32,
    ));
    out.extend_from_slice(&tzif_block(&times, &types, &ttis, chars, 8));
    out.extend_from_slice(b"\nEST5EDT,M3.2.0,M11.1.0\n");
    out
}

// ---------------------------------------------------------------------------
// Zone table: one C `timezone_t` and one Rust `TzState` per fixture.

struct FsSys;

impl TimeSys for FsSys {
    fn clock_get(&self, _id: ClockId) -> Result<Timespec, Errno> {
        Err(Errno(38))
    }

    fn nanosleep(&self, _ns: u64) -> Result<(), Errno> {
        Err(Errno(38))
    }

    fn read_file(&self, path: &[u8], buffer: &mut [u8]) -> Result<usize, Errno> {
        let path = std::str::from_utf8(path).map_err(|_| Errno(22))?;
        let data = std::fs::read(path).map_err(|_| Errno(2))?;
        if data.len() > buffer.len() {
            return Err(Errno(7));
        }
        buffer
            .get_mut(..data.len())
            .ok_or(Errno(7))?
            .copy_from_slice(&data);
        Ok(data.len())
    }

    fn issetugid(&self) -> bool {
        false
    }
}

struct Zone {
    name: &'static str,
    state: TzState,
    handle: Timezone,
    /// Whether the donor's `tm_wday` is exact for this fixture.
    wday_exact: bool,
}

// SAFETY: the C handle is an immutable `struct state` after `tzalloc`; the
// tzcode `_rz` entry points only read it and are called from one test at a time.
unsafe impl Send for Zone {}
unsafe impl Sync for Zone {}

impl Drop for Zone {
    fn drop(&mut self) {
        // SAFETY: the handle came from `tzalloc` and is freed exactly once.
        unsafe { oracle_tzfree(self.handle) };
    }
}

struct ZoneSet {
    zones: Vec<Zone>,
}

impl ZoneSet {
    fn zone(&self, name: &str) -> &Zone {
        self.zones
            .iter()
            .find(|zone| zone.name == name)
            .unwrap_or_else(|| panic!("unknown zone fixture {name}"))
    }
}

fn zones() -> &'static ZoneSet {
    static SET: OnceLock<ZoneSet> = OnceLock::new();
    SET.get_or_init(build_zones)
}

fn write_zone(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write TZif fixture");
}

fn c_handle(path: &std::path::Path) -> Timezone {
    let text = CString::new(format!(":{}", path.display())).expect("fixture path");
    // SAFETY: the path is a NUL-terminated string naming a generated regular file.
    let handle = unsafe { oracle_tzalloc(text.as_ptr()) };
    assert!(!handle.is_null(), "tzalloc {}", path.display());
    handle
}

fn rust_state(path: &std::path::Path) -> TzState {
    tzload(
        &FsSys,
        Some(path.to_string_lossy().as_bytes()),
        TZLOAD_TZSTRING,
    )
    .expect("Rust tzload")
}

fn posix_state(name: &str) -> TzState {
    tzparse(name.as_bytes(), None).expect("Rust tzparse")
}

fn posix_handle(name: &str) -> Timezone {
    let text = CString::new(name).expect("POSIX TZ string");
    // SAFETY: `name` is a C string with no interior NUL.
    let handle = unsafe { oracle_tzalloc(text.as_ptr()) };
    assert!(!handle.is_null(), "tzalloc {name}");
    handle
}

fn build_zones() -> ZoneSet {
    let dir = std::env::temp_dir().join(format!(
        "raven-libc-oracle-time-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("create zone fixture directory");

    let mut zones = Vec::new();
    for (name, bytes) in [
        ("v1", tzif_v1()),
        ("v2", tzif_v2plus(b'2')),
        ("v3", tzif_v2plus(b'3')),
        ("v4", tzif_v2plus(b'4')),
    ] {
        let path = dir.join(name);
        write_zone(&path, &bytes);
        let state = rust_state(&path);
        let handle = c_handle(&path);
        zones.push(Zone {
            name,
            state,
            handle,
            wday_exact: true,
        });
    }

    for name in [
        "UTC0",
        "EST5EDT,M3.2.0,M11.1.0",
        "<-03>3<-02>,M3.5.0/-2,M10.5.0/-1",
    ] {
        let state = posix_state(name);
        let handle = posix_handle(name);
        zones.push(Zone {
            name,
            state,
            handle,
            wday_exact: !name.contains(','),
        });
    }

    let _ = std::fs::remove_dir_all(&dir);

    // `strptime` reads the process-global `tzname`; keep it at UTC.
    // SAFETY: the table is initialised once and the process is single-threaded here.
    unsafe {
        std::env::set_var("TZ", "UTC0");
        oracle_tzset();
    }

    ZoneSet { zones }
}

fn zone(name: &str) -> &'static Zone {
    zones().zone(name)
}

// ---------------------------------------------------------------------------
// Observations

#[derive(Clone, Debug, PartialEq, Eq)]
struct TmObs {
    ok: bool,
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: Option<i32>,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: Option<Vec<u8>>,
}

impl TmObs {
    fn failed() -> Self {
        Self {
            ok: false,
            sec: 0,
            min: 0,
            hour: 0,
            mday: 0,
            mon: 0,
            year: 0,
            wday: None,
            yday: 0,
            isdst: 0,
            gmtoff: 0,
            zone: None,
        }
    }
}

fn oracle_tm_obs(tm: &OracleTm, wday: bool, zone: bool) -> TmObs {
    let zone = if zone && !tm.tm_zone.is_null() {
        // SAFETY: tzcode returns a NUL-terminated abbreviation pointer.
        Some(unsafe { CStr::from_ptr(tm.tm_zone) }.to_bytes().to_vec())
    } else {
        None
    };
    TmObs {
        ok: true,
        sec: tm.tm_sec,
        min: tm.tm_min,
        hour: tm.tm_hour,
        mday: tm.tm_mday,
        mon: tm.tm_mon,
        year: tm.tm_year,
        wday: wday.then_some(tm.tm_wday),
        yday: tm.tm_yday,
        isdst: tm.tm_isdst,
        gmtoff: tm.tm_gmtoff,
        zone,
    }
}

fn rust_tm_obs(state: Option<&TzState>, tm: &Tm, wday: bool, zone: bool) -> TmObs {
    let zone = if zone && tm.tm_zone != NO_ZONE {
        state.map(|state| state.abbr(tm.tm_zone).to_vec())
    } else {
        None
    };
    TmObs {
        ok: true,
        sec: tm.tm_sec,
        min: tm.tm_min,
        hour: tm.tm_hour,
        mday: tm.tm_mday,
        mon: tm.tm_mon,
        year: tm.tm_year,
        wday: wday.then_some(tm.tm_wday),
        yday: tm.tm_yday,
        isdst: tm.tm_isdst,
        gmtoff: tm.tm_gmtoff,
        zone,
    }
}

fn blank_oracle() -> OracleTm {
    OracleTm {
        tm_sec: -1,
        tm_min: -1,
        tm_hour: -1,
        tm_mday: -1,
        tm_mon: -1,
        tm_year: -1,
        tm_wday: -1,
        tm_yday: -1,
        tm_isdst: -1,
        tm_gmtoff: -1,
        tm_zone: std::ptr::null(),
    }
}

fn blank_tm() -> Tm {
    Tm {
        tm_sec: -1,
        tm_min: -1,
        tm_hour: -1,
        tm_mday: -1,
        tm_mon: -1,
        tm_year: -1,
        tm_wday: -1,
        tm_yday: -1,
        tm_isdst: -1,
        tm_gmtoff: -1,
        tm_zone: NO_ZONE,
    }
}

fn toml_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// localtime_r

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocaltimeCase {
    zone: String,
    t: i64,
}

impl LocaltimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        let names = [
            "v1",
            "v2",
            "v3",
            "v4",
            "UTC0",
            "EST5EDT,M3.2.0,M11.1.0",
            "<-03>3<-02>,M3.5.0/-2,M10.5.0/-1",
        ];
        let name = names[(rng.below(names.len() as u64) as usize).min(names.len() - 1)];
        let t = if rng.below(4) == 0 {
            let state = &zone(name).state;
            if state.ats.is_empty() {
                0
            } else {
                let index = rng.below(state.ats.len() as u64) as usize;
                state.ats[index.min(state.ats.len() - 1)] + (rng.below(7) as i64 - 3) * 86_400
            }
        } else {
            let raw = rng.next_u64() as i64;
            match rng.below(3) {
                0 => raw,
                1 => raw / 86_400 * 86_400,
                _ => raw / 1_000_000_000 * 1_000_000_000,
            }
        };
        Self {
            zone: name.to_string(),
            t,
        }
    }

    fn oracle(&self) -> TmObs {
        let zone = zone(&self.zone);
        let mut tm = blank_oracle();
        // SAFETY: `handle` is a live zone state and `t` is a valid pointer.
        let result = unsafe { oracle_localtime_rz(zone.handle, &self.t, &mut tm) };
        if result.is_null() {
            TmObs::failed()
        } else {
            oracle_tm_obs(&tm, zone.wday_exact, true)
        }
    }

    fn subject(&self) -> TmObs {
        let zone = zone(&self.zone);
        match localtime_r(&zone.state, self.t) {
            Ok(tm) => rust_tm_obs(Some(&zone.state), &tm, zone.wday_exact, true),
            Err(_) => TmObs::failed(),
        }
    }
}

impl raven_libc_oracle::diff::Case for LocaltimeCase {
    type Obs = TmObs;

    fn shrink(&self) -> Vec<Self> {
        vec![
            Self {
                zone: self.zone.clone(),
                t: self.t / 2,
            },
            Self {
                zone: "v2".to_string(),
                t: self.t,
            },
        ]
    }

    fn to_toml(&self) -> String {
        format!("zone = {}\nt = {}\n", toml_string(&self.zone), self.t)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// gmtime_r

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GmtimeCase {
    t: i64,
}

impl GmtimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        let raw = rng.next_u64() as i64;
        let t = match rng.below(4) {
            0 => raw,
            1 => raw / 86_400 * 86_400,
            2 => (raw / 1_000_000_000) * 1_000_000_000,
            _ => (raw % 4_000_000_000) - 2_000_000_000,
        };
        Self { t }
    }

    fn oracle(&self) -> TmObs {
        let mut tm = blank_oracle();
        // SAFETY: `tm` is a valid output buffer.
        let result = unsafe { oracle_gmtime_r(&self.t, &mut tm) };
        if result.is_null() {
            TmObs::failed()
        } else {
            oracle_tm_obs(&tm, true, false)
        }
    }

    fn subject(&self) -> TmObs {
        match gmtime_r(self.t) {
            Ok(tm) => rust_tm_obs(None, &tm, true, false),
            Err(_) => TmObs::failed(),
        }
    }
}

impl raven_libc_oracle::diff::Case for GmtimeCase {
    type Obs = TmObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self { t: self.t / 2 }]
    }

    fn to_toml(&self) -> String {
        format!("t = {}\n", self.t)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// mktime

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MktimeCase {
    zone: String,
    year: i32,
    mon: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
    isdst: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MktimeObs {
    ret: Option<i64>,
    tm: Option<TmObs>,
}

fn oracle_mktime_tm(case: &MktimeCase) -> OracleTm {
    OracleTm {
        tm_sec: case.sec,
        tm_min: case.min,
        tm_hour: case.hour,
        tm_mday: case.mday,
        tm_mon: case.mon,
        tm_year: case.year - TM_YEAR_BASE,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: case.isdst,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    }
}

fn rust_mktime_tm(case: &MktimeCase) -> Tm {
    Tm {
        tm_sec: case.sec,
        tm_min: case.min,
        tm_hour: case.hour,
        tm_mday: case.mday,
        tm_mon: case.mon,
        tm_year: case.year - TM_YEAR_BASE,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: case.isdst,
        tm_gmtoff: 0,
        tm_zone: NO_ZONE,
    }
}

impl MktimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        let names = ["v1", "v2", "v3", "v4", "UTC0", "EST5EDT,M3.2.0,M11.1.0"];
        let name = names[(rng.below(names.len() as u64) as usize).min(names.len() - 1)];
        Self {
            zone: name.to_string(),
            year: 1902 + rng.below(199) as i32,
            mon: rng.below(17) as i32 - 2,
            mday: rng.below(46) as i32 - 5,
            hour: rng.below(36) as i32 - 5,
            min: rng.below(141) as i32 - 70,
            sec: rng.below(141) as i32 - 70,
            isdst: rng.below(3) as i32 - 1,
        }
    }

    fn oracle(&self) -> MktimeObs {
        let zone = zone(&self.zone);
        let mut tm = oracle_mktime_tm(self);
        // SAFETY: `handle` is live and `tm` is a valid in/out buffer.
        let ret = unsafe { oracle_mktime_z(zone.handle, &mut tm) };
        if ret == -1 {
            MktimeObs {
                ret: None,
                tm: None,
            }
        } else {
            MktimeObs {
                ret: Some(ret),
                tm: Some(oracle_tm_obs(&tm, zone.wday_exact, true)),
            }
        }
    }

    fn subject(&self) -> MktimeObs {
        let zone = zone(&self.zone);
        let mut tm = rust_mktime_tm(self);
        match rust_mktime(&zone.state, &mut tm) {
            Ok(ret) => MktimeObs {
                ret: Some(ret),
                tm: Some(rust_tm_obs(Some(&zone.state), &tm, zone.wday_exact, true)),
            },
            Err(_) => MktimeObs {
                ret: None,
                tm: None,
            },
        }
    }
}

impl raven_libc_oracle::diff::Case for MktimeCase {
    type Obs = MktimeObs;

    fn shrink(&self) -> Vec<Self> {
        vec![
            Self {
                zone: self.zone.clone(),
                year: self.year,
                mon: 0,
                mday: 1,
                hour: 0,
                min: 0,
                sec: 0,
                isdst: self.isdst,
            },
            Self {
                zone: "v2".to_string(),
                year: 2023,
                mon: 6,
                mday: 1,
                hour: 12,
                min: 0,
                sec: 0,
                isdst: -1,
            },
        ]
    }

    fn to_toml(&self) -> String {
        format!(
            "zone = {}\nyear = {}\nmon = {}\nmday = {}\nhour = {}\nmin = {}\nsec = {}\nisdst = {}\n",
            toml_string(&self.zone),
            self.year,
            self.mon,
            self.mday,
            self.hour,
            self.min,
            self.sec,
            self.isdst
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// timegm and timeoff

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TimegmCase {
    year: i32,
    mon: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GmCaseObs {
    ret: i64,
    tm: TmObs,
}

impl TimegmCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        Self {
            year: 1902 + rng.below(199) as i32,
            mon: rng.below(17) as i32 - 2,
            mday: rng.below(46) as i32 - 5,
            hour: rng.below(36) as i32 - 5,
            min: rng.below(141) as i32 - 70,
            sec: rng.below(141) as i32 - 70,
        }
    }

    fn oracle_tm(&self) -> OracleTm {
        OracleTm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        }
    }

    fn rust_tm(&self) -> Tm {
        Tm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: NO_ZONE,
        }
    }

    fn oracle(&self) -> GmCaseObs {
        let mut tm = self.oracle_tm();
        // SAFETY: `tm` is a valid in/out buffer.
        let ret = unsafe { oracle_timegm(&mut tm) };
        GmCaseObs {
            ret,
            tm: oracle_tm_obs(&tm, true, false),
        }
    }

    fn subject(&self) -> GmCaseObs {
        let mut tm = self.rust_tm();
        let ret = rust_timegm(&mut tm).expect("timegm");
        GmCaseObs {
            ret,
            tm: rust_tm_obs(None, &tm, true, false),
        }
    }

    fn to_toml(&self) -> String {
        format!(
            "year = {}\nmon = {}\nmday = {}\nhour = {}\nmin = {}\nsec = {}\n",
            self.year, self.mon, self.mday, self.hour, self.min, self.sec
        )
    }
}

impl raven_libc_oracle::diff::Case for TimegmCase {
    type Obs = GmCaseObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self {
            year: self.year,
            mon: 0,
            mday: 1,
            hour: 0,
            min: 0,
            sec: 0,
        }]
    }

    fn to_toml(&self) -> String {
        TimegmCase::to_toml(self)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeoffCase {
    year: i32,
    mon: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
    offset: i64,
}

impl TimeoffCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        Self {
            year: 1902 + rng.below(199) as i32,
            mon: rng.below(17) as i32 - 2,
            mday: rng.below(46) as i32 - 5,
            hour: rng.below(36) as i32 - 5,
            min: rng.below(141) as i32 - 70,
            sec: rng.below(141) as i32 - 70,
            offset: rng.below(28 * 3600 + 1) as i64 - 14 * 3600,
        }
    }

    fn oracle(&self) -> GmCaseObs {
        let mut tm = OracleTm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        // SAFETY: `tm` is a valid in/out buffer and the offset is a plain long.
        let ret = unsafe { oracle_timeoff(&mut tm, self.offset) };
        GmCaseObs {
            ret,
            tm: oracle_tm_obs(&tm, true, false),
        }
    }

    fn subject(&self) -> GmCaseObs {
        let mut tm = Tm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: NO_ZONE,
        };
        let ret = rust_timeoff(&mut tm, self.offset).expect("timeoff");
        GmCaseObs {
            ret,
            tm: rust_tm_obs(None, &tm, true, false),
        }
    }

    fn to_toml(&self) -> String {
        format!(
            "year = {}\nmon = {}\nmday = {}\nhour = {}\nmin = {}\nsec = {}\noffset = {}\n",
            self.year, self.mon, self.mday, self.hour, self.min, self.sec, self.offset
        )
    }
}

impl raven_libc_oracle::diff::Case for TimeoffCase {
    type Obs = GmCaseObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self {
            year: self.year,
            mon: 0,
            mday: 1,
            hour: 0,
            min: 0,
            sec: 0,
            offset: 0,
        }]
    }

    fn to_toml(&self) -> String {
        TimeoffCase::to_toml(self)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// strftime

const FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%a %b %e %H:%M:%S %Y",
    "%A %B %d %Y",
    "%C %y %m %d",
    "%j %Y",
    "%U %W %V %G %g",
    "%u %w",
    "%I:%M:%S %p",
    "%r",
    "%R",
    "%T",
    "%D",
    "%F",
    "%v",
    "%+",
    "%n%t%%",
    "%z %Z",
    "%s",
    "%-d %_d %0d",
    "%k:%l",
    "%h %e",
    "%Q",
];

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrftimeCase {
    zone: String,
    year: i32,
    mon: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    format: String,
    size: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StrftimeObs {
    ret: usize,
    bytes: Vec<u8>,
}

fn zone_abbr(state: &TzState, isdst: i32) -> (&[u8], u16) {
    let want = isdst == 1;
    let info = state
        .ttis
        .iter()
        .find(|info| info.isdst == want)
        .or_else(|| state.ttis.first());
    match info {
        Some(info) => (
            state.abbr(ZoneIdx(u16::from(info.desigidx))),
            u16::from(info.desigidx),
        ),
        None => (b"", 0),
    }
}

impl StrftimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        let names = [
            "v1",
            "v2",
            "v3",
            "v4",
            "UTC0",
            "EST5EDT,M3.2.0,M11.1.0",
            "<-03>3<-02>,M3.5.0/-2,M10.5.0/-1",
        ];
        let name = names[(rng.below(names.len() as u64) as usize).min(names.len() - 1)];
        let format = if rng.below(4) == 0 {
            let index = rng.below(FORMATS.len() as u64) as usize;
            FORMATS[index.min(FORMATS.len() - 1)].to_string()
        } else {
            let first = rng.below(FORMATS.len() as u64) as usize;
            let second = rng.below(FORMATS.len() as u64) as usize;
            format!(
                "{} {}",
                FORMATS[first.min(FORMATS.len() - 1)],
                FORMATS[second.min(FORMATS.len() - 1)]
            )
        };
        Self {
            zone: name.to_string(),
            year: 1900 + rng.below(201) as i32,
            mon: rng.below(12) as i32,
            mday: 1 + rng.below(28) as i32,
            hour: rng.below(24) as i32,
            min: rng.below(60) as i32,
            sec: rng.below(60) as i32,
            wday: rng.below(7) as i32,
            yday: rng.below(366) as i32,
            isdst: rng.below(3) as i32 - 1,
            gmtoff: rng.below(28 * 3600 + 1) as i64 - 14 * 3600,
            format,
            size: 1 + rng.below(256) as usize,
        }
    }

    fn oracle_tm(&self, zone: &Zone) -> (OracleTm, CString) {
        let (abbr, _) = zone_abbr(&zone.state, self.isdst);
        let abbr = CString::new(abbr).expect("abbreviation has no NUL");
        let tm = OracleTm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: self.wday,
            tm_yday: self.yday,
            tm_isdst: self.isdst,
            tm_gmtoff: self.gmtoff,
            tm_zone: abbr.as_ptr(),
        };
        (tm, abbr)
    }

    fn rust_tm(&self, zone: &Zone) -> Tm {
        let (_, desig) = zone_abbr(&zone.state, self.isdst);
        Tm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: self.wday,
            tm_yday: self.yday,
            tm_isdst: self.isdst,
            tm_gmtoff: self.gmtoff,
            tm_zone: ZoneIdx(desig),
        }
    }

    fn oracle(&self) -> StrftimeObs {
        let zone = zone(&self.zone);
        let (tm, _abbr) = self.oracle_tm(zone);
        let format = CString::new(self.format.clone()).expect("format has no NUL");
        let mut buffer = vec![0xA5u8; self.size.saturating_add(8)];
        // SAFETY: the handle is live, both strings are NUL-terminated and the
        // buffer has `size + 8` writable bytes (the last eight are guards).
        let ret = unsafe {
            oracle_strftime_z(
                zone.handle,
                buffer.as_mut_ptr() as *mut c_char,
                self.size,
                format.as_ptr(),
                &tm,
            )
        };
        StrftimeObs {
            ret,
            bytes: if ret == 0 { Vec::new() } else { buffer },
        }
    }

    fn subject(&self) -> StrftimeObs {
        let zone = zone(&self.zone);
        let tm = self.rust_tm(zone);
        let ctx = StrftimeCtx {
            tzname: [b"", b""],
            localtime: None,
        };
        let mut buffer = vec![0xA5u8; self.size.saturating_add(8)];
        let ret = rust_strftime(
            &zone.state,
            &CLocale,
            buffer.get_mut(..self.size).unwrap_or(&mut []),
            self.format.as_bytes(),
            &tm,
            &ctx,
        );
        StrftimeObs {
            ret,
            bytes: if ret == 0 { Vec::new() } else { buffer },
        }
    }

    fn to_toml(&self) -> String {
        format!(
            "zone = {}\nyear = {}\nmon = {}\nmday = {}\nhour = {}\nmin = {}\nsec = {}\nwday = {}\nyday = {}\nisdst = {}\ngmtoff = {}\nformat = {}\nsize = {}\n",
            toml_string(&self.zone),
            self.year,
            self.mon,
            self.mday,
            self.hour,
            self.min,
            self.sec,
            self.wday,
            self.yday,
            self.isdst,
            self.gmtoff,
            toml_string(&self.format),
            self.size
        )
    }
}

impl raven_libc_oracle::diff::Case for StrftimeCase {
    type Obs = StrftimeObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self {
            zone: "UTC0".to_string(),
            year: 1970,
            mon: 0,
            mday: 1,
            hour: 0,
            min: 0,
            sec: 0,
            wday: 4,
            yday: 0,
            isdst: 0,
            gmtoff: 0,
            format: "%Y".to_string(),
            size: self.size,
        }]
    }

    fn to_toml(&self) -> String {
        StrftimeCase::to_toml(self)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// strptime

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrptimeCase {
    text: String,
    format: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StrptimeObs {
    ok: bool,
    tail: Vec<u8>,
    tm: Option<TmObs>,
}

const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const ABDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const ABMONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn is_leap(year: i32) -> bool {
    let year = i64::from(year);
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn first_wday_of(year: i32) -> i32 {
    let year = i64::from(year);
    let value = 1 + 5 * ((year - 1) % 4) + 4 * ((year - 1) % 100) + 6 * ((year - 1) % 400);
    (value % 7) as i32
}

fn week_yday(year: i32, week: i32, wday: i32, day_offset: i32) -> i32 {
    (7 - first_wday_of(year) + day_offset) % 7 + (week - 1) * 7 + wday - day_offset
}

fn strptime_case(rng: &mut raven_libc_oracle::rng::Rng) -> StrptimeCase {
    let year = 1902 + rng.below(199) as i32;
    let mon = rng.below(12) as usize;
    let mday = 1 + rng.below(28) as i32;
    let hour = rng.below(24) as i32;
    let min = rng.below(60) as i32;
    let sec = rng.below(60) as i32;
    let kind = rng.below(12);
    let (text, format) = match kind {
        0 => (
            format!(
                "{year:04}-{:02}-{mday:02} {hour:02}:{min:02}:{sec:02}",
                mon + 1
            ),
            "%Y-%m-%d %H:%M:%S".to_string(),
        ),
        1 => (
            format!(
                "{} {} {mday:2} {hour:02}:{min:02}:{sec:02} {year:04}",
                ABDAYS[rng.below(7) as usize],
                ABMONTHS[mon]
            ),
            "%a %b %e %H:%M:%S %Y".to_string(),
        ),
        2 => (
            format!(
                "{:02}/{:02}/{:02} {:02}:{min:02} {}",
                mday,
                mon + 1,
                year % 100,
                1 + rng.below(12) as i32,
                if rng.below(2) == 0 { "AM" } else { "PM" }
            ),
            "%d/%m/%y %I:%M %p".to_string(),
        ),
        3 => {
            // Day 366 in a non-leap year makes the donor's month scan read
            // start_of_month out of bounds; keep the defined cells only.
            let last = if is_leap(year) { 366 } else { 365 };
            (
                format!("{:03} {year:04}", 1 + rng.below(last) as i32),
                "%j %Y".to_string(),
            )
        }
        4 => {
            // The donor's month scan indexes start_of_month out of bounds when
            // the finalised yday is negative or above 364. Keep only the
            // defined cells; the UB reproducers are Rust-only regressions.
            let (week, wday) = loop {
                let week = rng.below(54) as i32;
                let wday = rng.below(7) as i32;
                let yday = week_yday(year, week, wday, 0);
                if (0..=364).contains(&yday) {
                    break (week, wday);
                }
            };
            (
                format!("{week:02} {wday} {year:04}"),
                "%U %w %Y".to_string(),
            )
        }
        5 => (
            format!("{}", rng.below(2_147_483_647) as i64),
            "%s".to_string(),
        ),
        6 => (
            format!(
                "{year:04}-{:02}-{mday:02}T{hour:02}:{min:02}:{sec:02}",
                mon + 1
            ),
            "%FT%T".to_string(),
        ),
        7 => (
            format!(
                "{:02}/{:02}/{:02} {:02}:{min:02}:{sec:02} {}",
                mday,
                mon + 1,
                year % 100,
                1 + rng.below(12) as i32,
                if rng.below(2) == 0 { "AM" } else { "PM" }
            ),
            "%D %r".to_string(),
        ),
        8 => (
            format!("{year:04}{:02}{mday:02}{hour:02}{min:02}{sec:02}", mon + 1),
            "%Y%m%d%H%M%S".to_string(),
        ),
        9 => (
            format!(
                "{}{hour:02}{min:02} {year:04}-{:02}-{mday:02}",
                if rng.below(2) == 0 { '+' } else { '-' },
                mon + 1
            ),
            "%z %Y-%m-%d".to_string(),
        ),
        10 => (
            format!("UTC {year:04}-{:02}-{mday:02}", mon + 1),
            "%Z %Y-%m-%d".to_string(),
        ),
        _ => (
            format!(
                "{} {} {mday:02} {year:04}",
                DAYS[rng.below(7) as usize],
                MONTHS[mon]
            ),
            "%A %B %d %Y".to_string(),
        ),
    };
    StrptimeCase { text, format }
}

impl StrptimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        strptime_case(rng)
    }

    fn oracle(&self) -> StrptimeObs {
        let text = CString::new(self.text.clone()).expect("text has no NUL");
        let format = CString::new(self.format.clone()).expect("format has no NUL");
        let mut tm = blank_oracle();
        // SAFETY: both strings are NUL-terminated and `tm` is a valid output buffer.
        let rest = unsafe { oracle_strptime(text.as_ptr(), format.as_ptr(), &mut tm) };
        if rest.is_null() {
            StrptimeObs {
                ok: false,
                tail: Vec::new(),
                tm: None,
            }
        } else {
            // SAFETY: the returned pointer is inside `text`.
            let tail = unsafe { CStr::from_ptr(rest) }.to_bytes().to_vec();
            StrptimeObs {
                ok: true,
                tail,
                tm: Some(oracle_tm_obs(&tm, true, false)),
            }
        }
    }

    fn subject(&self) -> StrptimeObs {
        let state = zone("UTC0");
        let mut tm = blank_tm();
        let ctx = StrptimeCtx {
            tzname: [b"UTC", b"UTC"],
            localtime: None,
        };
        match rust_strptime(
            &state.state,
            &CLocale,
            self.text.as_bytes(),
            self.format.as_bytes(),
            &mut tm,
            &ctx,
        ) {
            Some(rest) => StrptimeObs {
                ok: true,
                tail: rest.to_vec(),
                tm: Some(rust_tm_obs(None, &tm, true, false)),
            },
            None => StrptimeObs {
                ok: false,
                tail: Vec::new(),
                tm: None,
            },
        }
    }

    fn to_toml(&self) -> String {
        format!(
            "text = {}\nformat = {}\n",
            toml_string(&self.text),
            toml_string(&self.format)
        )
    }
}

impl raven_libc_oracle::diff::Case for StrptimeCase {
    type Obs = StrptimeObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self {
            text: self.text.chars().take(self.text.len() / 2).collect(),
            format: "%Y".to_string(),
        }]
    }

    fn to_toml(&self) -> String {
        StrptimeCase::to_toml(self)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// asctime_r

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AsctimeCase {
    year: i32,
    mon: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AsctimeObs {
    ok: bool,
    bytes: [u8; 26],
}

impl AsctimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        Self {
            year: 1900 + rng.below(200) as i32,
            mon: rng.below(12) as i32,
            mday: 1 + rng.below(31) as i32,
            hour: rng.below(24) as i32,
            min: rng.below(60) as i32,
            sec: rng.below(61) as i32,
            wday: rng.below(7) as i32,
            yday: rng.below(366) as i32,
            isdst: rng.below(2) as i32,
            gmtoff: 0,
        }
    }

    fn oracle_tm(&self) -> OracleTm {
        OracleTm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: self.wday,
            tm_yday: self.yday,
            tm_isdst: self.isdst,
            tm_gmtoff: self.gmtoff,
            tm_zone: std::ptr::null(),
        }
    }

    fn rust_tm(&self) -> Tm {
        Tm {
            tm_sec: self.sec,
            tm_min: self.min,
            tm_hour: self.hour,
            tm_mday: self.mday,
            tm_mon: self.mon,
            tm_year: self.year - TM_YEAR_BASE,
            tm_wday: self.wday,
            tm_yday: self.yday,
            tm_isdst: self.isdst,
            tm_gmtoff: self.gmtoff,
            tm_zone: NO_ZONE,
        }
    }

    fn oracle(&self) -> AsctimeObs {
        let tm = self.oracle_tm();
        let mut bytes = [0i8; 26];
        // SAFETY: `tm` is a complete struct and `bytes` is 26 writable bytes.
        let result = unsafe { oracle_asctime_r(&tm, bytes.as_mut_ptr() as *mut c_char) };
        AsctimeObs {
            ok: !result.is_null(),
            bytes: bytes.map(|byte| byte as u8),
        }
    }

    fn subject(&self) -> AsctimeObs {
        let tm = self.rust_tm();
        let mut bytes = [0u8; 26];
        AsctimeObs {
            ok: raven_libc_time::asctime::asctime_r(&tm, &mut bytes).is_ok(),
            bytes,
        }
    }

    fn to_toml(&self) -> String {
        format!(
            "year = {}\nmon = {}\nmday = {}\nhour = {}\nmin = {}\nsec = {}\nwday = {}\nyday = {}\nisdst = {}\ngmtoff = {}\n",
            self.year,
            self.mon,
            self.mday,
            self.hour,
            self.min,
            self.sec,
            self.wday,
            self.yday,
            self.isdst,
            self.gmtoff
        )
    }
}

impl raven_libc_oracle::diff::Case for AsctimeCase {
    type Obs = AsctimeObs;

    fn shrink(&self) -> Vec<Self> {
        vec![Self {
            year: 1970,
            mon: 0,
            mday: 1,
            hour: 0,
            min: 0,
            sec: 0,
            wday: 4,
            yday: 0,
            isdst: 0,
            gmtoff: 0,
        }]
    }

    fn to_toml(&self) -> String {
        AsctimeCase::to_toml(self)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

// ---------------------------------------------------------------------------
// difftime

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DifftimeCase {
    t1: i64,
    t0: i64,
}

impl DifftimeCase {
    fn random(rng: &mut raven_libc_oracle::rng::Rng) -> Self {
        Self {
            t1: rng.next_u64() as i64,
            t0: rng.next_u64() as i64,
        }
    }

    fn oracle(&self) -> u64 {
        // SAFETY: a pure arithmetic function.
        unsafe { oracle_difftime(self.t1, self.t0) }.to_bits()
    }

    fn subject(&self) -> u64 {
        raven_libc_time::difftime::difftime(self.t1, self.t0).to_bits()
    }
}

impl raven_libc_oracle::diff::Case for DifftimeCase {
    type Obs = u64;

    fn to_toml(&self) -> String {
        format!("t1 = {}\nt0 = {}\n", self.t1, self.t0)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:016x}")
    }
}

// ---------------------------------------------------------------------------
// Empty exhaustive domains (the random classes carry the 2M-case volume).

fn empty_localtime() -> Box<dyn Iterator<Item = LocaltimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_gmtime() -> Box<dyn Iterator<Item = GmtimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_mktime() -> Box<dyn Iterator<Item = MktimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_timegm() -> Box<dyn Iterator<Item = TimegmCase>> {
    Box::new(std::iter::empty())
}

fn empty_timeoff() -> Box<dyn Iterator<Item = TimeoffCase>> {
    Box::new(std::iter::empty())
}

fn empty_strftime() -> Box<dyn Iterator<Item = StrftimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_strptime() -> Box<dyn Iterator<Item = StrptimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_asctime() -> Box<dyn Iterator<Item = AsctimeCase>> {
    Box::new(std::iter::empty())
}

fn empty_difftime() -> Box<dyn Iterator<Item = DifftimeCase>> {
    Box::new(std::iter::empty())
}

// ---------------------------------------------------------------------------
// Differential tests

diff_case! {
    name: localtime,
    case: LocaltimeCase,
    fixed: "raven-libc-time/localtime/basic.toml",
    random: LocaltimeCase::random,
    exhaustive: (small, empty_localtime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: localtime_posix,
    case: LocaltimeCase,
    fixed: "raven-libc-time/localtime_posix/basic.toml",
    random: LocaltimeCase::random,
    exhaustive: (small, empty_localtime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: gmtime,
    case: GmtimeCase,
    fixed: "raven-libc-time/gmtime/basic.toml",
    random: GmtimeCase::random,
    exhaustive: (small, empty_gmtime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: mktime,
    case: MktimeCase,
    fixed: "raven-libc-time/mktime/basic.toml",
    random: MktimeCase::random,
    exhaustive: (small, empty_mktime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: timegm,
    case: TimegmCase,
    fixed: "raven-libc-time/timegm/basic.toml",
    random: TimegmCase::random,
    exhaustive: (small, empty_timegm),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: timeoff,
    case: TimeoffCase,
    fixed: "raven-libc-time/timeoff/basic.toml",
    random: TimeoffCase::random,
    exhaustive: (small, empty_timeoff),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: strftime,
    case: StrftimeCase,
    fixed: "raven-libc-time/strftime/basic.toml",
    random: StrftimeCase::random,
    exhaustive: (small, empty_strftime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: strptime,
    case: StrptimeCase,
    fixed: "raven-libc-time/strptime/basic.toml",
    random: StrptimeCase::random,
    exhaustive: (small, empty_strptime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: asctime,
    case: AsctimeCase,
    fixed: "raven-libc-time/asctime/basic.toml",
    random: AsctimeCase::random,
    exhaustive: (small, empty_asctime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}

diff_case! {
    name: difftime,
    case: DifftimeCase,
    fixed: "raven-libc-time/difftime/basic.toml",
    random: DifftimeCase::random,
    exhaustive: (small, empty_difftime),
    oracle: |case| case.oracle(),
    subject: |case| case.subject(),
}
