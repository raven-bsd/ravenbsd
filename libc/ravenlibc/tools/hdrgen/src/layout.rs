// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `repr(C)` layout arithmetic over the TOML field lists (verification leg L1).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use crate::model::{IntOrTable, OpaqueDef, RecordDef};

/// A parsed field type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// A fixed-width integer (`i32`, `u64`, ...).
    Int(&'static str),
    /// A data pointer (`void *` in C).
    Ptr,
    /// The C `char` type.
    CChar,
    /// A Rust-like array `[T; N]`.
    Array(Box<FieldType>, usize),
    /// Another record or opaque type of the same file.
    Named(String),
}

/// The size and alignment of one type, plus whether the C spelling needs `struct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeLayout {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
    /// True for a record; false for a scalar or opaque typedef.
    pub is_record: bool,
}

/// A computed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedField {
    /// Field name.
    pub name: String,
    /// Byte offset.
    pub offset: i64,
    /// Size in bytes.
    pub size: i64,
    /// The parsed type.
    pub ty: FieldType,
}

/// A computed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedRecord {
    /// Record name.
    pub name: String,
    /// Computed size.
    pub size: i64,
    /// Computed alignment.
    pub align: i64,
    /// Fields in declaration order.
    pub fields: Vec<ComputedField>,
}

/// Parse the field type grammar.
pub fn parse_field_type(text: &str) -> Result<FieldType> {
    let text = text.trim();
    match text {
        "i8" => Ok(FieldType::Int("i8")),
        "u8" => Ok(FieldType::Int("u8")),
        "i16" => Ok(FieldType::Int("i16")),
        "u16" => Ok(FieldType::Int("u16")),
        "i32" => Ok(FieldType::Int("i32")),
        "u32" => Ok(FieldType::Int("u32")),
        "i64" => Ok(FieldType::Int("i64")),
        "u64" => Ok(FieldType::Int("u64")),
        "ptr" => Ok(FieldType::Ptr),
        "c_char" => Ok(FieldType::CChar),
        other => {
            if let Some(inner) = other
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                let (element, length) = inner
                    .rsplit_once(';')
                    .ok_or_else(|| anyhow::anyhow!("array type {other:?} lacks `; N`"))?;
                let element = parse_field_type(element.trim())?;
                let length: usize = length
                    .trim()
                    .parse()
                    .with_context(|| format!("array length in {other:?}"))?;
                if length == 0 {
                    bail!("array type {other:?} has zero length");
                }
                return Ok(FieldType::Array(Box::new(element), length));
            }
            if other.len() > 1
                && matches!(other.as_bytes()[0], b'i' | b'u')
                && other[1..].bytes().all(|byte| byte.is_ascii_digit())
            {
                bail!("unknown field type {other:?}");
            }
            if is_identifier(other) {
                Ok(FieldType::Named(other.to_string()))
            } else {
                bail!("unknown field type {other:?}")
            }
        }
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

/// The scalar layout of a fixed-width integer field.
#[must_use]
pub fn int_layout(ty: &str) -> TypeLayout {
    let size = match ty {
        "i8" | "u8" => 1,
        "i16" | "u16" => 2,
        "i32" | "u32" => 4,
        "i64" | "u64" => 8,
        _ => 1,
    };
    TypeLayout {
        size,
        align: size,
        is_record: false,
    }
}

/// The unsigned Rust integer of `min(align, 8)` bytes used inside an opaque blob.
#[must_use]
pub fn opaque_element(align: i64) -> (&'static str, i64) {
    match align {
        0 | 1 => ("u8", 1),
        2 => ("u16", 2),
        3 | 4 => ("u32", 4),
        _ => ("u64", 8),
    }
}

/// A per-architecture opaque layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpaqueLayout {
    /// Size in bytes.
    pub size: i64,
    /// Alignment in bytes.
    pub align: i64,
}

