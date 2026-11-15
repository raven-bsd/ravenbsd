// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven config check|show|get`.

use std::ffi::OsString;
use std::path::Path;

use raven_config::{ConfigLoader, Node};
use raven_output::{Exit, Format, Record, Value};

use crate::Context;
use crate::commands::{Command, CommandMeta};

static CONFIG_META: CommandMeta = CommandMeta {
    name: "config",
    summary: "validate and show the effective TOML configuration",
    usage: "raven config <check|show|get <key>> [--json]",
    reserved: false,
};

pub(crate) static CONFIG_COMMAND: ConfigCommand = ConfigCommand;

/// The `config` command.
pub(crate) struct ConfigCommand;

impl Command for ConfigCommand {
    fn meta(&self) -> &'static CommandMeta {
        &CONFIG_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        run_with_root(context, Path::new("/"), args)
    }
}

pub(crate) fn run_with_root(context: &mut Context<'_>, root: &Path, args: &[OsString]) -> Exit {
    let Some(command) = args.first() else {
        return context.usage("config needs check, show or get");
    };
    let rest = &args[1..];
    match command.to_string_lossy().as_ref() {
        "check" if rest.is_empty() => check(context, root),
        "show" if rest.is_empty() => show(context, root),
        "get" if rest.len() == 1 => get(context, root, &rest[0].to_string_lossy()),
        "check" | "show" => context.usage("this config subcommand takes no arguments"),
        "get" => context.usage("config get needs exactly one dotted key"),
        _ => context.usage("config subcommand must be check, show or get"),
    }
}

fn check(context: &mut Context<'_>, root: &Path) -> Exit {
    match ConfigLoader::new(root).load() {
        Ok(config) => {
            if context.format == Format::Json {
                let record = Record::new("check")
                    .string("id", "config.parse")
                    .string("status", "ok")
                    .u64("files", config.source_count() as u64);
                context.emit_record(&record)
            } else {
                context.write_stdout(&format!("config: ok ({} files)\n", config.source_count()))
            }
        }
        Err(error) => context.fail("ECONFIG", &error.to_string()),
    }
}

fn show(context: &mut Context<'_>, root: &Path) -> Exit {
    let config = match ConfigLoader::new(root).load() {
        Ok(config) => config,
        Err(error) => return context.fail("ECONFIG", &error.to_string()),
    };
    if context.format == Format::Json {
        let record = Record::new("config")
            .field("config", output_value(config.node()))
            .u64("sources", config.source_count() as u64);
        context.emit_record(&record)
    } else {
        match config.to_toml_string() {
            Ok(text) => context.write_stdout(&text),
            Err(error) => context.fail("ECONFIG", &error.to_string()),
        }
    }
}

fn get(context: &mut Context<'_>, root: &Path, key: &str) -> Exit {
    let config = match ConfigLoader::new(root).load() {
        Ok(config) => config,
        Err(error) => return context.fail("ECONFIG", &error.to_string()),
    };
    let Some(value) = config.get(key) else {
        return context.fail("ENOENT", &format!("configuration key `{key}` is not set"));
    };
    if context.format == Format::Json {
        let record = Record::new("config")
            .string("key", key)
            .field("value", output_value(value));
        context.emit_record(&record)
    } else {
        context.write_stdout(&format!("{key} = {}\n", human_value(value)))
    }
}

fn output_value(node: &Node) -> Value {
    match node {
        Node::Null => Value::Null,
        Node::Bool(value) => Value::Bool(*value),
        Node::Integer(value) => Value::Signed(*value),
        Node::Float(value) => Value::Float(*value),
        Node::String(value) => Value::String(value.clone()),
        Node::Datetime(value) => Value::String(value.clone()),
        Node::Array(values) => Value::Array(values.iter().map(output_value).collect()),
        Node::Table(table) => Value::Object(
            table
                .iter()
                .map(|(key, value)| (key.clone(), output_value(value)))
                .collect(),
        ),
    }
}

fn human_value(node: &Node) -> String {
    match node {
        Node::Null => String::from("null"),
        Node::Bool(value) => value.to_string(),
        Node::Integer(value) => value.to_string(),
        Node::Float(value) => value.to_string(),
        Node::String(value) => format!("{value:?}"),
        Node::Datetime(value) => value.clone(),
        Node::Array(_) | Node::Table(_) => format!("{node:?}"),
    }
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
