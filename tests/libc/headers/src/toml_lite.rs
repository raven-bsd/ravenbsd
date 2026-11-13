// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! A deliberately small TOML scanner for the header test data files.
//!
//! Supported: `[[table]]`, `[[table.sub]]`, `key = "str" | int | bool | [..]`,
//! comments and blank lines. That is the whole vocabulary of `manifest.toml`,
//! `layouts.toml`, `spec-constants.toml`, `header-only-constants.toml`,
//! `probes.toml` and `exceptions.toml`; no crate dependency is needed.

use std::collections::BTreeMap;

/// A scalar or array value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Quoted string.
    Str(String),
    /// Signed integer.
    Int(i64),
    /// Boolean.
    Bool(bool),
    /// Array, one level deep in this subset.
    Array(Vec<Value>),
}

/// One parsed table (the body of a `[[name]]` block).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// Scalar fields.
    pub fields: BTreeMap<String, Value>,
    /// `[[parent.child]]` blocks attached to this table.
    pub subtables: BTreeMap<String, Vec<Table>>,
}

impl Table {
    /// String field, or an error naming the table.
    pub fn get_str(&self, key: &str) -> Result<&str, String> {
        match self.fields.get(key) {
            Some(Value::Str(value)) => Ok(value),
            Some(_) => Err(format!("`{key}` is not a string")),
            None => Err(format!("missing string field `{key}`")),
        }
    }

    /// Integer field.
    pub fn get_int(&self, key: &str) -> Result<i64, String> {
        match self.fields.get(key) {
            Some(Value::Int(value)) => Ok(*value),
            Some(_) => Err(format!("`{key}` is not an integer")),
            None => Err(format!("missing integer field `{key}`")),
        }
    }

    /// Optional integer field.
    pub fn opt_int(&self, key: &str) -> Result<Option<i64>, String> {
        match self.fields.get(key) {
            Some(Value::Int(value)) => Ok(Some(*value)),
            Some(_) => Err(format!("`{key}` is not an integer")),
            None => Ok(None),
        }
    }

    /// Boolean field.
    pub fn get_bool(&self, key: &str) -> Result<bool, String> {
        match self.fields.get(key) {
            Some(Value::Bool(value)) => Ok(*value),
            Some(_) => Err(format!("`{key}` is not a boolean")),
            None => Err(format!("missing boolean field `{key}`")),
        }
    }

    /// String-array field (`[]` when the key is absent).
    pub fn get_str_array(&self, key: &str) -> Result<Vec<String>, String> {
        match self.fields.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| match value {
                    Value::Str(text) => Ok(text.clone()),
                    _ => Err(format!("`{key}` contains a non-string element")),
                })
                .collect(),
            Some(_) => Err(format!("`{key}` is not an array")),
        }
    }

    /// Sub-tables of a name (empty when absent).
    pub fn get_tables(&self, key: &str) -> &[Table] {
        self.subtables.get(key).map_or(&[], Vec::as_slice)
    }
}

/// A parsed document: top-level `[[name]]` arrays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    /// Tables keyed by their top-level name.
    pub arrays: BTreeMap<String, Vec<Table>>,
}

impl Doc {
    /// Tables of a top-level name.
    pub fn get(&self, name: &str) -> &[Table] {
        self.arrays.get(name).map_or(&[], Vec::as_slice)
    }
}

/// Parse the subset described in the module documentation.
pub fn parse(text: &str) -> Result<Doc, String> {
    let mut doc = Doc::default();
    enum Current {
        Head(String, usize),
        Field(String, usize, String, usize),
    }
    let mut current: Option<Current> = None;
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[[") {
            let name = rest
                .strip_suffix("]]")
                .ok_or_else(|| format!("line {line_no}: unterminated table header"))?
                .trim();
            let (parent, child) = match name.split_once('.') {
                Some((parent, child)) if !child.is_empty() => (parent, Some(child)),
                _ => (name, None),
            };
            if parent.is_empty() {
                return Err(format!("line {line_no}: empty table name"));
            }
            let table = Table::default();
            let list = doc.arrays.entry(parent.to_string()).or_default();
            match child {
                Some(child) => {
                    let position = list.len().checked_sub(1).ok_or_else(|| {
                        format!("line {line_no}: `{name}` appears before `[[{parent}]]`")
                    })?;
                    let children = list[position]
                        .subtables
                        .entry(child.to_string())
                        .or_default();
                    children.push(table);
                    let child_index = children.len() - 1;
                    current = Some(Current::Field(
                        parent.to_string(),
                        position,
                        child.to_string(),
                        child_index,
                    ));
                }
                None => {
                    list.push(table);
                    current = Some(Current::Head(parent.to_string(), list.len() - 1));
                }
            }
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {line_no}: expected `key = value`"))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {line_no}: empty key"));
        }
        let parsed =
            parse_value(value.trim()).map_err(|error| format!("line {line_no}: {error}"))?;
        let table = match &current {
            Some(Current::Head(name, position)) => doc
                .arrays
                .get_mut(name)
                .and_then(|list| list.get_mut(*position)),
            Some(Current::Field(name, position, child, child_index)) => doc
                .arrays
                .get_mut(name)
                .and_then(|list| list.get_mut(*position))
                .and_then(|table| table.subtables.get_mut(child))
                .and_then(|children| children.get_mut(*child_index)),
            None => None,
        }
        .ok_or_else(|| format!("line {line_no}: assignment outside a table"))?;
        table.fields.insert(key.to_string(), parsed);
    }
    Ok(doc)
}

fn parse_value(text: &str) -> Result<Value, String> {
    if let Some(rest) = text.strip_prefix('"') {
        let body = rest
            .strip_suffix('"')
            .ok_or_else(|| "unterminated string".to_string())?;
        let mut out = String::new();
        let mut chars = body.chars();
        while let Some(character) = chars.next() {
            if character == '\\' {
                match chars.next() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => return Err("trailing backslash in string".to_string()),
                }
            } else {
                out.push(character);
            }
        }
        return Ok(Value::Str(out));
    }
    if text == "true" {
        return Ok(Value::Bool(true));
    }
    if text == "false" {
        return Ok(Value::Bool(false));
    }
    if let Some(rest) = text.strip_prefix('[') {
        let body = rest
            .strip_suffix(']')
            .ok_or_else(|| "unterminated array".to_string())?;
        let mut values = Vec::new();
        for item in split_items(body) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            values.push(parse_value(item)?);
        }
        return Ok(Value::Array(values));
    }
    text.parse::<i64>()
        .map(Value::Int)
        .map_err(|_| format!("unsupported value `{text}`"))
}

fn split_items(body: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in body.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == ',' {
            items.push(&body[start..index]);
            start = index + 1;
        }
    }
    items.push(&body[start..]);
    items
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == '#' {
            return &line[..index];
        }
    }
    line
}

#[cfg(test)]
#[path = "toml_lite_ut.rs"]
mod tests;
