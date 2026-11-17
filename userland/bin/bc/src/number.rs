// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded arbitrary-precision decimal numbers for `bc`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;

const BASE: u64 = 1_000_000_000;
const BASE_DIGITS: usize = 9;
const MAX_SCALE: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Number {
    negative: bool,
    digits: Vec<u32>,
    scale: usize,
}

impl Number {
    pub(crate) fn zero() -> Self {
        Self {
            negative: false,
            digits: Vec::new(),
            scale: 0,
        }
    }

    pub(crate) fn one() -> Self {
        Self {
            negative: false,
            digits: vec![1],
            scale: 0,
        }
    }

    pub(crate) fn from_i64(value: i64) -> Self {
        let negative = value < 0;
        let magnitude = value.unsigned_abs();
        let mut digits = Vec::new();
        let mut rest = magnitude;
        while rest > 0 {
            digits.push((rest % BASE) as u32);
            rest /= BASE;
        }
        Self {
            negative: negative && !digits.is_empty(),
            digits,
            scale: 0,
        }
    }

    pub(crate) fn from_usize(value: usize) -> Self {
        Self::from_i64(i64::try_from(value).unwrap_or(i64::MAX))
    }

    pub(crate) fn parse_decimal(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (negative, body) = match text.strip_prefix('-') {
            Some(body) => (true, body),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (integer, fraction) = match body.split_once('.') {
            Some((integer, fraction)) => (integer, fraction),
            None => (body, ""),
        };
        if integer.is_empty() && fraction.is_empty() {
            return None;
        }
        if !integer.chars().all(|character| character.is_ascii_digit())
            || !fraction.chars().all(|character| character.is_ascii_digit())
        {
            return None;
        }
        let mut decimal = String::new();
        decimal.push_str(integer);
        decimal.push_str(fraction);
        let mut digits = magnitude_from_decimal(&decimal)?;
        trim_magnitude(&mut digits);
        let zero = digits.is_empty();
        Some(Self {
            negative: negative && !zero,
            digits,
            scale: fraction.len(),
        })
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub(crate) fn is_negative(&self) -> bool {
        self.negative
    }

    pub(crate) fn scale(&self) -> usize {
        self.scale
    }

    pub(crate) fn length(&self) -> usize {
        if self.is_zero() {
            return 1;
        }
        let digits = magnitude_decimal(&self.digits).len();
        if self.scale > 0 && digits <= self.scale {
            self.scale
        } else {
            digits
        }
    }

    pub(crate) fn negated(&self) -> Self {
        if self.is_zero() {
            return self.clone();
        }
        let mut result = self.clone();
        result.negative = !result.negative;
        result
    }

    pub(crate) fn compare(&self, other: &Self) -> Ordering {
        let (left, right, _) = aligned(self, other);
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => compare_magnitude(&left, &right),
            (true, true) => compare_magnitude(&right, &left),
        }
    }

    pub(crate) fn add(&self, other: &Self) -> Self {
        let (left, right, scale) = aligned(self, other);
        if self.negative == other.negative {
            let mut digits = add_magnitude(&left, &right);
            trim_magnitude(&mut digits);
            let zero = digits.is_empty();
            return Self {
                negative: self.negative && !zero,
                digits,
                scale,
            };
        }
        match compare_magnitude(&left, &right) {
            Ordering::Equal => Self {
                negative: false,
                digits: Vec::new(),
                scale,
            },
            Ordering::Greater => {
                let mut digits = subtract_magnitude(&left, &right);
                trim_magnitude(&mut digits);
                Self {
                    negative: self.negative,
                    digits,
                    scale,
                }
            }
            Ordering::Less => {
                let mut digits = subtract_magnitude(&right, &left);
                trim_magnitude(&mut digits);
                Self {
                    negative: other.negative,
                    digits,
                    scale,
                }
            }
        }
    }

    pub(crate) fn sub(&self, other: &Self) -> Self {
        self.add(&other.negated())
    }

    pub(crate) fn mul(&self, other: &Self, global_scale: usize) -> Self {
        let raw_scale = self.scale.saturating_add(other.scale);
        let limit = global_scale.max(self.scale).max(other.scale);
        let scale = raw_scale.min(limit);
        let mut digits = multiply_magnitude(&self.digits, &other.digits);
        truncate_pow10_magnitude(&mut digits, raw_scale.saturating_sub(scale));
        trim_magnitude(&mut digits);
        let zero = digits.is_empty();
        Self {
            negative: (self.negative ^ other.negative) && !zero,
            digits,
            scale,
        }
    }

    pub(crate) fn div(&self, other: &Self, global_scale: usize) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(Self {
                negative: false,
                digits: Vec::new(),
                scale: global_scale,
            });
        }
        let scale = global_scale;
        let exponent = other.scale as isize + global_scale as isize - self.scale as isize;
        let mut numerator = self.digits.clone();
        let denominator = other.digits.clone();
        if exponent >= 0 {
            mul_pow10_magnitude(&mut numerator, exponent as usize);
        } else {
            truncate_pow10_magnitude(&mut numerator, (-exponent) as usize);
        }
        let (mut quotient, _) = divide_magnitude(&numerator, &denominator);
        trim_magnitude(&mut quotient);
        let zero = quotient.is_empty();
        Some(Self {
            negative: (self.negative ^ other.negative) && !zero,
            digits: quotient,
            scale,
        })
    }

    pub(crate) fn rem(&self, other: &Self, global_scale: usize) -> Option<Self> {
        let quotient = self.div(other, global_scale)?;
        let product = quotient.mul(other, global_scale);
        Some(self.sub(&product))
    }

    pub(crate) fn pow(&self, exponent: &Self, global_scale: usize) -> Option<Self> {
        if !exponent.is_integer() {
            return None;
        }
        let exponent = exponent.to_i64()?;
        if exponent.unsigned_abs() > 10_000 {
            return None;
        }
        if exponent == 0 {
            return Some(Self::one());
        }
        let magnitude = exponent.unsigned_abs() as u32;
        let raw_scale = self.scale.checked_mul(magnitude as usize)?;
        if raw_scale > MAX_SCALE {
            return None;
        }
        let mut digits = power_magnitude(&self.digits, magnitude);
        let limit = global_scale.max(self.scale);
        let scale = raw_scale.min(limit);
        truncate_pow10_magnitude(&mut digits, raw_scale.saturating_sub(scale));
        trim_magnitude(&mut digits);
        let zero = digits.is_empty();
        let negative = self.negative && exponent % 2 != 0 && !zero;
        let positive = Self {
            negative,
            digits,
            scale,
        };
        if exponent < 0 {
            Self::one().div(&positive, global_scale)
        } else {
            Some(positive)
        }
    }

    pub(crate) fn sqrt(&self, global_scale: usize) -> Option<Self> {
        if self.negative && !self.is_zero() {
            return None;
        }
        let scale = global_scale.max(self.scale);
        if self.is_zero() {
            return Some(Self {
                negative: false,
                digits: Vec::new(),
                scale,
            });
        }
        let exponent = scale.checked_mul(2)?.checked_sub(self.scale)?;
        let mut radicand = self.digits.clone();
        mul_pow10_magnitude(&mut radicand, exponent);
        let mut digits = integer_sqrt(&radicand);
        trim_magnitude(&mut digits);
        Some(Self {
            negative: false,
            digits,
            scale,
        })
    }

    pub(crate) fn is_integer(&self) -> bool {
        if self.scale == 0 || self.is_zero() {
            return true;
        }
        let mut whole = self.digits.clone();
        truncate_pow10_magnitude(&mut whole, self.scale);
        mul_pow10_magnitude(&mut whole, self.scale);
        trim_magnitude(&mut whole);
        whole == self.digits
    }

    pub(crate) fn to_i64(&self) -> Option<i64> {
        let mut integer = self.digits.clone();
        truncate_pow10_magnitude(&mut integer, self.scale);
        let mut magnitude = 0u128;
        for limb in integer.iter().rev() {
            magnitude = magnitude.checked_mul(BASE as u128)?;
            magnitude = magnitude.checked_add(*limb as u128)?;
        }
        if self.negative {
            if magnitude > i64::MAX as u128 + 1 {
                return None;
            }
            if magnitude == i64::MAX as u128 + 1 {
                return Some(i64::MIN);
            }
            Some(-(magnitude as i64))
        } else {
            i64::try_from(magnitude).ok()
        }
    }

    pub(crate) fn to_decimal_string(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        let digits = magnitude_decimal(&self.digits);
        let mut output = String::new();
        if self.negative {
            output.push('-');
        }
        if self.scale == 0 {
            output.push_str(&digits);
        } else if digits.len() <= self.scale {
            output.push('.');
            for _ in 0..self.scale.saturating_sub(digits.len()) {
                output.push('0');
            }
            output.push_str(&digits);
        } else {
            let split = digits.len() - self.scale;
            output.push_str(digits.get(..split).unwrap_or(""));
            output.push('.');
            output.push_str(digits.get(split..).unwrap_or(""));
        }
        output
    }

    pub(crate) fn with_scale(&self, scale: usize) -> Self {
        let mut digits = self.digits.clone();
        if scale >= self.scale {
            mul_pow10_magnitude(&mut digits, scale - self.scale);
        } else {
            truncate_pow10_magnitude(&mut digits, self.scale - scale);
        }
        trim_magnitude(&mut digits);
        let zero = digits.is_empty();
        Self {
            negative: self.negative && !zero,
            digits,
            scale,
        }
    }
}

