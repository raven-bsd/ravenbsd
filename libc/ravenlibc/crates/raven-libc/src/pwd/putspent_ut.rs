// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn entry() -> shadow::Spwd {
    shadow::Spwd {
        namp: core::ptr::null_mut(),
        pwdp: core::ptr::null_mut(),
        lstchg: 1,
        min: 2,
        max: 3,
        warn: 4,
        inact: 5,
        expire: 6,
        flag: 7,
    }
}

#[test]
fn a_regular_line_matches_the_printf_form() {
    let mut line = [0u8; 128];
    let length = render_line(&entry(), b"root", b"x", &mut line).expect("fits");
    assert_eq!(
        core::str::from_utf8(&line[..length]).expect("utf8"),
        "root:x:1:2:3:4:5:6:7\n"
    );
}

#[test]
fn negative_fields_keep_their_sign() {
    let mut value = entry();
    value.lstchg = -1;
    value.expire = -42;
    let mut line = [0u8; 128];
    let length = render_line(&value, b"u", b"p", &mut line).expect("fits");
    assert_eq!(
        core::str::from_utf8(&line[..length]).expect("utf8"),
        "u:p:-1:2:3:4:5:-42:7\n"
    );
}

#[test]
fn a_large_unsigned_flag_is_decimal() {
    let mut value = entry();
    value.flag = u64::MAX;
    let mut line = [0u8; 128];
    let length = render_line(&value, b"", b"", &mut line).expect("fits");
    assert!(
        core::str::from_utf8(&line[..length])
            .expect("utf8")
            .ends_with(":18446744073709551615\n")
    );
}

#[test]
fn an_exact_fit_is_accepted() {
    let mut line = [0u8; 20];
    let length = render_line(&entry(), b"u", b"p", &mut line).expect("fits");
    assert_eq!(length, 18);
}

#[test]
fn an_overflowing_line_is_refused() {
    let mut line = [0u8; 4];
    assert!(render_line(&entry(), b"root", b"x", &mut line).is_none());
}

#[test]
fn a_long_name_that_fits_the_bounded_buffer_is_kept() {
    let name = [b'n'; 900];
    let mut line = [0u8; 1024];
    let length = render_line(&entry(), &name, b"x", &mut line).expect("fits");
    assert_eq!(length, 900 + 2 + 15);
}
