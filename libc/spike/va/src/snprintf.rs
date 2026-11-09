// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A small `snprintf` consumer built on [`VaReader`](crate::VaReader).

use core::fmt::Write as _;

use crate::va::{VaList, VaReader};

const MAX_WIDTH: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Length {
    None,
    Long,
    LongLong,
    Size,
}

struct Spec {
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    width: Option<usize>,
    precision: Option<usize>,
    length: Length,
    conversion: u8,
    next_index: usize,
}

struct Writer {
    buf: *mut u8,
    capacity: usize,
    len: usize,
}

impl Writer {
    fn new(buf: *mut u8, capacity: usize) -> Self {
        Self {
            buf,
            capacity,
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len + 1 < self.capacity && !self.buf.is_null() {
            // SAFETY: the caller guarantees `buf` has `capacity` writable bytes; `len` is below
            // `capacity - 1` here, so the store is in bounds.
            unsafe { *self.buf.add(self.len) = byte };
        }
        self.len = self.len.saturating_add(1);
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push(byte);
        }
    }

    fn pad(&mut self, count: usize, byte: u8) {
        for _ in 0..count {
            self.push(byte);
        }
    }

    fn finish(self) -> i32 {
        if self.capacity > 0 && !self.buf.is_null() {
            let position = self.len.min(self.capacity - 1);
            // SAFETY: `position` is within the caller-provided `capacity` bytes.
            unsafe { *self.buf.add(position) = 0 };
        }
        i32::try_from(self.len).unwrap_or(i32::MAX)
    }
}

fn read_byte(fmt: *const u8, index: usize) -> u8 {
    // SAFETY: the caller guarantees `fmt` is a NUL-terminated string and the parser never advances
    // beyond its NUL.
    unsafe { *fmt.add(index) }
}

fn parse_spec(fmt: *const u8, mut index: usize) -> Option<Spec> {
    let mut left = false;
    let mut zero = false;
    let mut plus = false;
    let mut space = false;
    loop {
        match read_byte(fmt, index) {
            b'-' => left = true,
            b'0' => zero = true,
            b'+' => plus = true,
            b' ' => space = true,
            _ => break,
        }
        index += 1;
    }

    let mut width = None;
    if read_byte(fmt, index).is_ascii_digit() {
        let mut value = 0usize;
        while read_byte(fmt, index).is_ascii_digit() {
            value = value
                .saturating_mul(10)
                .saturating_add(usize::from(read_byte(fmt, index) - b'0'))
                .min(MAX_WIDTH);
            index += 1;
        }
        width = Some(value);
    }

    let mut precision = None;
    if read_byte(fmt, index) == b'.' {
        index += 1;
        let mut value = 0usize;
        while read_byte(fmt, index).is_ascii_digit() {
            value = value
                .saturating_mul(10)
                .saturating_add(usize::from(read_byte(fmt, index) - b'0'))
                .min(MAX_WIDTH);
            index += 1;
        }
        precision = Some(value);
    }

    let mut length = Length::None;
    if read_byte(fmt, index) == b'l' {
        index += 1;
        if read_byte(fmt, index) == b'l' {
            index += 1;
            length = Length::LongLong;
        } else {
            length = Length::Long;
        }
    } else if read_byte(fmt, index) == b'z' {
        index += 1;
        length = Length::Size;
    }

    let conversion = read_byte(fmt, index);
    if conversion == 0 {
        return None;
    }
    Some(Spec {
        left,
        zero,
        plus,
        space,
        width,
        precision,
        length,
        conversion,
        next_index: index + 1,
    })
}

/// Implementation called by the assembly trampoline.
///
/// # Safety
///
/// Called only from `my_snprintf` with a live `va_list` and a valid format string.
#[unsafe(no_mangle)]
pub extern "C" fn raven_va_snprintf_impl(
    buf: *mut u8,
    n: usize,
    fmt: *const u8,
    va: *mut VaList,
) -> i32 {
    let mut writer = Writer::new(buf, n);
    // SAFETY: the trampoline passes the live `va_list` it just built.
    let mut reader = unsafe { VaReader::new(va) };
    let mut index = 0usize;
    loop {
        let byte = read_byte(fmt, index);
        if byte == 0 {
            break;
        }
        if byte != b'%' {
            writer.push(byte);
            index += 1;
            continue;
        }
        index += 1;
        if read_byte(fmt, index) == b'%' {
            writer.push(b'%');
            index += 1;
            continue;
        }
        let Some(spec) = parse_spec(fmt, index) else {
            writer.push(b'%');
            break;
        };
        index = spec.next_index;
        format_one(&mut writer, &mut reader, &spec);
    }
    writer.finish()
}