fn aligned(left: &Number, right: &Number) -> (Vec<u32>, Vec<u32>, usize) {
    let scale = left.scale.max(right.scale);
    let mut left_digits = left.digits.clone();
    let mut right_digits = right.digits.clone();
    mul_pow10_magnitude(&mut left_digits, scale - left.scale);
    mul_pow10_magnitude(&mut right_digits, scale - right.scale);
    (left_digits, right_digits, scale)
}

fn magnitude_from_decimal(text: &str) -> Option<Vec<u32>> {
    let characters: Vec<char> = text.chars().collect();
    if characters
        .iter()
        .any(|character| !character.is_ascii_digit())
    {
        return None;
    }
    let mut digits = Vec::new();
    let mut end = characters.len();
    while end > 0 {
        let start = end.saturating_sub(BASE_DIGITS);
        let chunk: String = characters.get(start..end)?.iter().collect();
        digits.push(chunk.parse::<u32>().ok()?);
        end = start;
    }
    Some(digits)
}

fn magnitude_decimal(magnitude: &[u32]) -> String {
    if magnitude.is_empty() {
        return "0".to_string();
    }
    let mut output = String::new();
    for (index, limb) in magnitude.iter().rev().enumerate() {
        if index == 0 {
            output.push_str(&format!("{limb}"));
        } else {
            output.push_str(&format!("{limb:09}"));
        }
    }
    output
}

