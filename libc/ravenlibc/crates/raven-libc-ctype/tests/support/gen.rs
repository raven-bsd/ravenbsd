// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unicode 15.1.0 table generator shared by `examples/gen_unicode.rs` and `tables_fresh`.
//!
//! The generator reads `UnicodeData.txt`, `DerivedCoreProperties.txt` and `EastAsianWidth.txt`
//! from the vendored UCD directory and writes the Rust statics of `src/unicode/tables.rs`.
//! It is deterministic: the output depends only on the six vendored files and this source.

use std::fmt::Write;
use std::fs;
use std::path::Path;

const MAX_CP: usize = 0x11_0000;
const BLOCK_BITS: usize = 8;
const BLOCK_LEN: usize = 1 << BLOCK_BITS;
const PROPS_STAGE1_LEN: usize = MAX_CP >> BLOCK_BITS;
const CASE_STAGE1_LEN: usize = 0x2_0000 >> BLOCK_BITS;
const TABLE_BUDGET: usize = 48 * 1024;

const ALPHA: u8 = 1;
const PUNCT: u8 = 2;
const WIDE: u8 = 4;
const ZERO: u8 = 8;

/// Generated file and the statistics the example prints.
pub struct Generated {
    /// The complete Rust source.
    pub text: String,
    /// Distinct property blocks.
    pub props_blocks: usize,
    /// Bytes of `PROPS_STAGE2`.
    pub props_stage2_bytes: usize,
    /// Distinct case blocks.
    pub case_blocks: usize,
    /// Distinct case pairs after index 0.
    pub case_pairs: usize,
    /// Sum of the sizes of all generated statics.
    pub table_bytes: usize,
    /// FNV-1a 64 of each input file, in the ticket's order.
    pub hashes: Vec<(&'static str, u64)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gc {
    Cn,
    Cc,
    Cs,
    Co,
    Nd,
    Mn,
    Me,
    Cf,
    No,
    P,
    S,
    M,
    Other,
}

fn category(name: &str) -> Gc {
    match name.as_bytes() {
        b"Cn" => Gc::Cn,
        b"Cc" => Gc::Cc,
        b"Cs" => Gc::Cs,
        b"Co" => Gc::Co,
        b"Nd" => Gc::Nd,
        b"Mn" => Gc::Mn,
        b"Me" => Gc::Me,
        b"Cf" => Gc::Cf,
        b"No" => Gc::No,
        _ => match name.as_bytes().first() {
            Some(b'P') => Gc::P,
            Some(b'S') => Gc::S,
            Some(b'M') => Gc::M,
            _ => Gc::Other,
        },
    }
}

fn parse_hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 16).ok()
}

fn parse_range(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    if let Some((start, end)) = text.split_once("..") {
        Some((parse_hex(start)?, parse_hex(end)?))
    } else {
        let value = parse_hex(text)?;
        Some((value, value))
    }
}

