// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Table loading and every static rule of doc 14 4.3.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};

use crate::layout::{Layouts, compute_all, parse_field_type};
use crate::model::{
    ARCHES, BASES, ConstDef, ConstFile, DeviationFile, IntOrTable, RUST_TYPES, SUFFIXES,
    StructsFile, TypesFile, VISIBILITY,
};
use crate::values;

/// The fourteen constant groups, in generation order.
pub const GROUP_FILES: [&str; 14] = [
    "errno", "signal", "fcntl", "mman", "stat", "socket", "netinet", "poll", "time", "resource",
    "limits", "sysconf", "pthread", "misc",
];

/// One loaded constant group.
#[derive(Debug)]
pub struct Group {
    /// File stem and Rust module name.
    pub stem: String,
    /// The parsed file.
    pub def: ConstFile,
}

/// Every table of `libc/ravenlibc/abi`.
#[derive(Debug)]
pub struct Tables {
    /// `types.toml`.
    pub types: TypesFile,
    /// `structs.toml`.
    pub structs: StructsFile,
    /// The constant groups.
    pub groups: Vec<Group>,
    /// `deviations.toml`.
    pub deviations: DeviationFile,
}

/// One evaluated constant.
#[derive(Debug, Clone)]
pub struct Value {
    /// The evaluated integer.
    pub value: i128,
}

/// Evaluated constants of every group, keyed by name.
#[derive(Debug, Default)]
pub struct Values {
    /// Name -> value.
    pub by_name: BTreeMap<String, Value>,
}

fn load_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Read every table from `abi_dir` and validate it.
pub fn load(abi_dir: &Path) -> Result<Tables> {
    let types: TypesFile = load_toml(&abi_dir.join("types.toml"))?;
    let structs: StructsFile = load_toml(&abi_dir.join("structs.toml"))?;
    let deviations: DeviationFile = load_toml(&abi_dir.join("deviations.toml"))?;
    let mut groups = Vec::new();
    for stem in GROUP_FILES {
        let def: ConstFile = load_toml(&abi_dir.join(format!("{stem}.toml")))?;
        groups.push(Group {
            stem: stem.to_string(),
            def,
        });
    }
    let tables = Tables {
        types,
        structs,
        groups,
        deviations,
    };
    validate(&tables)?;
    Ok(tables)
}

/// Validate every static rule and evaluate all constants.
pub fn validate(tables: &Tables) -> Result<Values> {
    let mut problems = Vec::new();
    validate_types(tables, &mut problems);
    validate_structs(tables, &mut problems);
    validate_groups(tables, &mut problems);
    let values = evaluate_groups(tables, &mut problems);
    validate_deviations(tables, &values, &mut problems);
    if !problems.is_empty() {
        let mut message = String::from("abi tables are invalid:");
        for problem in &problems {
            message.push_str("\n  - ");
            message.push_str(problem);
        }
        bail!(message);
    }
    Ok(values)
}

fn validate_types(tables: &Tables, problems: &mut Vec<String>) {
    if tables.types.schema != 1 {
        problems.push(format!(
            "types.toml: schema {} is not 1",
            tables.types.schema
        ));
    }
    let mut names = BTreeSet::new();
    for ty in &tables.types.types {
        if !is_identifier(&ty.name) {
            problems.push(format!("types.toml: {:?} is not an identifier", ty.name));
        }
        if !names.insert(ty.name.clone()) {
            problems.push(format!("types.toml: duplicate type {:?}", ty.name));
        }
        if !matches!(ty.bits, 8 | 16 | 32 | 64) {
            problems.push(format!("types.toml: {} has bits {}", ty.name, ty.bits));
        }
        if let Some(expected) = rust_type_bits(&ty.rust)
            && expected != ty.bits
        {
            problems.push(format!(
                "types.toml: {} is {} bits but rust type {} is {expected}",
                ty.name, ty.bits, ty.rust
            ));
        }
        for arch in ty.per_arch.keys() {
            validate_arch(arch, "types.toml per_arch", problems);
        }
        if !ty.per_arch.is_empty() && !matches!(ty.name.as_str(), "wchar_t" | "char") {
            problems.push(format!(
                "types.toml: per_arch is only allowed for wchar_t and char, found on {}",
                ty.name
            ));
        }
        for arch in ty.deviates_musl.keys() {
            validate_arch(arch, "types.toml deviates_musl", problems);
        }
        for (arch, deviation) in &ty.deviates_musl {
            if deviation.bits == ty.bits {
                problems.push(format!(
                    "types.toml: {} deviates_musl on {arch} repeats our {} bits",
                    ty.name, ty.bits
                ));
            }
        }
        if !ty.deviates_musl.is_empty() && !matches!(ty.name.as_str(), "nlink_t" | "blksize_t") {
            problems.push(format!(
                "types.toml: deviates_musl is only documented for nlink_t and blksize_t, found on {}",
                ty.name
            ));
        }
    }
    for required in [
        "size_t",
        "ssize_t",
        "wchar_t",
        "wint_t",
        "time_t",
        "off_t",
        "mode_t",
        "nlink_t",
        "blksize_t",
        "pthread_t",
    ] {
        if !names.contains(required) {
            problems.push(format!("types.toml: required type {required} is missing"));
        }
    }
}

