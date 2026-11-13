// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The table test: every row of C59 section 4.6 is instantiated with the test prefix `"t_"`,
//! and a Rust call with the row's canned argument list shows that the recorder target
//! received the named arguments and a reader positioned at argument `named`.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/table_ut.rs"]
mod table;
#[path = "../src/targets.rs"]
mod targets;

use std::sync::{Mutex, MutexGuard};

use targets::{reset_records, take_records};

/// The recorder is a process-wide store; the row tests must not interleave.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|poison| poison.into_inner())
}

macro_rules! decl1 {
    ($name:ident) => {
        unsafe extern "C" {
            fn $name(a0: usize, ...) -> i32;
        }
    };
}
macro_rules! decl2 {
    ($name:ident) => {
        unsafe extern "C" {
            fn $name(a0: usize, a1: usize, ...) -> i32;
        }
    };
}
macro_rules! decl3 {
    ($name:ident) => {
        unsafe extern "C" {
            fn $name(a0: usize, a1: usize, a2: usize, ...) -> i32;
        }
    };
}
macro_rules! decl4 {
    ($name:ident) => {
        unsafe extern "C" {
            fn $name(a0: usize, a1: usize, a2: usize, a3: usize, ...) -> i32;
        }
    };
}

decl1!(t_printf);
decl2!(t_fprintf);
decl2!(t_sprintf);
decl3!(t_snprintf);
decl2!(t_dprintf);
decl2!(t_asprintf);
decl1!(t_scanf);
decl2!(t_fscanf);
decl2!(t_sscanf);
decl1!(t_wprintf);
decl2!(t_fwprintf);
decl3!(t_swprintf);
decl1!(t_wscanf);
decl2!(t_fwscanf);
decl2!(t_swscanf);
decl3!(t_snprintf_ss);
decl2!(t_err);
decl2!(t_errx);
decl1!(t_warn);
decl1!(t_warnx);
decl3!(t_errc);
decl2!(t_warnc);
decl2!(t_syslog);
decl2!(t_open);
decl3!(t_openat);
decl2!(t_fcntl);
decl2!(t_ioctl);
decl2!(t_execl);
decl2!(t_execle);
decl2!(t_execlp);
decl3!(t_semctl);
decl1!(t_prctl);
decl1!(t_ptrace);
decl2!(t_mq_open);
decl2!(t_sem_open);
decl1!(t_syscall);
decl3!(t_strfmon);
decl4!(t_strfmon_l);
decl4!(t_mremap);
decl3!(t_makecontext);

/// Check one recorder row: `named` distinct words, then three variadic words.
fn check(name: &str, named: usize) {
    let records = take_records();
    assert_eq!(records.len(), 1, "{name}: the target was not called once");
    let record = &records[0];
    assert_eq!(record.named, named, "{name}: named count");
    assert_eq!(
        record.words.len(),
        named,
        "{name}: the target did not receive `named` words"
    );
    for (index, word) in record.words.iter().enumerate() {
        let expected = 0x1000 + index as u64;
        assert_eq!(*word, expected, "{name}: named word {index}");
    }
    let expected_values = [0xAAu64, 0xBB, 0xCC];
    assert_eq!(
        &record.values[..3],
        &expected_values,
        "{name}: the reader is not positioned at argument {named}"
    );
}

macro_rules! call1 {
    ($name:ident) => {
        // SAFETY: one named word and three variadic words.
        unsafe { $name(0x1000, 0xAAusize, 0xBBusize, 0xCCusize) }
    };
}
macro_rules! call2 {
    ($name:ident) => {
        // SAFETY: two named words and three variadic words.
        unsafe { $name(0x1000, 0x1001, 0xAAusize, 0xBBusize, 0xCCusize) }
    };
}
macro_rules! call3 {
    ($name:ident) => {
        // SAFETY: three named words and three variadic words.
        unsafe { $name(0x1000, 0x1001, 0x1002, 0xAAusize, 0xBBusize, 0xCCusize) }
    };
}
macro_rules! call4 {
    ($name:ident) => {
        // SAFETY: four named words and three variadic words.
        unsafe {
            $name(
                0x1000, 0x1001, 0x1002, 0x1003, 0xAAusize, 0xBBusize, 0xCCusize,
            )
        }
    };
}