fn trim_magnitude(magnitude: &mut Vec<u32>) {
    while magnitude.last() == Some(&0) {
        magnitude.pop();
    }
}

fn compare_magnitude(left: &[u32], right: &[u32]) -> Ordering {
    if left.len() != right.len() {
        return left.len().cmp(&right.len());
    }
    for (left, right) in left.iter().rev().zip(right.iter().rev()) {
        if left != right {
            return left.cmp(right);
        }
    }
    Ordering::Equal
}

fn add_magnitude(left: &[u32], right: &[u32]) -> Vec<u32> {
    let length = left.len().max(right.len());
    let mut result = Vec::with_capacity(length.saturating_add(1));
    let mut carry = 0u64;
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0) as u64;
        let right = right.get(index).copied().unwrap_or(0) as u64;
        let value = left + right + carry;
        result.push((value % BASE) as u32);
        carry = value / BASE;
    }
    if carry > 0 {
        result.push(carry as u32);
    }
    result
}

fn subtract_magnitude(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut result = Vec::with_capacity(left.len());
    let mut borrow = 0i64;
    for index in 0..left.len() {
        let left = left.get(index).copied().unwrap_or(0) as i64;
        let right = right.get(index).copied().unwrap_or(0) as i64;
        let value = left - right - borrow;
        if value < 0 {
            result.push((value + BASE as i64) as u32);
            borrow = 1;
        } else {
            result.push(value as u32);
            borrow = 0;
        }
    }
    result
}

