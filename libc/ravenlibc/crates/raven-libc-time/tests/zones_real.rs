// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Real-zone differential: the Rust `localtime_r` against host glibc.
//!
//! Set `RAVEN_ZONES_DIR` to the directory produced by the ticket's `zic`
//! recipe (`slim`, `fat`, `right` and `zone1970.tab`). Without it the test
//! prints the NOT RUN line and passes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(clippy::undocumented_unsafe_blocks)]

use std::ffi::{CStr, c_char};
use std::path::{Path, PathBuf};

use raven_libc_time::tz::tzif::{TZLOAD_TZSTRING, tzload};
use raven_libc_time::tz::{TzState, convert::localtime_r};
use raven_libc_time::{ClockId, Errno, TimeSys, Timespec};

struct FileSys;

impl TimeSys for FileSys {
    fn clock_get(&self, _id: ClockId) -> Result<Timespec, Errno> {
        Err(Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
    fn nanosleep(&self, _ns: u64) -> Result<(), Errno> {
        Err(Errno(raven_libc_abi::errno::ENOSYS as u16))
    }
    fn read_file(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let path =
            std::str::from_utf8(path).map_err(|_| Errno(raven_libc_abi::errno::EINVAL as u16))?;
        let data = std::fs::read(path).map_err(|_| Errno(raven_libc_abi::errno::ENOENT as u16))?;
        if data.len() > buf.len() {
            return Err(Errno(raven_libc_abi::errno::E2BIG as u16));
        }
        buf.get_mut(..data.len())
            .ok_or(Errno(raven_libc_abi::errno::E2BIG as u16))?
            .copy_from_slice(&data);
        Ok(data.len())
    }
    fn issetugid(&self) -> bool {
        false
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GlibcTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const c_char,
}

impl GlibcTm {
    fn zeroed() -> Self {
        Self {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        }
    }
}

unsafe extern "C" {
    #[link_name = "localtime_r"]
    fn glibc_localtime_r(timep: *const i64, result: *mut GlibcTm) -> *mut GlibcTm;
    fn tzset();
}

fn zones(root: &Path) -> Vec<String> {
    let tab = std::fs::read_to_string(root.join("zone1970.tab")).expect("zone1970.tab");
    let mut result = vec!["UTC".to_string()];
    for line in tab.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if let Some(zone) = fields.get(2) {
            result.push((*zone).to_string());
        }
    }
    result.sort();
    result.dedup();
    result
}

fn instants(state: &TzState) -> Vec<i64> {
    const FIRST: i64 = -2_177_452_800; // 1901-01-01
    const LAST: i64 = 2_145_916_800; // 2038-01-01
    let mut values = Vec::new();
    while values.len() < 10 {
        let index = values.len();
        match state.ats.get(index) {
            Some(at) if *at >= FIRST && *at < LAST => {
                values.push(*at);
                if values.len() < 10 {
                    values.push(at.saturating_add(1));
                }
            }
            Some(_) => {
                // Use an evenly spaced fallback below.
                break;
            }
            None => break,
        }
    }
    let fallback = [
        -2_177_452_800i64,
        -1_577_836_800,
        -946_684_800,
        0,
        946_684_800,
        1_262_304_000,
        1_688_169_600,
        2_145_916_799,
    ];
    let mut index = 0usize;
    while values.len() < 10 {
        values.push(fallback[index % fallback.len()]);
        index += 1;
    }
    values
}

fn compare(zone: &str, t: i64, state: &TzState) {
    let ours = localtime_r(state, t).expect("rust localtime");
    // SAFETY: the test is single-threaded and has just set TZ/TZDIR.
    unsafe { tzset() };
    let mut theirs = GlibcTm::zeroed();
    // SAFETY: `localtime_r` writes one initialized `struct tm` into `theirs`;
    // the `TZ`/`TZDIR` environment is set by the single-threaded test below.
    let result = unsafe { glibc_localtime_r(&t, &mut theirs) };
    assert!(!result.is_null(), "{zone} {t}: glibc localtime_r failed");
    assert_eq!(ours.tm_sec, theirs.tm_sec, "{zone} {t} sec");
    assert_eq!(ours.tm_min, theirs.tm_min, "{zone} {t} min");
    assert_eq!(ours.tm_hour, theirs.tm_hour, "{zone} {t} hour");
    assert_eq!(ours.tm_mday, theirs.tm_mday, "{zone} {t} mday");
    assert_eq!(ours.tm_mon, theirs.tm_mon, "{zone} {t} mon");
    assert_eq!(ours.tm_year, theirs.tm_year, "{zone} {t} year");
    assert_eq!(ours.tm_wday, theirs.tm_wday, "{zone} {t} wday");
    assert_eq!(ours.tm_yday, theirs.tm_yday, "{zone} {t} yday");
    assert_eq!(ours.tm_isdst, theirs.tm_isdst, "{zone} {t} isdst");
    assert_eq!(ours.tm_gmtoff, theirs.tm_gmtoff, "{zone} {t} gmtoff");
    if !theirs.tm_zone.is_null() {
        // SAFETY: glibc returns a NUL-terminated string in the zone data.
        let expected = unsafe { CStr::from_ptr(theirs.tm_zone) }.to_bytes();
        assert_eq!(state.abbr(ours.tm_zone), expected, "{zone} {t} zone");
    }
}

fn run_tree(root: &Path, tree: &str) -> usize {
    let tree_path = root.join(tree);
    let zones = zones(root);
    let sys = FileSys;
    let mut cases = 0usize;
    for zone in &zones {
        let path: PathBuf = tree_path.join(zone);
        if !path.is_file() {
            continue;
        }
        let state = tzload(
            &sys,
            Some(path.to_string_lossy().as_bytes()),
            TZLOAD_TZSTRING,
        )
        .expect("tzload");
        // SAFETY: the test is single-threaded; glibc's environment is process-global.
        unsafe {
            std::env::set_var("TZDIR", tree_path.as_os_str());
            std::env::set_var("TZ", zone);
        }
        for t in instants(&state) {
            compare(zone, t, &state);
            cases += 1;
        }
    }
    cases
}

fn zone_runs() {
    let Some(root) = std::env::var_os("RAVEN_ZONES_DIR") else {
        println!("zones_real: NOT RUN (RAVEN_ZONES_DIR unset)");
        return;
    };
    let root = PathBuf::from(root);
    let slim = run_tree(&root, "slim");
    let fat = run_tree(&root, "fat");
    println!("zones_real: slim={slim} fat={fat}");
    assert!(slim >= 3000, "slim cases {slim}");
    assert!(fat >= 3000, "fat cases {fat}");
    println!("zones_real: ok");
}

#[test]
fn zones_real() {
    zone_runs();
}