macro_rules! test_row {
    ($test:ident, $name:ident, $named:literal, $call:ident) => {
        #[test]
        fn $test() {
            let _guard = serial();
            reset_records();
            let result = $call!($name);
            assert_eq!(result, 0x5A5A, "{}: return value", stringify!($name));
            check(stringify!($name), $named);
        }
    };
}

test_row!(row_printf, t_printf, 1, call1);
test_row!(row_fprintf, t_fprintf, 2, call2);
test_row!(row_sprintf, t_sprintf, 2, call2);
test_row!(row_dprintf, t_dprintf, 2, call2);
test_row!(row_asprintf, t_asprintf, 2, call2);
test_row!(row_scanf, t_scanf, 1, call1);
test_row!(row_fscanf, t_fscanf, 2, call2);
test_row!(row_wprintf, t_wprintf, 1, call1);
test_row!(row_fwprintf, t_fwprintf, 2, call2);
test_row!(row_swprintf, t_swprintf, 3, call3);
test_row!(row_wscanf, t_wscanf, 1, call1);
test_row!(row_fwscanf, t_fwscanf, 2, call2);
test_row!(row_swscanf, t_swscanf, 2, call2);
test_row!(row_snprintf_ss, t_snprintf_ss, 3, call3);
test_row!(row_err, t_err, 2, call2);
test_row!(row_errx, t_errx, 2, call2);
test_row!(row_warn, t_warn, 1, call1);
test_row!(row_warnx, t_warnx, 1, call1);
test_row!(row_errc, t_errc, 3, call3);
test_row!(row_warnc, t_warnc, 2, call2);
test_row!(row_syslog, t_syslog, 2, call2);
test_row!(row_open, t_open, 2, call2);
test_row!(row_openat, t_openat, 3, call3);
test_row!(row_fcntl, t_fcntl, 2, call2);
test_row!(row_ioctl, t_ioctl, 2, call2);
test_row!(row_execl, t_execl, 2, call2);
test_row!(row_execle, t_execle, 2, call2);
test_row!(row_execlp, t_execlp, 2, call2);
test_row!(row_semctl, t_semctl, 3, call3);
test_row!(row_prctl, t_prctl, 1, call1);
test_row!(row_ptrace, t_ptrace, 1, call1);
test_row!(row_mq_open, t_mq_open, 2, call2);
test_row!(row_sem_open, t_sem_open, 2, call2);
test_row!(row_syscall, t_syscall, 1, call1);
test_row!(row_strfmon, t_strfmon, 3, call3);
test_row!(row_strfmon_l, t_strfmon_l, 4, call4);
test_row!(row_mremap, t_mremap, 4, call4);
test_row!(row_makecontext, t_makecontext, 3, call3);

#[test]
fn the_snprintf_row_is_the_real_engine() {
    let format = b"value=%d\n\0";
    let mut buffer = [0u8; 64];
    // SAFETY: `t_snprintf` takes buf, n, fmt and one variadic integer.
    let result = unsafe {
        t_snprintf(
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            format.as_ptr() as usize,
            42i32,
        )
    };
    assert_eq!(result, 9);
    assert_eq!(&buffer[..9], b"value=42\n");
}

#[test]
fn the_sscanf_row_is_the_real_engine() {
    let input = b"42 0x10\0";
    let format = b"%d %x\0";
    let mut first = 0i32;
    let mut second = 0u32;
    // SAFETY: the destinations match the two conversions of the format.
    let result = unsafe {
        t_sscanf(
            input.as_ptr() as usize,
            format.as_ptr() as usize,
            core::ptr::addr_of_mut!(first),
            core::ptr::addr_of_mut!(second),
        )
    };
    assert_eq!(result, 2);
    assert_eq!(first, 42);
    assert_eq!(second, 16);
}