fn rust_type_bits(rust: &str) -> Option<u16> {
    Some(match rust {
        "i8" | "u8" => 8,
        "i16" | "u16" => 16,
        "i32" | "u32" | "f32" => 32,
        "i64" | "u64" | "f64" => 64,
        "isize" | "usize" | "*mut core::ffi::c_void" => 64,
        "core::ffi::c_char" => 8,
        _ => return None,
    })
}

fn validate_structs(tables: &Tables, problems: &mut Vec<String>) {
    if tables.structs.schema != 1 {
        problems.push(format!(
            "structs.toml: schema {} is not 1",
            tables.structs.schema
        ));
    }
    let mut names = BTreeSet::new();
    for record in &tables.structs.record {
        if !names.insert(record.name.clone()) {
            problems.push(format!("structs.toml: duplicate type {:?}", record.name));
        }
        if record.fields.is_empty() {
            problems.push(format!(
                "structs.toml: record {} has no fields",
                record.name
            ));
        }
        let mut field_names = BTreeSet::new();
        for field in &record.fields {
            if !field_names.insert(field.name.clone()) {
                problems.push(format!(
                    "structs.toml: record {} repeats field {}",
                    record.name, field.name
                ));
            }
            if let Err(error) = parse_field_type(&field.ty) {
                problems.push(format!(
                    "structs.toml: record {} field {}: {error}",
                    record.name, field.name
                ));
            }
        }
        for arch in record.deviates.keys() {
            validate_arch(arch, "structs.toml deviates", problems);
        }
    }
    for opaque in &tables.structs.opaque {
        if !names.insert(opaque.name.clone()) {
            problems.push(format!("structs.toml: duplicate type {:?}", opaque.name));
        }
        validate_int_or_table(&opaque.size, "size", &opaque.name, problems);
        validate_int_or_table(&opaque.align, "align", &opaque.name, problems);
    }
    // L1: every record must equal its computed repr(C) layout on every architecture.
    for arch in ARCHES {
        match compute_all(&tables.structs.record, &tables.structs.opaque, arch) {
            Ok(_) => {}
            Err(error) => problems.push(format!("structs.toml ({arch}): {error}")),
        }
    }
}

fn validate_int_or_table(value: &IntOrTable, field: &str, name: &str, problems: &mut Vec<String>) {
    match value {
        IntOrTable::Int(number) => {
            if *number <= 0 {
                problems.push(format!(
                    "structs.toml: {name} {field} {number} is not positive"
                ));
            }
        }
        IntOrTable::Table(table) => {
            for arch in ARCHES {
                if !table.contains_key(arch) {
                    problems.push(format!("structs.toml: {name} {field} lacks {arch}"));
                }
            }
            for arch in table.keys() {
                validate_arch(arch, "structs.toml opaque table", problems);
            }
        }
    }
}