fn format_one(writer: &mut Writer, reader: &mut VaReader, spec: &Spec) {
    match spec.conversion {
        b'd' | b'i' => {
            let value = match spec.length {
                Length::None => i64::from(reader.arg_i64() as i32),
                Length::Long | Length::LongLong => reader.arg_i64(),
                Length::Size => reader.arg_i64() as isize as i64,
            };
            format_signed(writer, value, 10, false, spec);
        }
        b'u' => {
            let value = match spec.length {
                Length::None => u64::from(reader.arg_u64() as u32),
                Length::Long | Length::LongLong => reader.arg_u64(),
                Length::Size => reader.arg_u64() as usize as u64,
            };
            format_unsigned(writer, value, 10, false, spec);
        }
        b'x' | b'X' => {
            let value = match spec.length {
                Length::None => u64::from(reader.arg_u64() as u32),
                Length::Long | Length::LongLong => reader.arg_u64(),
                Length::Size => reader.arg_u64() as usize as u64,
            };
            format_unsigned(writer, value, 16, spec.conversion == b'X', spec);
        }
        b'o' => {
            let value = match spec.length {
                Length::None => u64::from(reader.arg_u64() as u32),
                Length::Long | Length::LongLong => reader.arg_u64(),
                Length::Size => reader.arg_u64() as usize as u64,
            };
            format_unsigned(writer, value, 8, false, spec);
        }
        b'c' => {
            let byte = u8::try_from(reader.arg_u64() as u32 & 0xff).unwrap_or(0);
            let width = spec.width.unwrap_or(0);
            if !spec.left {
                writer.pad(width.saturating_sub(1), b' ');
            }
            writer.push(byte);
            if spec.left {
                writer.pad(width.saturating_sub(1), b' ');
            }
        }
        b's' => {
            let pointer = reader.arg_ptr::<u8>();
            let width = spec.width.unwrap_or(0);
            if pointer.is_null() {
                let bytes = b"(null)";
                if !spec.left {
                    writer.pad(width.saturating_sub(bytes.len()), b' ');
                }
                writer.push_bytes(bytes);
                if spec.left {
                    writer.pad(width.saturating_sub(bytes.len()), b' ');
                }
            } else {
                // SAFETY: the pointer is a caller-supplied C string; the scan stops at NUL or precision.
                let length = unsafe { c_string_length(pointer, spec.precision) };
                // SAFETY: the caller supplied a C string; `length` was measured within it.
                let bytes = unsafe { core::slice::from_raw_parts(pointer, length) };
                if !spec.left {
                    writer.pad(width.saturating_sub(bytes.len()), b' ');
                }
                writer.push_bytes(bytes);
                if spec.left {
                    writer.pad(width.saturating_sub(bytes.len()), b' ');
                }
            }
        }
        b'p' => {
            let pointer = reader.arg_ptr::<u8>();
            if pointer.is_null() {
                let text = b"(nil)";
                let width = spec.width.unwrap_or(0);
                if !spec.left {
                    writer.pad(width.saturating_sub(text.len()), b' ');
                }
                writer.push_bytes(text);
                if spec.left {
                    writer.pad(width.saturating_sub(text.len()), b' ');
                }
            } else {
                format_pointer(writer, pointer as usize, spec);
            }
        }
        b'f' => {
            let value = reader.arg_f64();
            format_float(writer, value, spec);
        }
        other => {
            writer.push(b'%');
            writer.push(other);
        }
    }
}

fn format_signed(writer: &mut Writer, value: i64, base: u32, upper: bool, spec: &Spec) {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let sign = if negative {
        b'-'
    } else if spec.plus {
        b'+'
    } else if spec.space {
        b' '
    } else {
        0
    };
    format_number(writer, magnitude, base, upper, sign != 0, sign, spec);
}

fn format_unsigned(writer: &mut Writer, value: u64, base: u32, upper: bool, spec: &Spec) {
    format_number(writer, value, base, upper, false, 0, spec);
}