/// Resolve one opaque definition for `arch`, validating the blob arithmetic.
pub fn opaque_layout(def: &OpaqueDef, arch: &str) -> Result<OpaqueLayout> {
    let size = def
        .size
        .get(arch)
        .with_context(|| format!("opaque {} has no size for {arch}", def.name))?;
    let align = def
        .align
        .get(arch)
        .with_context(|| format!("opaque {} has no align for {arch}", def.name))?;
    if size <= 0 {
        bail!("opaque {} has a non-positive size {size}", def.name);
    }
    if align <= 0 || !(align & (align - 1) == 0) {
        bail!("opaque {} has an invalid alignment {align}", def.name);
    }
    if size % align != 0 {
        bail!(
            "opaque {}: size {size} is not a multiple of align {align}",
            def.name
        );
    }
    let (_, element) = opaque_element(align);
    if size % element != 0 {
        bail!(
            "opaque {}: size {size} is not a multiple of element size {element}",
            def.name
        );
    }
    Ok(OpaqueLayout { size, align })
}

/// Whether an opaque has the same size and alignment on every architecture.
pub fn opaque_is_arch_independent(def: &OpaqueDef) -> Result<bool> {
    let mut sizes = BTreeSet::new();
    let mut aligns = BTreeSet::new();
    for arch in crate::model::ARCHES {
        let layout = opaque_layout(def, arch)?;
        sizes.insert(layout.size);
        aligns.insert(layout.align);
    }
    Ok(sizes.len() == 1 && aligns.len() == 1)
}

/// All named type layouts, keyed by the TOML `name`.
#[derive(Debug, Default)]
pub struct Layouts {
    /// Opaque layouts per architecture.
    pub opaque: BTreeMap<String, OpaqueLayout>,
    /// Record layouts.
    pub records: BTreeMap<String, TypeLayout>,
    /// Computed records in first-definition order.
    pub computed: BTreeMap<String, ComputedRecord>,
}

/// Compute every record layout and validate the stated offsets, sizes and alignments.
pub fn compute_all(records: &[RecordDef], opaques: &[OpaqueDef], arch: &str) -> Result<Layouts> {
    let mut layouts = Layouts::default();
    for opaque in opaques {
        let layout = opaque_layout(opaque, arch)?;
        layouts.opaque.insert(opaque.name.clone(), {
            let _ = layout;
            layout
        });
    }
    let by_name: BTreeMap<&str, &RecordDef> = records
        .iter()
        .map(|record| (record.name.as_str(), record))
        .collect();
    let mut visiting = BTreeSet::new();
    for record in records {
        compute_record(record, &by_name, opaques, &mut layouts, &mut visiting)?;
    }
    Ok(layouts)
}

fn compute_record(
    record: &RecordDef,
    by_name: &BTreeMap<&str, &RecordDef>,
    opaques: &[OpaqueDef],
    layouts: &mut Layouts,
    visiting: &mut BTreeSet<String>,
) -> Result<TypeLayout> {
    if let Some(layout) = layouts.records.get(&record.name) {
        return Ok(*layout);
    }
    if !visiting.insert(record.name.clone()) {
        bail!("record {} embeds itself", record.name);
    }
    let mut cursor = 0i64;
    let mut max_align = 1i64;
    let mut fields = Vec::new();
    for field in &record.fields {
        let ty = parse_field_type(&field.ty)
            .with_context(|| format!("record {} field {}", record.name, field.name))?;
        let layout = field_layout(&ty, record, by_name, opaques, layouts, visiting)?;
        let offset = round_up(cursor, layout.align);
        if field.off != offset {
            bail!(
                "record {} field {}: stated offset {} but repr(C) computes {offset}",
                record.name,
                field.name,
                field.off
            );
        }
        cursor = offset
            .checked_add(layout.size)
            .with_context(|| format!("record {} field {} overflows", record.name, field.name))?;
        max_align = max_align.max(layout.align);
        fields.push(ComputedField {
            name: field.name.clone(),
            offset,
            size: layout.size,
            ty,
        });
    }
    let size = round_up(cursor, max_align);
    if record.size != size {
        bail!(
            "record {}: stated size {} but repr(C) computes {size}",
            record.name,
            record.size
        );
    }
    if record.align != max_align {
        bail!(
            "record {}: stated align {} but repr(C) computes {max_align}",
            record.name,
            record.align
        );
    }
    let layout = TypeLayout {
        size,
        align: max_align,
        is_record: true,
    };
    layouts.records.insert(record.name.clone(), layout);
    layouts.computed.insert(
        record.name.clone(),
        ComputedRecord {
            name: record.name.clone(),
            size,
            align: max_align,
            fields,
        },
    );
    visiting.remove(&record.name);
    Ok(layout)
}