fn multiply_magnitude(left: &[u32], right: &[u32]) -> Vec<u32> {
    if left.is_empty() || right.is_empty() {
        return Vec::new();
    }
    let mut result = vec![0u32; left.len().saturating_add(right.len())];
    for (left_index, left_limb) in left.iter().enumerate() {
        let mut carry = 0u64;
        for (right_index, right_limb) in right.iter().enumerate() {
            let index = left_index.saturating_add(right_index);
            let value = result.get(index).copied().unwrap_or(0) as u64
                + (*left_limb as u64) * (*right_limb as u64)
                + carry;
            if let Some(slot) = result.get_mut(index) {
                *slot = (value % BASE) as u32;
            }
            carry = value / BASE;
        }
        let mut index = left_index.saturating_add(right.len());
        while carry > 0 {
            let value = result.get(index).copied().unwrap_or(0) as u64 + carry;
            if let Some(slot) = result.get_mut(index) {
                *slot = (value % BASE) as u32;
            }
            carry = value / BASE;
            index = index.saturating_add(1);
        }
    }
    trim_magnitude(&mut result);
    result
}

fn mul_small_magnitude(magnitude: &[u32], factor: u32) -> Vec<u32> {
    if factor == 0 || magnitude.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(magnitude.len().saturating_add(1));
    let mut carry = 0u64;
    for limb in magnitude {
        let value = (*limb as u64) * (factor as u64) + carry;
        result.push((value % BASE) as u32);
        carry = value / BASE;
    }
    while carry > 0 {
        result.push((carry % BASE) as u32);
        carry /= BASE;
    }
    trim_magnitude(&mut result);
    result
}

fn div_small_magnitude(magnitude: &[u32], divisor: u32) -> (Vec<u32>, u32) {
    if divisor == 0 {
        return (Vec::new(), 0);
    }
    let mut result = vec![0u32; magnitude.len()];
    let mut remainder = 0u64;
    for index in (0..magnitude.len()).rev() {
        let value = remainder * BASE + magnitude.get(index).copied().unwrap_or(0) as u64;
        if let Some(slot) = result.get_mut(index) {
            *slot = (value / divisor as u64) as u32;
        }
        remainder = value % divisor as u64;
    }
    trim_magnitude(&mut result);
    (result, remainder as u32)
}

fn divide_magnitude(left: &[u32], right: &[u32]) -> (Vec<u32>, Vec<u32>) {
    if right.is_empty() {
        return (Vec::new(), left.to_vec());
    }
    if compare_magnitude(left, right) == Ordering::Less {
        return (Vec::new(), left.to_vec());
    }
    let mut quotient = vec![0u32; left.len()];
    let mut remainder: Vec<u32> = Vec::new();
    for index in (0..left.len()).rev() {
        if remainder.is_empty() {
            remainder.push(left.get(index).copied().unwrap_or(0));
        } else {
            remainder.insert(0, left.get(index).copied().unwrap_or(0));
        }
        trim_magnitude(&mut remainder);
        let mut low = 0u64;
        let mut high = BASE - 1;
        while low <= high {
            let middle = low + (high - low) / 2;
            let product = mul_small_magnitude(right, middle as u32);
            match compare_magnitude(&product, &remainder) {
                Ordering::Greater => {
                    if middle == 0 {
                        break;
                    }
                    high = middle - 1;
                }
                _ => low = middle + 1,
            }
        }
        let digit = high.min(BASE - 1) as u32;
        if let Some(slot) = quotient.get_mut(index) {
            *slot = digit;
        }
        if digit != 0 {
            let product = mul_small_magnitude(right, digit);
            remainder = subtract_magnitude(&remainder, &product);
            trim_magnitude(&mut remainder);
        }
    }
    trim_magnitude(&mut quotient);
    (quotient, remainder)
}

