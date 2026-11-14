// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven doctor`.

use std::ffi::OsString;
use std::fmt::Write as _;

use raven_output::{Exit, Format, Record};

use super::BackendError;
use crate::Context;
use crate::commands::{Command, CommandMeta};

static DOCTOR_META: CommandMeta = CommandMeta {
    name: "doctor",
    summary: "run ordered diagnostics",
    usage: "raven doctor [--strict] [--json]",
    reserved: false,
};

pub(crate) static DOCTOR_COMMAND: DoctorCommand = DoctorCommand;

/// The `doctor` command.
pub(crate) struct DoctorCommand;

impl Command for DoctorCommand {
    fn meta(&self) -> &'static CommandMeta {
        &DOCTOR_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        let mut strict = false;
        for argument in args {
            match argument.to_string_lossy().as_ref() {
                "--strict" if !strict => strict = true,
                other => {
                    return context.usage(&format!("unknown doctor argument `{other}`"));
                }
            }
        }
        run_with_backend(context, &UnavailableBackend, strict)
    }
}

/// One check status.
///
/// The target doctor backend constructs these values; the host placeholder returns an error.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckStatus {
    /// The check passed.
    Ok,
    /// The check produced a warning.
    Warn,
    /// The check failed.
    Fail,
}

impl CheckStatus {
    /// The stable wire spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// One doctor check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Check {
    /// Stable check identifier.
    pub(crate) id: String,
    /// Result.
    pub(crate) status: CheckStatus,
    /// Human detail.
    pub(crate) detail: String,
    /// Suggested repair text, never executed.
    pub(crate) fix: Option<String>,
}

/// A doctor data source.
pub(crate) trait DoctorBackend {
    /// Run the checks in order.
    fn checks(&self) -> Result<Vec<Check>, BackendError>;
}

/// The host placeholder: report the missing backend instead of inventing checks.
pub(crate) struct UnavailableBackend;

impl DoctorBackend for UnavailableBackend {
    fn checks(&self) -> Result<Vec<Check>, BackendError> {
        Err(BackendError::unavailable("doctor"))
    }
}

pub(crate) fn run_with_backend(
    context: &mut Context<'_>,
    backend: &dyn DoctorBackend,
    strict: bool,
) -> Exit {
    let checks = match backend.checks() {
        Ok(checks) => checks,
        Err(error) => {
            return context.fail_context(&error.code, &error.message, Some(("command", "doctor")));
        }
    };
    let mut failed = false;
    for check in &checks {
        match check.status {
            CheckStatus::Fail => failed = true,
            CheckStatus::Warn if strict => failed = true,
            CheckStatus::Ok | CheckStatus::Warn => {}
        }
        if context.format == Format::Json {
            let mut record = Record::new("check")
                .string("id", &check.id)
                .string("status", check.status.as_str())
                .string("detail", &check.detail);
            if let Some(fix) = &check.fix {
                record = record.string("fix", fix);
            }
            let result = context.emit_record(&record);
            if result != Exit::Success {
                return result;
            }
        } else if context.format == Format::Human {
            let mut text = String::new();
            let _ = writeln!(
                text,
                "{}: {} {}",
                check.id,
                check.status.as_str(),
                check.detail
            );
            if let Some(fix) = &check.fix {
                let _ = writeln!(text, "  fix: {fix}");
            }
            let result = context.write_stdout(&text);
            if result != Exit::Success {
                return result;
            }
        }
    }
    if failed { Exit::Failure } else { Exit::Success }
}

#[cfg(test)]
#[path = "doctor_ut.rs"]
mod tests;
