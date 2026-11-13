// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::printf::vformat;
use crate::test_support::{FakeFloat, SliceVa, VecSink};

fn f(fmt: &str, v: f64) -> String {
    let mut va = SliceVa::new();
    va.dbl(v);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    vformat(&mut sink, fmt.as_bytes(), &mut va, &opts, &FakeFloat).unwrap();
    String::from_utf8(sink.data).unwrap()
}

#[test]
fn e_and_e_upper() {
    assert_eq!(f("%e", 1.5), "1.500000e+00");
    assert_eq!(f("%E", 1.5), "1.500000E+00");
    assert_eq!(f("%.2e", 1.5), "1.50e+00");
    assert_eq!(f("%.0e", 1.5), "2e+00");
    assert_eq!(f("%12.3e", 1.5), "   1.500e+00");
    assert_eq!(f("%-12.3e|", 1.5), "1.500e+00   |");
    assert_eq!(f("%012.3e", 1.5), "0001.500e+00");
}

#[test]
fn f_and_f_upper() {
    assert_eq!(f("%.2f", 1.5), "1.50");
    assert_eq!(f("%#.2f", 1.5), "1.50");
    assert_eq!(f("%#.0f", 2.0), "2.");
    assert_eq!(f("%.0f", 0.0), "0");
    assert_eq!(f("%.0f", 0.4), "0");
    assert_eq!(f("%.0f", 0.6), "1");
    assert_eq!(f("%.3f", -0.0015), "-0.002");
}

#[test]
fn g_switch() {
    assert_eq!(f("%g", 1.5), "1.5");
    assert_eq!(f("%g", 0.0001), "0.0001");
    assert_eq!(f("%g", 0.00001), "1e-05");
    assert_eq!(f("%g", 1234567.0), "1.23457e+06");
    assert_eq!(f("%g", 100000.0), "100000");
    assert_eq!(f("%.0g", 1.5), "2");
    assert_eq!(f("%#g", 1.5), "1.50000");
    assert_eq!(f("%G", 0.00001), "1E-05");
}

#[test]
fn a_and_a_upper() {
    assert_eq!(f("%a", 1.5), "0x1.8p+0");
    assert_eq!(f("%A", 1.5), "0X1.8P+0");
    assert_eq!(f("%.0a", 1.5), "0x1p+0");
    assert_eq!(f("%#.0a", 1.5), "0x1.p+0");
    assert_eq!(f("%a", 0.0), "0x0p+0");
    assert_eq!(f("%a", -0.0), "-0x0p+0");
}

#[test]
fn inf_and_nan_rules() {
    assert_eq!(f("%f", f64::INFINITY), "inf");
    assert_eq!(f("%F", f64::INFINITY), "INF");
    assert_eq!(f("%e", f64::INFINITY), "inf");
    assert_eq!(f("%a", f64::INFINITY), "inf");
    assert_eq!(f("%f", f64::NEG_INFINITY), "-inf");
    assert_eq!(f("%f", f64::NAN), "nan");
    assert_eq!(f("%F", f64::NAN), "NAN");
    assert_eq!(f("%+f", f64::NAN), "nan");
    assert_eq!(f("%08f", f64::NAN), "     nan");
    assert_eq!(f("%10f", f64::INFINITY), "       inf");
    assert_eq!(f("%-10f|", f64::INFINITY), "inf       |");
}

#[test]
fn float_precision_and_width_interact() {
    assert_eq!(f("%020.6f", 1.5), "0000000000001.500000");
    assert_eq!(f("%+020.6f", 1.5), "+000000000001.500000");
    assert_eq!(f("% 20.6f", 1.5), "            1.500000");
    assert_eq!(f("%-20.6f|", 1.5), "1.500000            |");
}

#[test]
fn long_double_uses_the_ld_buffer() {
    let mut va = SliceVa::new();
    va.ld_bits(1);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    vformat(&mut sink, b"%Lf", &mut va, &opts, &FakeFloat).unwrap();
    assert_eq!(sink.data, b"1.500000");
}
