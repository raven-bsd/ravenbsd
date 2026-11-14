// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One NDJSON record: a fixed envelope plus ordered typed fields.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::SCHEMA;
use crate::json::escape_json;

/// One field value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A known-but-absent value.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Signed(i64),
    /// An unsigned integer.
    Unsigned(u64),
    /// A finite float; a non-finite value is written as null.
    Float(f64),
    /// A UTF-8 string, escaped by the writer.
    String(String),
    /// An ordered array.
    Array(Vec<Value>),
    /// An ordered object.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Write the value as JSON.
    pub fn write_json(&self, output: &mut String) -> fmt::Result {
        match self {
            Self::Null => output.push_str("null"),
            Self::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Self::Signed(value) => write!(output, "{value}")?,
            Self::Unsigned(value) => write!(output, "{value}")?,
            Self::Float(value) => {
                if value.is_finite() {
                    write!(output, "{value}")?;
                } else {
                    output.push_str("null");
                }
            }
            Self::String(value) => {
                output.push('"');
                escape_json(value, output);
                output.push('"');
            }
            Self::Array(items) => {
                output.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    item.write_json(output)?;
                }
                output.push(']');
            }
            Self::Object(entries) => {
                output.push('{');
                for (index, (name, value)) in entries.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    output.push('"');
                    escape_json(name, output);
                    output.push_str("\":");
                    value.write_json(output)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
}

/// One NDJSON object with the mandatory `schema` and `kind` envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    kind: String,
    fields: Vec<(String, Value)>,
}

impl Record {
    /// Start a record of `kind`; `schema` is emitted automatically.
    #[must_use]
    pub fn new(kind: &str) -> Self {
        Self {
            kind: String::from(kind),
            fields: Vec::new(),
        }
    }

    /// Append one typed field.
    #[must_use]
    pub fn field(mut self, name: &str, value: Value) -> Self {
        self.fields.push((String::from(name), value));
        self
    }

    /// Append a string field.
    #[must_use]
    pub fn string(self, name: &str, value: &str) -> Self {
        self.field(name, Value::String(String::from(value)))
    }

    /// Append an unsigned field.
    #[must_use]
    pub fn u64(self, name: &str, value: u64) -> Self {
        self.field(name, Value::Unsigned(value))
    }

    /// Append a signed field.
    #[must_use]
    pub fn i64(self, name: &str, value: i64) -> Self {
        self.field(name, Value::Signed(value))
    }

    /// Append a float field; non-finite values are written as null.
    #[must_use]
    pub fn f64(self, name: &str, value: f64) -> Self {
        self.field(name, Value::Float(value))
    }

    /// Append a boolean field.
    #[must_use]
    pub fn bool(self, name: &str, value: bool) -> Self {
        self.field(name, Value::Bool(value))
    }

    /// Append a null field.
    #[must_use]
    pub fn null(self, name: &str) -> Self {
        self.field(name, Value::Null)
    }

    /// Append an array field.
    #[must_use]
    pub fn array(self, name: &str, items: Vec<Value>) -> Self {
        self.field(name, Value::Array(items))
    }

    /// Append an object field.
    #[must_use]
    pub fn object(self, name: &str, entries: Vec<(String, Value)>) -> Self {
        self.field(name, Value::Object(entries))
    }

    /// The record kind.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The ordered fields, excluding the `schema`/`kind` envelope.
    #[must_use]
    pub fn fields(&self) -> &[(String, Value)] {
        &self.fields
    }

    /// Write the object without a trailing newline.
    pub fn write_json(&self, output: &mut String) -> fmt::Result {
        output.push_str("{\"schema\":");
        write!(output, "{SCHEMA}")?;
        output.push_str(",\"kind\":\"");
        escape_json(&self.kind, output);
        output.push('"');
        for (name, value) in &self.fields {
            output.push_str(",\"");
            escape_json(name, output);
            output.push_str("\":");
            value.write_json(output)?;
        }
        output.push('}');
        Ok(())
    }

    /// The object as one JSON string, without a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut output = String::new();
        let _ = self.write_json(&mut output);
        output
    }
}