fn power_magnitude(base: &[u32], exponent: u32) -> Vec<u32> {
    if exponent == 0 {
        return vec![1];
    }
    let mut result = vec![1];
    let mut factor = base.to_vec();
    let mut power = exponent;
    while power > 0 {
        if power & 1 == 1 {
            result = multiply_magnitude(&result, &factor);
        }
        power >>= 1;
        if power > 0 {
            factor = multiply_magnitude(&factor, &factor);
        }
    }
    trim_magnitude(&mut result);
    result
}

fn pow10_magnitude(power: usize) -> Vec<u32> {
    let limbs = power / BASE_DIGITS;
    let remainder = power % BASE_DIGITS;
    let mut result = vec![0u32; limbs];
    if remainder == 0 {
        result.push(1);
    } else {
        result.push(10u32.pow(remainder as u32));
    }
    result
}

fn mul_pow10_magnitude(magnitude: &mut Vec<u32>, power: usize) {
    if power == 0 || magnitude.is_empty() {
        return;
    }
    let limbs = power / BASE_DIGITS;
    let remainder = power % BASE_DIGITS;
    if remainder > 0 {
        *magnitude = mul_small_magnitude(magnitude, 10u32.pow(remainder as u32));
    }
    if limbs > 0 {
        let mut zeros = vec![0u32; limbs];
        zeros.extend(magnitude.iter());
        *magnitude = zeros;
    }
    trim_magnitude(magnitude);
}

fn truncate_pow10_magnitude(magnitude: &mut Vec<u32>, power: usize) {
    if power == 0 || magnitude.is_empty() {
        return;
    }
    let limbs = power / BASE_DIGITS;
    let remainder = power % BASE_DIGITS;
    if limbs > 0 {
        if limbs >= magnitude.len() {
            magnitude.clear();
        } else {
            magnitude.drain(0..limbs);
        }
    }
    if remainder > 0 && !magnitude.is_empty() {
        let (quotient, _) = div_small_magnitude(magnitude, 10u32.pow(remainder as u32));
        *magnitude = quotient;
    }
    trim_magnitude(magnitude);
}

fn decimal_digits(magnitude: &[u32]) -> usize {
    if magnitude.is_empty() {
        return 1;
    }
    let top = magnitude.last().copied().unwrap_or(0);
    let top_digits = if top == 0 {
        0
    } else {
        let mut value = top;
        let mut count = 0usize;
        while value > 0 {
            value /= 10;
            count = count.saturating_add(1);
        }
        count
    };
    magnitude.len().saturating_sub(1) * BASE_DIGITS + top_digits
}

fn integer_sqrt(magnitude: &[u32]) -> Vec<u32> {
    if magnitude.is_empty() {
        return Vec::new();
    }
    let digits = decimal_digits(magnitude);
    let shift = digits.div_ceil(2);
    let mut current = pow10_magnitude(shift);
    loop {
        let (quotient, _) = divide_magnitude(magnitude, &current);
        let sum = add_magnitude(&current, &quotient);
        let (next, _) = div_small_magnitude(&sum, 2);
        if compare_magnitude(&next, &current) != Ordering::Less {
            trim_magnitude(&mut current);
            return current;
        }
        current = next;
    }
}

#[cfg(test)]
#[path = "number_ut.rs"]
mod tests;
