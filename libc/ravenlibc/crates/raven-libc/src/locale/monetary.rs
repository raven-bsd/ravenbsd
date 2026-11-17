// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<monetary.h>`: the `strfmon`/`strfmon_l` variadic targets.
//!
//! C-locale form. The numeric conversion is delegated to the existing `vsnprintf` engine (one
//! `%f` directive per monetary conversion, so the next `double` is consumed from the same
//! `va_list`); the shell then applies the sign, currency and width decoration. The C locale's
//! currency symbol and thousands separator are empty, so grouping and `!` have no visible
//! effect here; a future locale table fills them through `localeconv`.

use core::ffi::{c_char, c_void};

use crate::stdio::va::List;

/// The active monetary data (the C locale today).
fn monetary() -> &'static raven_libc_locale::LConvData {
    raven_libc_locale::localeconv()
}

/// Append `bytes` to `out` at `position`.
fn push(out: &mut [u8], position: &mut usize, bytes: &[u8]) -> bool {
    let Some(end) = position.checked_add(bytes.len()) else {
        return false;
    };
    let Some(slot) = out.get_mut(*position..end) else {
        return false;
    };
    slot.copy_from_slice(bytes);
    *position = end;
    true
}

/// Fill `out` at `position` with `count` copies of `byte`.
fn fill(out: &mut [u8], position: &mut usize, byte: u8, count: usize) -> bool {
    let Some(end) = position.checked_add(count) else {
        return false;
    };
    let Some(slot) = out.get_mut(*position..end) else {
        return false;
    };
    slot.fill(byte);
    *position = end;
    true
}

/// Format one `%n`/`%i` directive.
///
/// `flags` is the raw flag text, `width` and `precision` the parsed numbers. `international`
/// selects `%i` (the international currency) over `%n`.
///
/// # Safety
/// `list` is the trampoline's `va_list`.
unsafe fn convert(
    out: &mut [u8],
    position: &mut usize,
    flags: &[u8],
    width: Option<usize>,
    precision: Option<usize>,
    international: bool,
    list: *mut List,
) -> bool {
    let mut fmt = [0u8; 24];
    let mut at = 0usize;
    let push_fmt = |fmt: &mut [u8; 24], at: &mut usize, byte: u8| -> bool {
        let Some(slot) = fmt.get_mut(*at) else {
            return false;
        };
        *slot = byte;
        *at = at.saturating_add(1);
        true
    };
    if !push_fmt(&mut fmt, &mut at, b'%') {
        return false;
    }
    if let Some(digits) = precision {
        let mut digits = digits;
        if !push_fmt(&mut fmt, &mut at, b'.') {
            return false;
        }
        let mut text = [0u8; 8];
        let mut count = 0usize;
        loop {
            let Some(slot) = text.get_mut(count) else {
                return false;
            };
            *slot = b'0'.wrapping_add((digits % 10) as u8);
            count = count.saturating_add(1);
            digits /= 10;
            if digits == 0 {
                break;
            }
        }
        for index in (0..count).rev() {
            if let Some(byte) = text.get(index).copied()
                && !push_fmt(&mut fmt, &mut at, byte)
            {
                return false;
            }
        }
    }
    if !push_fmt(&mut fmt, &mut at, b'f') || !push_fmt(&mut fmt, &mut at, 0) {
        return false;
    }
    let mut number = [0u8; 128];
    // SAFETY: the caller's `va_list`; `vsnprintf` consumes the next `double`.
    let written = unsafe {
        crate::stdio::vsnprintf::vsnprintf(
            number.as_mut_ptr().cast::<c_char>(),
            number.len(),
            fmt.as_ptr().cast::<c_char>(),
            list,
        )
    };
    if written < 0 {
        return false;
    }
    let length = usize::try_from(written)
        .unwrap_or(0)
        .min(number.len().saturating_sub(1));
    let rendered = number.get(..length).unwrap_or(&[]);
    let (negative, digits) = match rendered.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, rendered),
    };

    let mut body = [0u8; 160];
    let mut body_len = 0usize;
    let data = monetary();
    let symbol = if international {
        data.int_curr_symbol
    } else {
        data.currency_symbol
    };
    let symbol = symbol.split(|byte| *byte == 0).next().unwrap_or(&[]);
    let paren = negative && flags.contains(&b'(');
    let explicit_plus = !negative && flags.contains(&b'+');
    let no_symbol = flags.contains(&b'!');
    let mut ok = true;
    if paren {
        ok = ok && push(&mut body, &mut body_len, b"(");
    } else if negative {
        ok = ok && push(&mut body, &mut body_len, b"-");
    } else if explicit_plus {
        ok = ok && push(&mut body, &mut body_len, b"+");
    }
    let precedes = if international {
        data.int_p_cs_precedes
    } else {
        data.p_cs_precedes
    };
    if !no_symbol && precedes != 0 {
        ok = ok && push(&mut body, &mut body_len, symbol);
        if (international && data.int_p_sep_by_space != 0)
            || (!international && data.p_sep_by_space != 0)
        {
            ok = ok && push(&mut body, &mut body_len, b" ");
        }
    }
    ok = ok && push(&mut body, &mut body_len, digits);
    if !no_symbol && precedes == 0 {
        if (international && data.int_p_sep_by_space != 0)
            || (!international && data.p_sep_by_space != 0)
        {
            ok = ok && push(&mut body, &mut body_len, b" ");
        }
        ok = ok && push(&mut body, &mut body_len, symbol);
    }
    if paren {
        ok = ok && push(&mut body, &mut body_len, b")");
    }
    if !ok {
        return false;
    }
    let body = body.get(..body_len).unwrap_or(&[]);
    let requested = width.unwrap_or(0);
    let pad = requested.saturating_sub(body.len());
    let fill_byte = flags
        .iter()
        .position(|byte| *byte == b'=')
        .and_then(|index| flags.get(index.saturating_add(1)).copied())
        .unwrap_or(b' ');
    if flags.contains(&b'-') {
        push(out, position, body) && fill(out, position, b' ', pad)
    } else {
        fill(out, position, fill_byte, pad) && push(out, position, body)
    }
}

