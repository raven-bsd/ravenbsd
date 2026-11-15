// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven theme`: preview, set and apply the semantic palette.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use raven_output::{Exit, Format, Record, Value};
use raven_theme::adapters::shell_fragment;
use raven_theme::{Depth, Palette, ThemeFile, paint};

use crate::Context;
use crate::commands::{Command, CommandMeta};

static THEME_META: CommandMeta = CommandMeta {
    name: "theme",
    summary: "preview, set, apply or export the Raven terminal theme",
    usage: "raven theme <preview|set|apply|export> [OPTIONS]",
    reserved: false,
};

pub(crate) static THEME_COMMAND: Theme = Theme;

/// The `theme` command.
pub(crate) struct Theme;

impl Command for Theme {
    fn meta(&self) -> &'static CommandMeta {
        &THEME_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        let Some(action) = args.first().map(|value| value.to_string_lossy()) else {
            return context.usage("theme needs preview, set, apply or export");
        };
        match action.as_ref() {
            "preview" => preview(context, &args[1..]),
            "set" => set(context, &args[1..]),
            "apply" => apply(context, &args[1..]),
            "export" => export(context, &args[1..]),
            _ => context.usage("theme needs preview, set, apply or export"),
        }
    }
}

fn palette() -> Result<Palette, String> {
    ThemeFile::load()
}

fn depth() -> Depth {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    Depth::detect(colorterm.as_deref(), term.as_deref())
}

fn preview(context: &mut Context<'_>, args: &[OsString]) -> Exit {
    if !args.is_empty() {
        return context.usage("theme preview takes no arguments");
    }
    let palette = match palette() {
        Ok(palette) => palette,
        Err(error) => return context.fail("ECONFIG", &error),
    };
    if context.format == Format::Json {
        let roles = palette
            .roles()
            .into_iter()
            .map(|(role, colour)| (role.as_str().to_string(), Value::String(colour.to_hex())))
            .collect();
        let record = Record::new("theme")
            .string("name", &palette.name)
            .object("roles", roles);
        return context.emit_record(&record);
    }
    let depth = depth();
    let mut text = String::new();
    for (role, colour) in palette.roles() {
        if context.color {
            text.push_str(&paint(role, role.as_str(), &palette, depth, true));
        } else {
            text.push_str(role.as_str());
        }
        text.push_str("  ");
        text.push_str(&colour.to_hex());
        text.push('\n');
    }
    context.write_stdout(&text)
}

fn export(context: &mut Context<'_>, args: &[OsString]) -> Exit {
    let shell = match args {
        [] => false,
        [flag] if flag == "--shell" => true,
        _ => return context.usage("theme export takes only --shell"),
    };
    let palette = match palette() {
        Ok(palette) => palette,
        Err(error) => return context.fail("ECONFIG", &error),
    };
    let spec = palette.to_env_spec();
    if context.format == Format::Json {
        let record = Record::new("theme")
            .string("name", &palette.name)
            .string("palette", &spec);
        return context.emit_record(&record);
    }
    if shell {
        context.write_stdout(&format!("export RAVEN_PALETTE='{spec}'\n"))
    } else {
        context.write_stdout(&format!("{spec}\n"))
    }
}

fn set(context: &mut Context<'_>, args: &[OsString]) -> Exit {
    let [value] = args else {
        return context.usage("theme set needs a name or path");
    };
    let value = value.to_string_lossy();
    let text = if let Some(palette) = Palette::builtin(&value) {
        format!("[theme]\nname = \"{}\"\n", palette.name)
    } else {
        let path = Path::new(value.as_ref());
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => return context.fail("ECONFIG", &format!("{}: {error}", path.display())),
        }
    };
    if let Err(error) = ThemeFile::parse(&text) {
        return context.fail("ECONFIG", &error);
    }
    let path = ThemeFile::user_path();
    if context.dry_run {
        return emit_action(context, "set", &path, None, true);
    }
    if let Err(error) = write_atomic(&path, &text) {
        return context.fail("ECONFIG", &error);
    }
    emit_action(context, "set", &path, None, false)
}

fn apply(context: &mut Context<'_>, args: &[OsString]) -> Exit {
    if !args.is_empty() {
        return context.usage("theme apply takes no positional argument");
    }
    let palette = match palette() {
        Ok(palette) => palette,
        Err(error) => return context.fail("ECONFIG", &error),
    };
    let path = adapter_path();
    let backup = match apply_to(&path, &palette, depth(), context.dry_run) {
        Ok(backup) => backup,
        Err(error) => return context.fail("ECONFIG", &error),
    };
    emit_action(context, "apply", &path, backup.as_deref(), context.dry_run)
}

/// Write the adapter file, backing up an existing file first.
///
/// Returns the backup path when a backup was (or, for a dry run, would be) made.
fn apply_to(
    path: &Path,
    palette: &Palette,
    depth: Depth,
    dry_run: bool,
) -> Result<Option<PathBuf>, String> {
    let backup = path.is_file().then(|| backup_path(path));
    if dry_run {
        return Ok(backup);
    }
    if let Some(backup) = &backup {
        fs::copy(path, backup).map_err(|error| format!("{}: {error}", backup.display()))?;
    }
    write_atomic(path, &shell_fragment(palette, depth))?;
    Ok(backup)
}

fn emit_action(
    context: &mut Context<'_>,
    action: &str,
    path: &Path,
    backup: Option<&Path>,
    dry_run: bool,
) -> Exit {
    if context.format == Format::Json {
        let record = Record::new("theme")
            .string("action", action)
            .string("path", &path.display().to_string())
            .field(
                "backup",
                backup.map_or(Value::Null, |path| {
                    Value::String(path.display().to_string())
                }),
            )
            .bool("dry_run", dry_run);
        return context.emit_record(&record);
    }
    let mut text = if dry_run {
        format!("would write {}\n", path.display())
    } else {
        format!("theme: {}\n", path.display())
    };
    if let Some(backup) = backup {
        if dry_run {
            text.push_str(&format!("would back up {}\n", backup.display()));
        } else {
            text.push_str(&format!("backup: {}\n", backup.display()));
        }
    }
    context.write_stdout(&text)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let temporary = appended(path, ".tmp");
    fs::write(&temporary, text).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}

fn backup_path(path: &Path) -> PathBuf {
    appended(path, ".bak")
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// The adapter path used by `theme apply`.
#[must_use]
pub fn adapter_path() -> PathBuf {
    ThemeFile::user_path().with_file_name("theme.env")
}

#[cfg(test)]
#[path = "theme_ut.rs"]
mod tests;
