// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded `-l` math library functions.

use crate::number::Number;

const PI: &str =
    "3.14159265358979323846264338327950288419716939937510582097494459230781640628620899";
const SERIES_LIMIT: usize = 500;

fn work_scale(scale: usize) -> usize {
    scale.saturating_add(10)
}

fn integer(value: usize) -> Option<Number> {
    Some(Number::from_i64(i64::try_from(value).ok()?))
}

pub(crate) fn exponential(value: &Number, scale: usize) -> Option<Number> {
    let work = work_scale(scale);
    let mut sum = Number::one();
    let mut term = Number::one();
    for index in 1..=SERIES_LIMIT {
        term = term.mul(value, work).div(&integer(index)?, work)?;
        if term.is_zero() {
            break;
        }
        sum = sum.add(&term);
    }
    Some(sum.with_scale(scale))
}

pub(crate) fn logarithm(value: &Number, scale: usize) -> Option<Number> {
    if value.is_zero() || value.is_negative() {
        return None;
    }
    let work = work_scale(scale);
    let one = Number::one();
    let z = value.sub(&one).div(&value.add(&one), work)?;
    let z_squared = z.mul(&z, work);
    let mut sum = z.clone();
    let mut power = z;
    for index in 1..=SERIES_LIMIT {
        power = power.mul(&z_squared, work);
        let term = power.div(&integer(index.saturating_mul(2).saturating_add(1))?, work)?;
        if term.is_zero() {
            break;
        }
        sum = sum.add(&term);
    }
    Some(sum.mul(&integer(2)?, work).with_scale(scale))
}

pub(crate) fn sine(value: &Number, scale: usize) -> Option<Number> {
    let work = work_scale(scale);
    let squared = value.mul(value, work);
    let mut sum = value.clone();
    let mut term = value.clone();
    for index in 1..=SERIES_LIMIT {
        let a = index.saturating_mul(2);
        let denominator = integer(a.saturating_mul(a.saturating_add(1)))?;
        term = term.mul(&squared, work).div(&denominator, work)?;
        if term.is_zero() {
            break;
        }
        if index % 2 == 1 {
            sum = sum.sub(&term);
        } else {
            sum = sum.add(&term);
        }
    }
    Some(sum.with_scale(scale))
}

pub(crate) fn cosine(value: &Number, scale: usize) -> Option<Number> {
    let work = work_scale(scale);
    let squared = value.mul(value, work);
    let mut sum = Number::one();
    let mut term = Number::one();
    for index in 1..=SERIES_LIMIT {
        let a = index.saturating_mul(2);
        let denominator = integer(a.saturating_mul(a.saturating_sub(1)))?;
        term = term.mul(&squared, work).div(&denominator, work)?;
        if term.is_zero() {
            break;
        }
        if index % 2 == 1 {
            sum = sum.sub(&term);
        } else {
            sum = sum.add(&term);
        }
    }
    Some(sum.with_scale(scale))
}

pub(crate) fn arctangent(value: &Number, scale: usize) -> Option<Number> {
    let work = work_scale(scale);
    let result = arctangent_work(value, work)?;
    Some(result.with_scale(scale))
}

fn arctangent_work(value: &Number, work: usize) -> Option<Number> {
    if value.is_zero() {
        return Some(Number::zero());
    }
    if value.is_negative() {
        return Some(arctangent_work(&value.negated(), work)?.negated());
    }
    let half = Number::parse_decimal(".5")?;
    if value.compare(&half) != core::cmp::Ordering::Greater {
        return atan_series(value, work);
    }
    let one = Number::one();
    let pi = Number::parse_decimal(PI)?;
    if value.compare(&one) == core::cmp::Ordering::Less {
        let argument = value.sub(&one).div(&value.add(&one), work)?;
        let quarter = pi.div(&integer(4)?, work)?;
        return Some(quarter.add(&atan_series(&argument, work)?));
    }
    if value.compare(&one) == core::cmp::Ordering::Equal {
        return pi.div(&integer(4)?, work);
    }
    let inverse = one.div(value, work)?;
    let half_pi = pi.div(&integer(2)?, work)?;
    Some(half_pi.sub(&arctangent_work(&inverse, work)?))
}

fn atan_series(value: &Number, work: usize) -> Option<Number> {
    let squared = value.mul(value, work);
    let mut sum = value.clone();
    let mut power = value.clone();
    for index in 1..=SERIES_LIMIT {
        power = power.mul(&squared, work);
        let term = power.div(&integer(index.saturating_mul(2).saturating_add(1))?, work)?;
        if term.is_zero() {
            break;
        }
        if index % 2 == 1 {
            sum = sum.sub(&term);
        } else {
            sum = sum.add(&term);
        }
    }
    Some(sum)
}

pub(crate) fn call(name: &str, value: &Number, scale: usize) -> Option<Number> {
    match name {
        "e" => exponential(value, scale),
        "l" => logarithm(value, scale),
        "s" => sine(value, scale),
        "c" => cosine(value, scale),
        "a" => arctangent(value, scale),
        _ => None,
    }
}

#[cfg(test)]
#[path = "math_ut.rs"]
mod tests;
