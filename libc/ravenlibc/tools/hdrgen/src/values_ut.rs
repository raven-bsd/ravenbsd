// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for literals, evaluation and rendering.

use super::*;
use crate::model::ConstFile;

fn def(entry: &str) -> ConstDef {
    let text = format!(
        "schema = 1\ngroup = \"g\"\nc_header = \"bits/abi_g.h\"\nguard = \"_G\"\nconsts = [{entry}]\n"
    );
    let mut file: ConstFile = toml::from_str(&text).expect("const file");
    file.consts.remove(0)
}

fn eval(entry: &str) -> i128 {
    let mut resolve = |name: &str| -> Result<i128> {
        Ok(match name {
            "O_PATH" => 0o10000000,
            "A" => 5,
            other => bail!("unknown {other}"),
        })
    };
    evaluate(&def(entry), &mut resolve).expect("evaluate")
}

#[test]
fn parses_decimal() {
    assert_eq!(parse_int_literal("42").unwrap(), 42);
    assert_eq!(parse_int_literal("-7").unwrap(), -7);
    assert_eq!(parse_int_literal("+3").unwrap(), 3);
}

#[test]
fn parses_hex_and_octal_prefixes() {
    assert_eq!(parse_int_literal("0xff").unwrap(), 255);
    assert_eq!(parse_int_literal("0X10").unwrap(), 16);
    assert_eq!(parse_int_literal("0o777").unwrap(), 511);
    assert_eq!(parse_int_literal("0b101").unwrap(), 5);
}

#[test]
fn parses_c_suffixes() {
    assert_eq!(parse_int_literal("1U").unwrap(), 1);
    assert_eq!(parse_int_literal("2UL").unwrap(), 2);
    assert_eq!(parse_int_literal("3ULL").unwrap(), 3);
    assert_eq!(parse_int_literal("0x40U").unwrap(), 64);
}

#[test]
fn parses_u64_max() {
    assert_eq!(
        parse_int_literal("0xffffffffffffffff").unwrap(),
        i128::from(u64::MAX)
    );
}

#[test]
fn rejects_bad_literals() {
    assert!(parse_int_literal("").is_err());
    assert!(parse_int_literal("nope").is_err());
    assert!(parse_int_literal("0xzz").is_err());
}

#[test]
fn type_fits_boundaries() {
    assert!(fits(127, "i8"));
    assert!(!fits(128, "i8"));
    assert!(fits(-128, "i8"));
    assert!(!fits(-129, "i8"));
    assert!(fits(255, "u8"));
    assert!(!fits(-1, "u8"));
    assert!(fits(i128::from(u64::MAX), "u64"));
    assert!(!fits(i128::from(u64::MAX) + 1, "u64"));
    assert!(fits(i128::from(i32::MIN), "i32"));
    assert!(!fits(i128::from(u32::MAX) + 1, "u32"));
    assert!(!fits(0, "nonsense"));
}

#[test]
fn renders_c_bases() {
    assert_eq!(
        render_c(&def("{ name = \"A\", value = 10 }"), 10, "dec"),
        "10"
    );
    assert_eq!(
        render_c(&def("{ name = \"A\", value = 15 }"), 15, "hex"),
        "0xf"
    );
    assert_eq!(
        render_c(&def("{ name = \"A\", value = 8 }"), 8, "oct"),
        "010"
    );
    assert_eq!(render_c(&def("{ name = \"A\", value = 0 }"), 0, "oct"), "0");
}

#[test]
fn renders_c_negative_parenthesised() {
    assert_eq!(
        render_c(&def("{ name = \"A\", value = -100 }"), -100, "dec"),
        "(-100)"
    );
}

#[test]
fn renders_c_suffix() {
    assert_eq!(
        render_c(
            &def("{ name = \"A\", value = 1000000, suffix = \"L\" }"),
            1000000,
            "dec"
        ),
        "1000000L"
    );
}

#[test]
fn renders_c_alias_or_and_shift() {
    assert_eq!(
        render_c(&def("{ name = \"A\", alias = \"B\" }"), 0, "dec"),
        "B"
    );
    assert_eq!(
        render_c(
            &def("{ name = \"A\", or = [3, \"O_PATH\"] }"),
            0o10000003,
            "oct"
        ),
        "(03|O_PATH)"
    );
    assert_eq!(
        render_c(&def("{ name = \"A\", shl = [14, 26] }"), 939524096, "dec"),
        "(14 << 26)"
    );
}

#[test]
fn renders_rust_forms() {
    assert_eq!(
        render_rust(&def("{ name = \"A\", value = 8 }"), 8, "oct"),
        "0o10"
    );
    assert_eq!(
        render_rust(&def("{ name = \"A\", value = -2 }"), -2, "dec"),
        "-2"
    );
    assert_eq!(
        render_rust(&def("{ name = \"A\", alias = \"B\" }"), 0, "dec"),
        "B"
    );
    assert_eq!(
        render_rust(&def("{ name = \"A\", or = [3, \"O_PATH\"] }"), 3, "oct"),
        "0o3 | O_PATH"
    );
    assert_eq!(
        render_rust(&def("{ name = \"A\", shl = [14, 26] }"), 0, "dec"),
        "14 << 26"
    );
}