fn apply_code_range(text: &str, mut apply: impl FnMut(u32)) -> Result<(), String> {
    let Some((start, end)) = parse_range(text) else {
        return Err(format!("bad code point range: {text}"));
    };
    if end < start || end as usize >= MAX_CP {
        return Err(format!("range out of bounds: {start:04X}..{end:04X}"));
    }
    for cp in start..=end {
        apply(cp);
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct Ucd {
    gc: Vec<Gc>,
    alphabetic: Vec<bool>,
    wide: Vec<bool>,
    upper: Vec<i32>,
    lower: Vec<i32>,
}

fn parse_unicode_data(bytes: &[u8], ucd: &mut Ucd) -> Result<(), String> {
    let text = String::from_utf8_lossy(bytes);
    let mut pending: Option<(u32, Gc)> = None;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() < 15 {
            return Err(format!(
                "UnicodeData line has {} fields: {line}",
                fields.len()
            ));
        }
        let Some(cp) = parse_hex(fields[0]) else {
            return Err(format!("bad code point: {}", fields[0]));
        };
        if cp as usize >= MAX_CP {
            return Err(format!("code point out of range: {}", fields[0]));
        }
        let name = fields[1];
        let gc = category(fields[2]);
        if name.ends_with(", First>") {
            pending = Some((cp, gc));
            continue;
        }
        if name.ends_with(", Last>") {
            let Some((start, category_first)) = pending.take() else {
                return Err(format!("range end without start at U+{cp:04X}"));
            };
            if category_first != gc || cp < start {
                return Err(format!("inconsistent range at U+{cp:04X}"));
            }
            for code in start..=cp {
                let Some(slot) = ucd.gc.get_mut(code as usize) else {
                    return Err("UnicodeData range out of bounds".to_string());
                };
                *slot = gc;
            }
            continue;
        }
        if let Some((start, gc_first)) = pending.take()
            && let Some(slot) = ucd.gc.get_mut(start as usize)
        {
            *slot = gc_first;
        }
        let Some(slot) = ucd.gc.get_mut(cp as usize) else {
            return Err("UnicodeData code point out of bounds".to_string());
        };
        *slot = gc;
        if let Some(upper) = fields.get(12).and_then(|text| parse_hex(text)) {
            ucd.upper[cp as usize] = upper as i32 - cp as i32;
        }
        if let Some(lower) = fields.get(13).and_then(|text| parse_hex(text)) {
            ucd.lower[cp as usize] = lower as i32 - cp as i32;
        }
    }
    if let Some((start, gc)) = pending
        && let Some(slot) = ucd.gc.get_mut(start as usize)
    {
        *slot = gc;
    }
    Ok(())
}

fn parse_alphabetic(bytes: &[u8], ucd: &mut Ucd) -> Result<usize, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut applied = 0usize;
    for line in text.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((range, property)) = data.split_once(';') else {
            return Err(format!("bad DerivedCoreProperties line: {line}"));
        };
        if property.trim() != "Alphabetic" {
            continue;
        }
        apply_code_range(range, |cp| {
            ucd.alphabetic[cp as usize] = true;
            applied += 1;
        })?;
    }
    Ok(applied)
}

fn parse_wide(bytes: &[u8], ucd: &mut Ucd) -> Result<usize, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut applied = 0usize;
    for line in text.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if data.is_empty() {
            continue;
        }
        let Some((range, width)) = data.split_once(';') else {
            return Err(format!("bad EastAsianWidth line: {line}"));
        };
        let width = width.trim();
        if width != "W" && width != "F" {
            continue;
        }
        apply_code_range(range, |cp| {
            ucd.wide[cp as usize] = true;
            applied += 1;
        })?;
    }
    Ok(applied)
}

fn property_nibble(cp: u32, ucd: &Ucd) -> u8 {
    let gc = ucd.gc[cp as usize];
    let alphabet = ucd.alphabetic[cp as usize];
    let mut bits = 0u8;
    let digit = gc == Gc::Nd && !(0x30..=0x39).contains(&cp);
    if alphabet || digit {
        bits |= ALPHA;
    }
    let no_break = matches!(cp, 0x00a0 | 0x00ad | 0x2007 | 0x202f);
    let mark = matches!(gc, Gc::Mn | Gc::Me | Gc::M);
    if matches!(gc, Gc::P | Gc::S) || no_break || (mark && !alphabet) || (gc == Gc::No && !alphabet)
    {
        bits |= PUNCT;
    }
    if ucd.wide[cp as usize] {
        bits |= WIDE;
    }
    let zero_gc = matches!(gc, Gc::Mn | Gc::Me | Gc::Cf);
    let zero_extra = (0x1160..=0x11ff).contains(&cp)
        || (0x200b..=0x200f).contains(&cp)
        || (0x2060..=0x2064).contains(&cp);
    if (zero_gc && cp != 0x00ad) || zero_extra {
        bits |= ZERO;
    }
    bits
}

fn dedup_blocks(blocks: &[Vec<u8>]) -> Result<(Vec<u8>, Vec<u8>, usize), String> {
    let mut unique: Vec<&[u8]> = Vec::new();
    let mut stage1 = Vec::with_capacity(blocks.len());
    let mut stage2 = Vec::new();
    for block in blocks {
        let position =
            if let Some(position) = unique.iter().position(|known| *known == block.as_slice()) {
                position
            } else {
                if unique.len() > u8::MAX as usize {
                    return Err(format!("block count {} does not fit u8", unique.len() + 1));
                }
                unique.push(block.as_slice());
                stage2.extend_from_slice(block);
                unique.len() - 1
            };
        stage1.push(position as u8);
    }
    Ok((stage1, stage2, unique.len()))
}

