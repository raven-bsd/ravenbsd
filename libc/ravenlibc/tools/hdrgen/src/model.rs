// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The TOML data model of `libc/ravenlibc/abi`: types, records, opaque types, constant groups
//! and the deviation register. Every table rejects unknown keys.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The three supported architectures, in report order.
pub const ARCHES: [&str; 3] = ["x86_64", "aarch64", "riscv64"];

/// Rust integer types accepted for a constant.
pub const RUST_TYPES: [&str; 10] = [
    "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "isize", "usize",
];

/// Print bases accepted for a constant.
pub const BASES: [&str; 3] = ["dec", "hex", "oct"];

/// Visibility classes accepted for a constant.
pub const VISIBILITY: [&str; 4] = ["bsd", "gnu", "linux", "solaris"];

/// C integer suffixes accepted for a constant.
pub const SUFFIXES: [&str; 6] = ["", "U", "L", "UL", "ULL", "LL"];

/// `types.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypesFile {
    /// Schema version; must be 1.
    pub schema: u32,
    /// Scalar type rows.
    #[serde(rename = "type", default)]
    pub types: Vec<TypeDef>,
}

/// One scalar type row.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDef {
    /// C name (the primary spelling).
    pub name: String,
    /// Rust type.
    pub rust: String,
    /// C definition.
    pub c: String,
    /// Width in bits.
    pub bits: u16,
    /// Whether the C type is signed.
    pub signed: bool,
    /// Per-architecture overrides (only `wchar_t` and `char` use it).
    #[serde(default)]
    pub per_arch: BTreeMap<String, PerArchType>,
    /// Recorded musl difference, in bits (documents DV1).
    #[serde(default)]
    pub deviates_musl: BTreeMap<String, MuslTypeDeviation>,
    /// Name used by the musl size probe.
    #[serde(default)]
    pub ctype_probe: Option<String>,
    /// Free-form note.
    #[serde(default)]
    pub doc: Option<String>,
}

/// A per-architecture type override.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerArchType {
    /// Rust type on this architecture.
    pub rust: String,
    /// C definition on this architecture.
    pub c: String,
    /// Signedness on this architecture.
    pub signed: bool,
}

/// A recorded musl type difference.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MuslTypeDeviation {
    /// musl width in bits.
    pub bits: u16,
}

/// `structs.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructsFile {
    /// Schema version; must be 1.
    pub schema: u32,
    /// Field-list records.
    #[serde(default)]
    pub record: Vec<RecordDef>,
    /// Sized blobs without public fields.
    #[serde(default)]
    pub opaque: Vec<OpaqueDef>,
}

/// One `#[repr(C)]` record.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDef {
    /// Data name (the TOML key used by fields).
    pub name: String,
    /// Rust type name.
    pub rust: String,
    /// C spelling (`struct stat`).
    pub c: String,
    /// musl spelling used by the layout probe.
    #[serde(default)]
    pub musl: Option<String>,
    /// Size in bytes (all three architectures).
    pub size: i64,
    /// Alignment in bytes (all three architectures).
    pub align: i64,
    /// Per-architecture musl sizes and alignments.
    #[serde(default)]
    pub deviates: BTreeMap<String, LayoutSpec>,
    /// Fields in declaration order.
    pub fields: Vec<FieldDef>,
    /// Free-form note.
    #[serde(default)]
    pub doc: Option<String>,
}

/// A measured size and alignment.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutSpec {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
}

/// One record field.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    /// Field name in the C and Rust struct.
    pub name: String,
    /// Field type grammar (`i32`, `ptr`, `c_char`, `[u8; 8]`, another record or opaque).
    pub ty: String,
    /// Byte offset.
    pub off: i64,
    /// musl field name when it differs (`__tm_gmtoff`).
    #[serde(default)]
    pub musl_field: Option<String>,
    /// Free-form note.
    #[serde(default)]
    pub doc: Option<String>,
}

/// One opaque type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpaqueDef {
    /// Data name.
    pub name: String,
    /// Rust type name.
    pub rust: String,
    /// C spelling.
    pub c: String,
    /// musl spelling used by the layout probe.
    #[serde(default)]
    pub musl: Option<String>,
    /// Size in bytes, or per architecture.
    pub size: IntOrTable,
    /// Alignment in bytes, or per architecture.
    pub align: IntOrTable,
    /// Free-form note.
    #[serde(default)]
    pub doc: Option<String>,
}

/// A TOML integer, or a table of per-architecture integers.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum IntOrTable {
    /// One value for every architecture.
    Int(i64),
    /// One value per architecture.
    Table(BTreeMap<String, i64>),
}

impl IntOrTable {
    /// The value for `arch`, or the shared value.
    #[must_use]
    pub fn get(&self, arch: &str) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Table(table) => table.get(arch).copied(),
        }
    }

    /// Every architecture named by a table (empty for a shared value).
    #[cfg(test)]
    #[must_use]
    pub fn arches(&self) -> Vec<&str> {
        match self {
            Self::Int(_) => Vec::new(),
            Self::Table(table) => table.keys().map(String::as_str).collect(),
        }
    }
}

