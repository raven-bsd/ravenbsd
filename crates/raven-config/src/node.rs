// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A TOML-shaped configuration tree with an explicit removal value.

use std::collections::BTreeMap;

/// One configuration value.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// Remove the key from the merged table.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Integer(i64),
    /// A float.
    Float(f64),
    /// A string.
    String(String),
    /// A date/time value kept in its TOML text form.
    Datetime(String),
    /// An ordered array.
    Array(Vec<Node>),
    /// A table with byte-sorted keys.
    Table(BTreeMap<String, Node>),
}

impl Node {
    /// Convert a parsed TOML value.
    #[must_use]
    pub fn from_toml(value: toml::Value) -> Self {
        match value {
            toml::Value::String(value) => Self::String(value),
            toml::Value::Integer(value) => Self::Integer(value),
            toml::Value::Float(value) => Self::Float(value),
            toml::Value::Boolean(value) => Self::Bool(value),
            toml::Value::Datetime(value) => Self::Datetime(value.to_string()),
            toml::Value::Array(values) => {
                Self::Array(values.into_iter().map(Self::from_toml).collect())
            }
            toml::Value::Table(table) => Self::Table(
                table
                    .into_iter()
                    .map(|(key, value)| (key, Self::from_toml(value)))
                    .collect(),
            ),
        }
    }

    /// Convert back to TOML; a [`Node::Null`] has no TOML form.
    #[must_use]
    pub fn to_toml(&self) -> Option<toml::Value> {
        match self {
            Self::Null => None,
            Self::Bool(value) => Some(toml::Value::Boolean(*value)),
            Self::Integer(value) => Some(toml::Value::Integer(*value)),
            Self::Float(value) => Some(toml::Value::Float(*value)),
            Self::String(value) => Some(toml::Value::String(value.clone())),
            Self::Datetime(value) => value
                .parse::<toml::value::Datetime>()
                .ok()
                .map(toml::Value::Datetime),
            Self::Array(values) => Some(toml::Value::Array(
                values.iter().filter_map(Self::to_toml).collect(),
            )),
            Self::Table(table) => Some(toml::Value::Table(
                table
                    .iter()
                    .filter_map(|(key, value)| value.to_toml().map(|value| (key.clone(), value)))
                    .collect(),
            )),
        }
    }

    /// Convert to a JSON value; null stays null.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Integer(value) => serde_json::Value::Number((*value).into()),
            Self::Float(value) => serde_json::Number::from_f64(*value)
                .map_or(serde_json::Value::Null, serde_json::Value::Number),
            Self::String(value) => serde_json::Value::String(value.clone()),
            Self::Datetime(value) => serde_json::Value::String(value.clone()),
            Self::Array(values) => {
                serde_json::Value::Array(values.iter().map(Self::to_json).collect())
            }
            Self::Table(table) => serde_json::Value::Object(
                table
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_json()))
                    .collect(),
            ),
        }
    }

    /// Return the value at a dotted key, following only tables.
    #[must_use]
    pub fn get(&self, dotted: &str) -> Option<&Self> {
        let mut current = self;
        for part in dotted.split('.') {
            let table = match current {
                Self::Table(table) => table,
                _ => return None,
            };
            current = table.get(part)?;
        }
        Some(current)
    }

    /// The table behind this node.
    #[must_use]
    pub fn as_table(&self) -> Option<&BTreeMap<String, Node>> {
        match self {
            Self::Table(table) => Some(table),
            _ => None,
        }
    }

    /// The string behind this node.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// The boolean behind this node.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The integer behind this node.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The array behind this node.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Node]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }
}