/// The shared `strfmon` body.
///
/// # Safety
/// `s` is NULL or `maxsize` writable bytes; `format` is NUL terminated; `list` is the trampoline.
unsafe fn format_strfmon(
    s: *mut c_char,
    maxsize: usize,
    format: *const c_char,
    list: *mut List,
) -> isize {
    // SAFETY: the caller's format contract.
    let spec = unsafe { crate::ffi::cstr::slice(format) };
    let mut out = [0u8; 1024];
    let mut position = 0usize;
    let mut index = 0usize;
    while let Some(byte) = spec.get(index).copied() {
        index = index.saturating_add(1);
        if byte != b'%' {
            if !push(&mut out, &mut position, &[byte]) {
                crate::ffi::errno::set(raven_libc_abi::errno::Errno(7));
                return -1;
            }
            continue;
        }
        let Some(next) = spec.get(index).copied() else {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
            return -1;
        };
        if next == b'%' {
            index = index.saturating_add(1);
            if !push(&mut out, &mut position, &[b'%']) {
                crate::ffi::errno::set(raven_libc_abi::errno::Errno(7));
                return -1;
            }
            continue;
        }
        let flags_start = index;
        while let Some(flag) = spec.get(index).copied() {
            if matches!(flag, b'=' | b'^' | b'+' | b'(' | b'!' | b'-') {
                index = index.saturating_add(1);
                if flag == b'=' {
                    index = index.saturating_add(1);
                }
            } else {
                break;
            }
        }
        let flags = spec.get(flags_start..index).unwrap_or(&[]);
        let mut width = None;
        let mut value = 0usize;
        let mut seen = false;
        while let Some(digit) = spec.get(index).copied() {
            if let Some(digit) = (digit as char).to_digit(10) {
                value = value.saturating_mul(10).saturating_add(digit as usize);
                index = index.saturating_add(1);
                seen = true;
            } else {
                break;
            }
        }
        if seen {
            width = Some(value);
        }
        let mut precision = None;
        if spec.get(index) == Some(&b'.') {
            index = index.saturating_add(1);
            let mut value = 0usize;
            let mut seen = false;
            while let Some(digit) = spec.get(index).copied() {
                if let Some(digit) = (digit as char).to_digit(10) {
                    value = value.saturating_mul(10).saturating_add(digit as usize);
                    index = index.saturating_add(1);
                    seen = true;
                } else {
                    break;
                }
            }
            precision = Some(if seen { value } else { 0 });
        }
        let Some(conversion) = spec.get(index).copied() else {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
            return -1;
        };
        index = index.saturating_add(1);
        let international = match conversion {
            b'n' => false,
            b'i' => true,
            _ => {
                crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
                return -1;
            }
        };
        // SAFETY: the trampoline's list and the parsed directive.
        if !unsafe {
            convert(
                &mut out,
                &mut position,
                flags,
                width,
                precision,
                international,
                list,
            )
        } {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(7));
            return -1;
        }
    }
    if maxsize != 0 {
        // SAFETY: `s` has `maxsize` writable bytes.
        unsafe {
            let copied =
                crate::ffi::out::write_cstr(s, out.get(..position).unwrap_or(&[]), maxsize);
            let _ = copied;
        }
    }
    isize::try_from(position).unwrap_or(isize::MAX)
}

/// The `strfmon` trampoline target.
///
/// # Safety
/// `s` is NULL or `maxsize` writable bytes; `format` is NUL terminated; `list` is the trampoline.
pub(crate) unsafe extern "C" fn __raven_va_strfmon(
    s: *mut c_char,
    maxsize: usize,
    format: *const c_char,
    list: *mut List,
) -> isize {
    // SAFETY: forwarded trampoline contract.
    unsafe { format_strfmon(s, maxsize, format, list) }
}

/// The `strfmon_l` trampoline target. The C locale has one monetary table, so the locale
/// argument does not change the output; the signature keeps the ABI.
///
/// # Safety
/// As `strfmon`.
pub(crate) unsafe extern "C" fn __raven_va_strfmon_l(
    s: *mut c_char,
    maxsize: usize,
    _locale: *mut c_void,
    format: *const c_char,
    list: *mut List,
) -> isize {
    // SAFETY: forwarded trampoline contract.
    unsafe { format_strfmon(s, maxsize, format, list) }
}