fn validate_groups(tables: &Tables, problems: &mut Vec<String>) {
    let mut names: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    let mut headers = BTreeSet::new();
    let mut guards = BTreeSet::new();
    for group in &tables.groups {
        let def = &group.def;
        if def.schema != 1 {
            problems.push(format!(
                "{}.toml: schema {} is not 1",
                group.stem, def.schema
            ));
        }
        if def.group != group.stem {
            problems.push(format!(
                "{}.toml: group {:?} does not equal the file stem",
                group.stem, def.group
            ));
        }
        let expected_header = expected_header(&group.stem);
        if def.c_header != expected_header {
            problems.push(format!(
                "{}.toml: c_header {:?} must be {expected_header:?}",
                group.stem, def.c_header
            ));
        }
        if !headers.insert(def.c_header.clone()) {
            problems.push(format!("{}.toml: duplicate c_header", group.stem));
        }
        if !guards.insert(def.guard.clone()) {
            problems.push(format!("{}.toml: duplicate guard", group.stem));
        }
        if !RUST_TYPES.contains(&def.rust_type.as_str()) {
            problems.push(format!(
                "{}.toml: invalid rust_type {:?}",
                group.stem, def.rust_type
            ));
        }
        if !BASES.contains(&def.base.as_str()) {
            problems.push(format!("{}.toml: invalid base {:?}", group.stem, def.base));
        }
        for source in &def.source {
            if !is_relative(source) {
                problems.push(format!(
                    "{}.toml: source path {source:?} is not relative",
                    group.stem
                ));
            }
        }
        if def.prefixes.is_empty() {
            problems.push(format!("{}.toml: prefixes is empty", group.stem));
        }
        let mut omit_names = BTreeSet::new();
        for omit in &def.omit {
            if omit.reason.trim().is_empty() {
                problems.push(format!(
                    "{}.toml: omit {} has an empty reason",
                    group.stem, omit.name
                ));
            }
            if !omit_names.insert(omit.name.clone()) {
                problems.push(format!(
                    "{}.toml: omit {} is listed twice",
                    group.stem, omit.name
                ));
            }
        }
        for constant in &def.consts {
            validate_constant(group, constant, &omit_names, problems);
            if let Some(arches) = arch_set(&constant.arch) {
                names.entry(constant.name.clone()).or_default().push(arches);
            }
        }
    }
    for (name, sets) in &names {
        for (index, left) in sets.iter().enumerate() {
            for right in sets.iter().skip(index + 1) {
                if left.iter().any(|arch| right.contains(arch)) {
                    problems.push(format!(
                        "constant {name} is defined twice for an overlapping architecture set"
                    ));
                }
            }
        }
    }
    // Resolution: aliases and or-terms must exist and must not form a cycle.
    resolve_all(tables, problems);
}

fn validate_constant(
    group: &Group,
    constant: &ConstDef,
    omit: &BTreeSet<String>,
    problems: &mut Vec<String>,
) {
    let def = &group.def;
    if !is_identifier(&constant.name) {
        problems.push(format!(
            "{}.toml: {:?} is not an identifier",
            group.stem, constant.name
        ));
    }
    if omit.contains(&constant.name) {
        problems.push(format!(
            "{}.toml: {} is both a constant and an omit row",
            group.stem, constant.name
        ));
    }
    let forms = usize::from(constant.value.is_some())
        + usize::from(constant.alias.is_some())
        + usize::from(constant.or.is_some())
        + usize::from(constant.shl.is_some());
    if forms != 1 {
        problems.push(format!(
            "{}.toml: {} must have exactly one of value, alias, or, shl",
            group.stem, constant.name
        ));
    }
    let ty = constant.ty.as_deref().unwrap_or(&def.rust_type);
    if !RUST_TYPES.contains(&ty) {
        problems.push(format!(
            "{}.toml: {} has invalid type {ty:?}",
            group.stem, constant.name
        ));
    }
    let base = constant.base.as_deref().unwrap_or(&def.base);
    if !BASES.contains(&base) {
        problems.push(format!(
            "{}.toml: {} has invalid base {base:?}",
            group.stem, constant.name
        ));
    }
    if let Some(suffix) = &constant.suffix
        && !SUFFIXES.contains(&suffix.as_str())
    {
        problems.push(format!(
            "{}.toml: {} has invalid suffix {suffix:?}",
            group.stem, constant.name
        ));
    }
    if let Some(visible) = &constant.visible
        && !VISIBILITY.contains(&visible.as_str())
    {
        problems.push(format!(
            "{}.toml: {} has invalid visibility {visible:?}",
            group.stem, constant.name
        ));
    }
    for arch in &constant.arch {
        validate_arch(arch, "constant arch", problems);
    }
    if constant.or.as_ref().is_some_and(Vec::is_empty) {
        problems.push(format!(
            "{}.toml: {} has an empty or list",
            group.stem, constant.name
        ));
    }
    if let Some([_, right]) = constant.shl
        && !(0..=127).contains(&right)
    {
        problems.push(format!(
            "{}.toml: {} has an out-of-range shift {right}",
            group.stem, constant.name
        ));
    }
    if let Some(value) = &constant.value
        && !matches!(value, toml::Value::Integer(_) | toml::Value::String(_))
    {
        problems.push(format!(
            "{}.toml: {} has a non-scalar value",
            group.stem, constant.name
        ));
    }
    if let Some(value) = &constant.value
        && let Some(number) = value_integer(value)
    {
        if !values::fits(number, ty) {
            problems.push(format!(
                "{}.toml: {} value {number} does not fit {ty}",
                group.stem, constant.name
            ));
        }
        if number < 0 && matches!(ty, "u8" | "u16" | "u32" | "u64" | "usize") {
            problems.push(format!(
                "{}.toml: {} is negative but {ty} is unsigned",
                group.stem, constant.name
            ));
        }
    }
}