fn emit_array<W: Write>(out: &mut W, name: &str, ty: &str, values: &[u8]) -> Result<(), String> {
    writeln!(out, "#[rustfmt::skip]").map_err(|error| error.to_string())?;
    writeln!(
        out,
        "pub(crate) static {name}: [{ty}; {}] = [",
        values.len()
    )
    .map_err(|error| error.to_string())?;
    for row in values.chunks(16) {
        let line: Vec<String> = row.iter().map(|value| format!("{value}")).collect();
        writeln!(out, "    {},", line.join(", ")).map_err(|error| error.to_string())?;
    }
    writeln!(out, "];").map_err(|error| error.to_string())?;
    Ok(())
}

fn emit_pairs<W: Write>(out: &mut W, pairs: &[(i32, i32)]) -> Result<(), String> {
    writeln!(out, "#[rustfmt::skip]").map_err(|error| error.to_string())?;
    writeln!(
        out,
        "pub(crate) static CASE_PAIRS: [(i32, i32); {}] = [",
        pairs.len()
    )
    .map_err(|error| error.to_string())?;
    for (upper, lower) in pairs {
        writeln!(out, "    ({upper}, {lower}),").map_err(|error| error.to_string())?;
    }
    writeln!(out, "];").map_err(|error| error.to_string())?;
    Ok(())
}

fn emit_banner<W: Write>(out: &mut W, ucd: &Path, hashes: &[(&str, u64)]) -> Result<(), String> {
    writeln!(
        out,
        "// @generated by examples/gen_unicode.rs from xtask/data/ucd-15.1.0, DO NOT EDIT"
    )
    .map_err(|error| error.to_string())?;
    writeln!(out, "// SPDX-License-Identifier: BSD-3-Clause").map_err(|error| error.to_string())?;
    writeln!(
        out,
        "// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors"
    )
    .map_err(|error| error.to_string())?;
    for (name, _) in hashes {
        let bytes = read(&ucd.join(name))?;
        let text = String::from_utf8_lossy(&bytes);
        writeln!(out, "//").map_err(|error| error.to_string())?;
        writeln!(out, "// input: {name}").map_err(|error| error.to_string())?;
        for line in text.lines().take(3) {
            if line.is_empty() {
                writeln!(out, "//").map_err(|error| error.to_string())?;
            } else {
                writeln!(out, "// {line}").map_err(|error| error.to_string())?;
            }
        }
    }
    writeln!(out, "//").map_err(|error| error.to_string())?;
    for (name, hash) in hashes {
        writeln!(out, "// fnv1a64({name}) = 0x{hash:016x}").map_err(|error| error.to_string())?;
    }
    writeln!(out, "//").map_err(|error| error.to_string())?;
    let license = read(&ucd.join("unicode-license.txt"))?;
    let text = String::from_utf8_lossy(&license);
    for line in text.trim_start_matches('\u{feff}').lines() {
        if line.is_empty() {
            writeln!(out, "//").map_err(|error| error.to_string())?;
        } else {
            writeln!(out, "// {line}").map_err(|error| error.to_string())?;
        }
    }
    writeln!(out).map_err(|error| error.to_string())?;
    Ok(())
}