/// One constant group file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstFile {
    /// Schema version; must be 1.
    pub schema: u32,
    /// Group name; must equal the file stem and the Rust module name.
    pub group: String,
    /// Header path under `abi/generated/include/`.
    pub c_header: String,
    /// Include guard.
    pub guard: String,
    /// Default Rust type.
    #[serde(default = "default_rust_type")]
    pub rust_type: String,
    /// Default print base.
    #[serde(default = "default_base")]
    pub base: String,
    /// musl-relative source files; empty for a first-party group.
    #[serde(default)]
    pub source: Vec<String>,
    /// Prefixes used by the completeness scan.
    #[serde(default)]
    pub prefixes: Vec<String>,
    /// Deliberately absent musl names.
    #[serde(default)]
    pub omit: Vec<OmitRow>,
    /// The constants.
    #[serde(default)]
    pub consts: Vec<ConstDef>,
}

fn default_rust_type() -> String {
    "i32".to_string()
}

fn default_base() -> String {
    "dec".to_string()
}

/// One deliberately omitted musl name.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OmitRow {
    /// The musl name.
    pub name: String,
    /// Why it is absent.
    pub reason: String,
}

/// One constant entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstDef {
    /// Identifier.
    pub name: String,
    /// A literal value (TOML integer, or a string for values that do not fit `i64`).
    #[serde(default)]
    pub value: Option<toml::Value>,
    /// Alias of another constant.
    #[serde(default)]
    pub alias: Option<String>,
    /// Bitwise OR of terms.
    #[serde(default)]
    pub or: Option<Vec<Term>>,
    /// Shift: `left << right`.
    #[serde(default)]
    pub shl: Option<[i64; 2]>,
    /// Type override.
    #[serde(default)]
    pub ty: Option<String>,
    /// Print-base override.
    #[serde(default)]
    pub base: Option<String>,
    /// C suffix.
    #[serde(default)]
    pub suffix: Option<String>,
    /// Architectures this entry exists on (empty = all).
    #[serde(default)]
    pub arch: Vec<String>,
    /// Feature visibility class.
    #[serde(default)]
    pub visible: Option<String>,
    /// Free-form note.
    #[serde(default)]
    pub doc: Option<String>,
}

/// One term of an `or` expression.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Term {
    /// An integer literal.
    Int(i64),
    /// Another constant.
    Name(String),
}

/// `deviations.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviationFile {
    /// Schema version; must be 1.
    pub schema: u32,
    /// The deviation register.
    #[serde(default)]
    pub deviation: Vec<Deviation>,
}

/// One deliberate deviation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deviation {
    /// Constant, type, record or opaque name.
    pub name: String,
    /// `musl` or `linux-abi`.
    pub against: String,
    /// Our value: an integer, a "size/align" string, or a per-architecture table.
    pub ours: toml::Value,
    /// The donor value, in the same forms as `ours`.
    pub theirs: toml::Value,
    /// Why the difference exists.
    pub reason: String,
}

impl Deviation {
    /// Our value for `arch` as an integer, when the row is constant-shaped.
    #[must_use]
    pub fn ours_int(&self, arch: &str) -> Option<i128> {
        value_int(&self.ours, arch)
    }

    /// The donor value for `arch` as an integer. An architecture missing from a table means
    /// "no difference on that architecture", so our value is returned.
    #[must_use]
    pub fn theirs_int(&self, arch: &str) -> Option<i128> {
        match &self.theirs {
            toml::Value::Table(table) if !table.contains_key(arch) => self.ours_int(arch),
            _ => value_int(&self.theirs, arch),
        }
    }

    /// Our layout value for `arch` (`size/align`).
    #[must_use]
    pub fn ours_layout(&self, arch: &str) -> Option<(i64, i64)> {
        value_layout(&self.ours, arch)
    }

    /// The donor layout value for `arch` (`size/align`). An architecture missing from a table
    /// means "no difference on that architecture", so our value is returned.
    #[must_use]
    pub fn theirs_layout(&self, arch: &str) -> Option<(i64, i64)> {
        match &self.theirs {
            toml::Value::Table(table) if !table.contains_key(arch) => self.ours_layout(arch),
            _ => value_layout(&self.theirs, arch),
        }
    }
}

fn value_int(value: &toml::Value, arch: &str) -> Option<i128> {
    match value {
        toml::Value::Integer(number) => Some(i128::from(*number)),
        toml::Value::String(text) => crate::values::parse_int_literal(text).ok(),
        toml::Value::Table(table) => table.get(arch).and_then(|entry| match entry {
            toml::Value::Integer(number) => Some(i128::from(*number)),
            toml::Value::String(text) => crate::values::parse_int_literal(text).ok(),
            _ => None,
        }),
        _ => None,
    }
}

fn value_layout(value: &toml::Value, arch: &str) -> Option<(i64, i64)> {
    match value {
        toml::Value::String(text) => parse_layout(text),
        toml::Value::Table(table) => table.get(arch).and_then(|entry| match entry {
            toml::Value::String(text) => parse_layout(text),
            _ => None,
        }),
        _ => None,
    }
}

fn parse_layout(text: &str) -> Option<(i64, i64)> {
    let (size, align) = text.split_once('/')?;
    Some((size.trim().parse().ok()?, align.trim().parse().ok()?))
}

/// Render a `toml::Value` integer-ish entry for error messages.
#[cfg(test)]
#[must_use]
pub fn describe_value(value: &toml::Value) -> String {
    match value {
        toml::Value::Integer(number) => number.to_string(),
        toml::Value::String(text) => format!("\"{text}\""),
        toml::Value::Table(table) => format!("{table:?}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