fn value_integer(value: &toml::Value) -> Option<i128> {
    match value {
        toml::Value::Integer(number) => Some(i128::from(*number)),
        toml::Value::String(text) => values::parse_int_literal(text).ok(),
        _ => None,
    }
}

fn resolve_all(tables: &Tables, problems: &mut Vec<String>) {
    let mut index: BTreeMap<&str, (&str, &ConstDef)> = BTreeMap::new();
    for group in &tables.groups {
        for constant in &group.def.consts {
            index.insert(constant.name.as_str(), (group.stem.as_str(), constant));
        }
    }
    for group in &tables.groups {
        for constant in &group.def.consts {
            if constant.alias.is_none() && constant.or.is_none() {
                continue;
            }
            for arch in ARCHES {
                if !available(constant, arch) {
                    continue;
                }
                let mut stack = Vec::new();
                if let Err(error) = resolve_one(constant, arch, &index, &mut stack) {
                    problems.push(format!("{}.toml: {}: {error}", group.stem, constant.name));
                }
            }
        }
    }
}

fn resolve_one(
    constant: &ConstDef,
    arch: &str,
    index: &BTreeMap<&str, (&str, &ConstDef)>,
    stack: &mut Vec<String>,
) -> Result<i128> {
    if stack.iter().any(|name| name == &constant.name) {
        bail!("alias cycle through {}", constant.name);
    }
    if !available(constant, arch) {
        bail!("{} is not available on {arch}", constant.name);
    }
    let mut resolver = |name: &str| -> Result<i128> {
        let Some((_, target)) = index.get(name) else {
            bail!("term {name} is not a constant of any group");
        };
        if !available(target, arch) {
            bail!("term {name} is not available on {arch}");
        }
        stack.push(constant.name.clone());
        let result = resolve_one(target, arch, index, stack);
        stack.pop();
        result
    };
    values::evaluate(constant, &mut resolver)
}

fn available(constant: &ConstDef, arch: &str) -> bool {
    constant.arch.is_empty() || constant.arch.iter().any(|candidate| candidate == arch)
}

fn arch_set(arches: &[String]) -> Option<Vec<String>> {
    if arches.is_empty() {
        Some(ARCHES.iter().map(|arch| (*arch).to_string()).collect())
    } else {
        Some(arches.to_vec())
    }
}

fn evaluate_groups(tables: &Tables, problems: &mut Vec<String>) -> Values {
    let mut index: BTreeMap<&str, (&str, &ConstDef)> = BTreeMap::new();
    for group in &tables.groups {
        for constant in &group.def.consts {
            index.insert(constant.name.as_str(), (group.stem.as_str(), constant));
        }
    }
    let mut result = Values::default();
    for group in &tables.groups {
        for constant in &group.def.consts {
            let mut evaluated: Option<i128> = None;
            let mut failed = false;
            for arch in ARCHES {
                if !available(constant, arch) {
                    continue;
                }
                let mut stack = Vec::new();
                match resolve_one(constant, arch, &index, &mut stack) {
                    Ok(value) => {
                        if let Some(previous) = evaluated
                            && previous != value
                        {
                            problems.push(format!(
                                "{}.toml: {} evaluates differently per architecture",
                                group.stem, constant.name
                            ));
                            failed = true;
                        }
                        evaluated = Some(value);
                    }
                    Err(error) => {
                        problems.push(format!("{}.toml: {}: {error}", group.stem, constant.name));
                        failed = true;
                    }
                }
            }
            if let (Some(value), false) = (evaluated, failed) {
                let ty = constant.ty.as_deref().unwrap_or(&group.def.rust_type);
                if !values::fits(value, ty) {
                    problems.push(format!(
                        "{}.toml: {} evaluates to {value}, which does not fit {ty}",
                        group.stem, constant.name
                    ));
                }
                if value < 0 && matches!(ty, "u8" | "u16" | "u32" | "u64" | "usize") {
                    problems.push(format!(
                        "{}.toml: {} is negative but {ty} is unsigned",
                        group.stem, constant.name
                    ));
                }
                result
                    .by_name
                    .insert(constant.name.clone(), Value { value });
            }
        }
    }
    result
}