#[test]
fn renders_i128_literals() {
    assert_eq!(render_i128(1), "1i128");
    assert_eq!(render_i128(-1), "-1i128");
    assert_eq!(
        render_i128(i128::from(u64::MAX)),
        "18446744073709551615i128"
    );
}

#[test]
fn evaluates_value_alias_or_and_shift() {
    assert_eq!(eval("{ name = \"A\", value = 3 }"), 3);
    assert_eq!(eval("{ name = \"B\", alias = \"O_PATH\" }"), 0o10000000);
    assert_eq!(eval("{ name = \"C\", or = [3, \"O_PATH\"] }"), 0o10000003);
    assert_eq!(eval("{ name = \"D\", shl = [14, 26] }"), 939524096);
}

#[test]
fn evaluate_rejects_zero_or_two_forms() {
    let mut resolve = |_: &str| -> Result<i128> { Ok(0) };
    assert!(evaluate(&def("{ name = \"A\" }"), &mut resolve).is_err());
    let text = "schema = 1\ngroup = \"g\"\nc_header = \"bits/abi_g.h\"\nguard = \"_G\"\nconsts = [ { name = \"A\", value = 1, alias = \"B\" } ]\n";
    let mut file: ConstFile = toml::from_str(text).expect("file");
    assert!(evaluate(&file.consts.remove(0), &mut resolve).is_err());
}

#[test]
fn evaluates_shift_range() {
    let mut resolve = |_: &str| -> Result<i128> { Ok(0) };
    assert!(evaluate(&def("{ name = \"A\", shl = [1, 200] }"), &mut resolve).is_err());
}

#[test]
fn parse_defines_keeps_object_like_macros() {
    let rows = parse_defines(
        "#define EPERM 1\n#define F(x) ((x)+1)\n#define ALIAS EPERM\n#define EMPTY\n",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "EPERM");
    assert_eq!(rows[1].0, "ALIAS");
}

#[test]
fn define_set_resolves_aliases_and_casts() {
    let mut set = DefineSet::default();
    set.definitions.insert("A".to_string(), "1".to_string());
    set.definitions.insert("B".to_string(), "A".to_string());
    set.definitions
        .insert("C".to_string(), "((in_addr_t) 7)".to_string());
    set.definitions
        .insert("D".to_string(), "((void *) -1)".to_string());
    set.definitions
        .insert("E".to_string(), "call(1)".to_string());
    set.definitions
        .insert("F".to_string(), "(1U << 31)".to_string());
    assert_eq!(set.constant_value("B"), Some(1));
    assert_eq!(set.constant_value("C"), Some(7));
    assert_eq!(set.constant_value("D"), None);
    assert_eq!(set.constant_value("E"), None);
    assert_eq!(set.constant_value("F"), Some(1 << 31));
}

#[test]
fn define_set_prefix_scan() {
    let mut set = DefineSet::default();
    set.definitions.insert("O_A".to_string(), "1".to_string());
    set.definitions.insert("O_B".to_string(), "2".to_string());
    set.definitions.insert("F_C".to_string(), "3".to_string());
    let names = set.names_with_prefixes(&["O_".to_string()]);
    assert_eq!(names, vec!["O_A", "O_B"]);
}

#[test]
fn expression_parser_handles_precedence() {
    let mut none = |_: &str| -> Option<i128> { None };
    assert_eq!(eval_expression("1 | 2 & 4", &mut none), Some(1));
    assert_eq!(eval_expression("(1 | 2) & 4", &mut none), Some(0));
    assert_eq!(eval_expression("1 << 4", &mut none), Some(16));
    assert_eq!(eval_expression("~0", &mut none), Some(-1));
    assert_eq!(eval_expression("010", &mut none), Some(8));
    assert_eq!(eval_expression("8 / 2", &mut none), Some(4));
}

#[test]
fn expression_parser_rejects_non_constants() {
    let mut none = |_: &str| -> Option<i128> { None };
    assert_eq!(eval_expression("8 / 0", &mut none), None);
    assert_eq!(eval_expression("1 << 200", &mut none), None);
    assert_eq!(eval_expression("\"text\"", &mut none), None);
    assert_eq!(eval_expression("a[0]", &mut none), None);
    assert_eq!(eval_expression("a && b", &mut none), None);
    assert_eq!(eval_expression("(EAGAIN)", &mut none), None);
}

#[test]
fn expression_parser_resolves_names() {
    let mut lookup = |name: &str| -> Option<i128> {
        match name {
            "A" => Some(4),
            "B" => Some(3),
            _ => None,
        }
    };
    assert_eq!(eval_expression("A|B", &mut lookup), Some(7));
    assert_eq!(eval_expression("-(A+B)", &mut lookup), Some(-7));
}