fn field_layout(
    ty: &FieldType,
    record: &RecordDef,
    by_name: &BTreeMap<&str, &RecordDef>,
    opaques: &[OpaqueDef],
    layouts: &mut Layouts,
    visiting: &mut BTreeSet<String>,
) -> Result<TypeLayout> {
    Ok(match ty {
        FieldType::Int(name) => int_layout(name),
        FieldType::Ptr => TypeLayout {
            size: 8,
            align: 8,
            is_record: false,
        },
        FieldType::CChar => TypeLayout {
            size: 1,
            align: 1,
            is_record: false,
        },
        FieldType::Array(element, length) => {
            let element = field_layout(element, record, by_name, opaques, layouts, visiting)?;
            let length = i64::try_from(*length).context("array length")?;
            TypeLayout {
                size: element
                    .size
                    .checked_mul(length)
                    .with_context(|| format!("array in record {} overflows", record.name))?,
                align: element.align,
                is_record: false,
            }
        }
        FieldType::Named(name) => {
            if let Some(nested) = by_name.get(name.as_str()) {
                compute_record(nested, by_name, opaques, layouts, visiting)?
            } else if let Some(opaque) = opaques.iter().find(|opaque| opaque.name == *name) {
                if !opaque_is_arch_independent(opaque)? {
                    bail!(
                        "record {} embeds per-architecture opaque {name}; only arch-independent \
                         opaques are allowed inside records",
                        record.name
                    );
                }
                let layout = opaque_layout(opaque, crate::model::ARCHES[0])?;
                TypeLayout {
                    size: layout.size,
                    align: layout.align,
                    is_record: false,
                }
            } else {
                bail!(
                    "record {} field type {name} is neither a record nor an opaque of the file",
                    record.name
                );
            }
        }
    })
}

fn round_up(value: i64, align: i64) -> i64 {
    if align <= 1 {
        value
    } else {
        (value + align - 1) / align * align
    }
}

/// Render one C member declaration for the L2 probe.
#[must_use]
pub fn c_member(ty: &FieldType, name: &str, is_record: &dyn Fn(&str) -> bool) -> String {
    match ty {
        FieldType::Int(ty) => format!("{} {name}", c_int_name(ty)),
        FieldType::Ptr => format!("void *{name}"),
        FieldType::CChar => format!("char {name}"),
        FieldType::Array(element, length) => {
            let inner = c_member(element, name, is_record);
            // Move the array bound after the member name: `unsigned char d_name[256]`.
            format!("{inner}[{length}]")
        }
        FieldType::Named(nested) => {
            if is_record(nested) {
                format!("struct __ab_{nested} {name}")
            } else {
                format!("__ab_{nested} {name}")
            }
        }
    }
}

/// The C spelling of a fixed-width integer field.
#[must_use]
pub fn c_int_name(ty: &str) -> &'static str {
    match ty {
        "i8" => "signed char",
        "u8" => "unsigned char",
        "i16" => "short",
        "u16" => "unsigned short",
        "i32" => "int",
        "u32" => "unsigned int",
        "i64" => "long",
        "u64" => "unsigned long",
        _ => "int",
    }
}

/// Render the C blob definition used by the L2 probe for an opaque type.
#[must_use]
pub fn c_opaque_blob(def: &OpaqueDef, layout: OpaqueLayout) -> String {
    format!(
        "typedef struct {{ _Alignas({}) unsigned char __bytes[{}]; }} __ab_{};",
        layout.align, layout.size, def.name
    )
}

/// The Rust blob type for an opaque type on `arch`.
pub fn rust_opaque_body(def: &OpaqueDef, arch: &str) -> Result<(&'static str, i64, i64, i64)> {
    let layout = opaque_layout(def, arch)?;
    let (element, element_size) = opaque_element(layout.align);
    let count = layout.size / element_size;
    Ok((element, count, layout.size, layout.align))
}

impl IntOrTable {
    /// Whether this value is a table with differing entries.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        match self {
            Self::Int(_) => true,
            Self::Table(table) => {
                let mut values = table.values();
                let Some(first) = values.next() else {
                    return false;
                };
                values.all(|value| value == first)
            }
        }
    }
}

#[cfg(test)]
#[path = "layout_ut.rs"]
mod tests;