/// Generate `tables.rs` from the vendored UCD directory.
///
/// # Errors
///
/// A missing or malformed input, an index type that would overflow, or a table over the 48 KiB
/// budget.
pub fn generate(ucd: &Path) -> Result<Generated, String> {
    let files = [
        "UnicodeData.txt",
        "DerivedCoreProperties.txt",
        "EastAsianWidth.txt",
        "PropList.txt",
        "emoji-data.txt",
        "unicode-license.txt",
    ];
    let mut contents = Vec::new();
    let mut hashes = Vec::new();
    for name in files {
        let bytes = read(&ucd.join(name))?;
        hashes.push((name, fnv1a64(&bytes)));
        contents.push(bytes);
    }

    let mut data = Ucd {
        gc: vec![Gc::Cn; MAX_CP],
        alphabetic: vec![false; MAX_CP],
        wide: vec![false; MAX_CP],
        upper: vec![0; MAX_CP],
        lower: vec![0; MAX_CP],
    };
    parse_unicode_data(&contents[0], &mut data)?;
    let alphabetic = parse_alphabetic(&contents[1], &mut data)?;
    let wide = parse_wide(&contents[2], &mut data)?;

    let mut prop_blocks = Vec::with_capacity(PROPS_STAGE1_LEN);
    for block in 0..PROPS_STAGE1_LEN {
        let mut packed = vec![0u8; BLOCK_LEN / 2];
        for (half, slot) in packed.iter_mut().enumerate() {
            let even = (block * BLOCK_LEN + half * 2) as u32;
            let odd = even + 1;
            let low = property_nibble(even, &data);
            let high = property_nibble(odd, &data);
            *slot = low | (high << 4);
        }
        prop_blocks.push(packed);
    }
    let (props_stage1, props_stage2, props_blocks) = dedup_blocks(&prop_blocks)?;
    if props_stage1.len() != PROPS_STAGE1_LEN {
        return Err("property stage1 length mismatch".to_string());
    }

    let mut pairs: Vec<(i32, i32)> = vec![(0, 0)];
    let mut pair_index = vec![0u8; 0x2_0000];
    for (cp, slot) in pair_index.iter_mut().enumerate() {
        let pair = (data.upper[cp], data.lower[cp]);
        if pair == (0, 0) {
            continue;
        }
        let position = if let Some(position) = pairs.iter().position(|known| *known == pair) {
            position
        } else {
            if pairs.len() > u8::MAX as usize {
                return Err(format!(
                    "case pair count {} does not fit u8",
                    pairs.len() + 1
                ));
            }
            pairs.push(pair);
            pairs.len() - 1
        };
        *slot = position as u8;
    }
    let mut case_blocks = Vec::with_capacity(CASE_STAGE1_LEN);
    for block in 0..CASE_STAGE1_LEN {
        case_blocks.push(pair_index[block * BLOCK_LEN..(block + 1) * BLOCK_LEN].to_vec());
    }
    let (case_stage1, case_stage2, case_block_count) = dedup_blocks(&case_blocks)?;
    if case_stage1.len() != CASE_STAGE1_LEN {
        return Err("case stage1 length mismatch".to_string());
    }

    let table_bytes = props_stage1.len()
        + props_stage2.len()
        + case_stage1.len()
        + case_stage2.len()
        + pairs.len() * core::mem::size_of::<(i32, i32)>();
    if table_bytes > TABLE_BUDGET {
        return Err(format!(
            "TABLE_BYTES {table_bytes} exceeds the {TABLE_BUDGET}-byte budget"
        ));
    }

    let mut out = String::new();
    emit_banner(&mut out, ucd, &hashes)?;
    writeln!(
        out,
        "// statistics: alphabetic ranges {alphabetic}, wide ranges {wide}"
    )
    .map_err(|error| error.to_string())?;
    writeln!(
        out,
        "// property blocks {props_blocks}, case blocks {case_block_count}, case pairs {}",
        pairs.len()
    )
    .map_err(|error| error.to_string())?;
    writeln!(out, "// TABLE_BYTES {table_bytes}").map_err(|error| error.to_string())?;
    writeln!(out).map_err(|error| error.to_string())?;
    writeln!(
        out,
        "//! Generated Unicode 15.1.0 property and case tables."
    )
    .map_err(|error| error.to_string())?;
    writeln!(out).map_err(|error| error.to_string())?;
    emit_array(&mut out, "PROPS_STAGE1", "u8", &props_stage1)?;
    emit_array(&mut out, "PROPS_STAGE2", "u8", &props_stage2)?;
    emit_array(&mut out, "CASE_STAGE1", "u8", &case_stage1)?;
    emit_array(&mut out, "CASE_STAGE2", "u8", &case_stage2)?;
    emit_pairs(&mut out, &pairs)?;
    writeln!(out, "/// Exact size of all generated statics; `<= 48 KiB`.")
        .map_err(|error| error.to_string())?;
    writeln!(out, "pub const TABLE_BYTES: usize = {table_bytes};")
        .map_err(|error| error.to_string())?;

    Ok(Generated {
        text: out,
        props_blocks,
        props_stage2_bytes: props_stage2.len(),
        case_blocks: case_block_count,
        case_pairs: pairs.len(),
        table_bytes,
        hashes,
    })
}