fn validate_deviations(tables: &Tables, values: &Values, problems: &mut Vec<String>) {
    if tables.deviations.schema != 1 {
        problems.push(format!(
            "deviations.toml: schema {} is not 1",
            tables.deviations.schema
        ));
    }
    let kinds = type_kinds(tables);
    for row in &tables.deviations.deviation {
        if !matches!(row.against.as_str(), "musl" | "linux-abi") {
            problems.push(format!(
                "deviations.toml: {} has invalid against {:?}",
                row.name, row.against
            ));
        }
        if row.reason.trim().is_empty() {
            problems.push(format!("deviations.toml: {} has an empty reason", row.name));
        }
        let Some(kind) = kinds.get(&row.name) else {
            problems.push(format!(
                "deviations.toml: {} is not a constant, type, record or opaque",
                row.name
            ));
            continue;
        };
        match kind {
            Kind::Constant => {
                if !values.by_name.contains_key(&row.name) {
                    problems.push(format!(
                        "deviations.toml: constant {} is not defined",
                        row.name
                    ));
                }
                for arch in ARCHES {
                    if row.ours_int(arch).is_none() || row.theirs_int(arch).is_none() {
                        problems.push(format!(
                            "deviations.toml: constant row {} needs integer ours/theirs for {arch}",
                            row.name
                        ));
                        break;
                    }
                }
            }
            Kind::Layout => {
                for arch in ARCHES {
                    if row.ours_layout(arch).is_none() {
                        problems.push(format!(
                            "deviations.toml: layout row {} needs an ours size/align for {arch}",
                            row.name
                        ));
                        break;
                    }
                }
            }
        }
    }
}

/// What kind of table entry a deviation names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A constant entry.
    Constant,
    /// A type, record or opaque.
    Layout,
}

/// Classify every name that a deviation may name.
#[must_use]
pub fn type_kinds(tables: &Tables) -> BTreeMap<String, Kind> {
    let mut kinds = BTreeMap::new();
    for ty in &tables.types.types {
        kinds.insert(ty.name.clone(), Kind::Layout);
    }
    for record in &tables.structs.record {
        kinds.insert(record.name.clone(), Kind::Layout);
    }
    for opaque in &tables.structs.opaque {
        kinds.insert(opaque.name.clone(), Kind::Layout);
    }
    for group in &tables.groups {
        for constant in &group.def.consts {
            kinds.insert(constant.name.clone(), Kind::Constant);
        }
    }
    kinds
}

/// The header path decided in doc 14 4.5.
#[must_use]
pub fn expected_header(stem: &str) -> String {
    match stem {
        "errno" => "bits/errno.h".to_string(),
        "signal" => "bits/signum.h".to_string(),
        "sysconf" => "bits/sysconf.h".to_string(),
        other => format!("bits/abi_{other}.h"),
    }
}

/// The Rust type name for a record or opaque.
#[must_use]
pub fn rust_name<'a>(tables: &'a Tables, name: &str) -> Option<&'a str> {
    tables
        .structs
        .record
        .iter()
        .find(|record| record.name == name)
        .map(|record| record.rust.as_str())
        .or_else(|| {
            tables
                .structs
                .opaque
                .iter()
                .find(|opaque| opaque.name == name)
                .map(|opaque| opaque.rust.as_str())
        })
}

/// The computed layouts for `arch`, with any error propagated.
pub fn layouts(tables: &Tables, arch: &str) -> Result<Layouts> {
    compute_all(&tables.structs.record, &tables.structs.opaque, arch)
}

/// Whether the type exists in `types.toml`.
#[cfg(test)]
#[must_use]
pub fn type_def<'a>(tables: &'a Tables, name: &str) -> Option<&'a crate::model::TypeDef> {
    tables.types.types.iter().find(|ty| ty.name == name)
}

/// The field type of a record field, parsed.
#[cfg(test)]
pub fn field_type(
    tables: &Tables,
    record_name: &str,
    field_name: &str,
) -> Result<crate::layout::FieldType> {
    let record = tables
        .structs
        .record
        .iter()
        .find(|record| record.name == record_name)
        .with_context(|| format!("record {record_name} does not exist"))?;
    let field = record
        .fields
        .iter()
        .find(|candidate| candidate.name == field_name)
        .with_context(|| format!("record {} has no field {field_name}", record.name))?;
    parse_field_type(&field.ty)
}

fn validate_arch(arch: &str, what: &str, problems: &mut Vec<String>) {
    if !ARCHES.contains(&arch) {
        problems.push(format!("{what}: unknown architecture {arch:?}"));
    }
}

fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_relative(path: &str) -> bool {
    let path = Path::new(path);
    path.is_relative()
        && path
            .components()
            .all(|component| !matches!(component, Component::ParentDir))
}

#[cfg(test)]
#[path = "validate_ut.rs"]
mod tests;