fn format_number(
    writer: &mut Writer,
    value: u64,
    base: u32,
    upper: bool,
    has_sign: bool,
    sign: u8,
    spec: &Spec,
) {
    let mut digits = [0u8; 64];
    let mut count = 0usize;
    let mut magnitude = value;
    if magnitude == 0 {
        if spec.precision != Some(0) {
            digits[0] = b'0';
            count = 1;
        }
    } else {
        while magnitude != 0 {
            let digit = (magnitude % u64::from(base)) as u8;
            digits[count] = if digit < 10 {
                b'0' + digit
            } else if upper {
                b'A' + digit - 10
            } else {
                b'a' + digit - 10
            };
            count += 1;
            magnitude /= u64::from(base);
        }
    }
    let precision = spec.precision.unwrap_or(0).min(MAX_WIDTH);
    let digit_width = count.max(if spec.precision.is_some() {
        precision
    } else {
        count
    });
    let sign_width = usize::from(has_sign);
    let width = spec.width.unwrap_or(0);
    let content_width = sign_width.saturating_add(digit_width);
    let pad = width.saturating_sub(content_width);
    let (left_pad, zero_pad, right_pad) = if spec.left {
        (0, 0, pad)
    } else if spec.zero && spec.precision.is_none() {
        (0, pad, 0)
    } else {
        (pad, 0, 0)
    };

    writer.pad(left_pad, b' ');
    if has_sign {
        writer.push(sign);
    }
    writer.pad(zero_pad, b'0');
    writer.pad(digit_width.saturating_sub(count), b'0');
    for index in (0..count).rev() {
        writer.push(digits[index]);
    }
    writer.pad(right_pad, b' ');
}

fn format_pointer(writer: &mut Writer, value: usize, spec: &Spec) {
    let mut digits = [0u8; 32];
    let mut count = 0usize;
    let mut magnitude = value;
    if magnitude == 0 {
        digits[0] = b'0';
        count = 1;
    } else {
        while magnitude != 0 {
            let digit = (magnitude % 16) as u8;
            digits[count] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            };
            count += 1;
            magnitude /= 16;
        }
    }
    let width = spec.width.unwrap_or(0);
    let content_width = 2usize.saturating_add(count);
    let pad = width.saturating_sub(content_width);
    let (left_pad, zero_pad, right_pad) = if spec.left {
        (0, 0, pad)
    } else if spec.zero {
        (0, pad, 0)
    } else {
        (pad, 0, 0)
    };
    writer.pad(left_pad, b' ');
    writer.push_bytes(b"0x");
    writer.pad(zero_pad, b'0');
    for index in (0..count).rev() {
        writer.push(digits[index]);
    }
    writer.pad(right_pad, b' ');
}

fn format_float(writer: &mut Writer, value: f64, spec: &Spec) {
    let precision = spec.precision.unwrap_or(6).min(18);
    let negative = value.is_sign_negative();
    let sign = if negative {
        b'-'
    } else if spec.plus {
        b'+'
    } else if spec.space {
        b' '
    } else {
        0
    };
    let mut text = [0u8; 128];
    let mut buffer = SliceWriter::new(&mut text);
    if value.is_nan() {
        let _ = buffer.write_str("nan");
    } else if value.is_infinite() {
        let _ = buffer.write_str("inf");
    } else {
        let _ = write!(&mut buffer, "{:.*}", precision, value.abs());
    }
    let slice = buffer.bytes();
    let width = spec.width.unwrap_or(0);
    let sign_width = usize::from(sign != 0);
    let content_width = sign_width.saturating_add(slice.len());
    let pad = width.saturating_sub(content_width);
    let (left_pad, zero_pad, right_pad) = if spec.left {
        (0, 0, pad)
    } else if spec.zero {
        (0, pad, 0)
    } else {
        (pad, 0, 0)
    };
    writer.pad(left_pad, b' ');
    if sign != 0 {
        writer.push(sign);
    }
    writer.pad(zero_pad, b'0');
    writer.push_bytes(slice);
    writer.pad(right_pad, b' ');
}

unsafe fn c_string_length(pointer: *const u8, precision: Option<usize>) -> usize {
    let max = precision.unwrap_or(MAX_WIDTH);
    let mut length = 0usize;
    while length < max {
        // SAFETY: the caller supplied a C string; scanning stops at its NUL.
        if unsafe { *pointer.add(length) } == 0 {
            break;
        }
        length += 1;
    }
    length
}

struct SliceWriter<'a> {
    buffer: &'a mut [u8],
    len: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, len: 0 }
    }

    fn bytes(&self) -> &[u8] {
        self.buffer.get(..self.len).unwrap_or(&[])
    }
}

impl core::fmt::Write for SliceWriter<'_> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let bytes = text.as_bytes();
        let available = self.buffer.len().saturating_sub(self.len);
        let take = available.min(bytes.len());
        if let Some(destination) = self.buffer.get_mut(self.len..self.len.saturating_add(take)) {
            destination.copy_from_slice(bytes.get(..take).unwrap_or(&[]));
            self.len = self.len.saturating_add(take);
        }
        Ok(())
    }
}
