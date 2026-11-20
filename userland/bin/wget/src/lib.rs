// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable core of the simple `wget` tool.
//!
//! `wget` saves the response to the URL basename (or `-O FILE`), follows bounded redirects and
//! prints one short summary. It shares the HTTP/TLS request engine with `curl` and deliberately
//! does not implement recursive spidering, shell commands or a GNU command surface.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use curl::{FetchError, Options as CurlOptions};
use raven_http::{Clock, Response, Transport};

/// Successful exit status.
pub const EXIT_OK: i32 = 0;
/// Download or output failure.
pub const EXIT_ERROR: i32 = 1;
/// Usage error.
pub const EXIT_USAGE: i32 = 2;
/// Default total request timeout when `--max-time` is absent.
pub const DEFAULT_MAX_TIME_SECS: u32 = 10;

/// Parsed `wget` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Absolute URL.
    pub url: String,
    /// `-O FILE`, including `-` for stdout.
    pub output: Option<String>,
    /// `-q`; suppress the summary and diagnostics.
    pub quiet: bool,
    /// `--max-time SECS`.
    pub max_time_secs: Option<u32>,
    /// `--cacert FILE` for the TLS add-on build.
    pub ca_file: String,
}

impl Options {
    /// Total request timeout in milliseconds.
    #[must_use]
    pub fn timeout_ms(&self) -> u32 {
        self.max_time_secs
            .unwrap_or(DEFAULT_MAX_TIME_SECS)
            .saturating_mul(1_000)
    }
}

/// An argument parsing failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// An option name is not supported.
    UnknownFlag,
    /// An option that takes a value is last.
    MissingValue,
    /// `--max-time` is not a positive integer.
    InvalidMaxTime,
    /// No URL was given.
    MissingUrl,
    /// More than one URL was given.
    MultipleUrls,
    /// The URL is malformed or has no usable basename.
    InvalidUrl,
}

impl ParseError {
    /// A short user-facing message.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::UnknownFlag => "unknown option",
            Self::MissingValue => "option requires a value",
            Self::InvalidMaxTime => "--max-time needs a positive number of seconds",
            Self::MissingUrl => "missing URL",
            Self::MultipleUrls => "only one URL is supported",
            Self::InvalidUrl => "invalid URL or filename",
        }
    }
}

/// Parse command-line arguments (without the program name).
///
/// Supported: `-O FILE`, `-q`, `--max-time SECS` and `--cacert FILE` (TLS build). Recursive
/// spidering, shell execution and GNU-only flags are not implemented.
///
/// # Errors
///
/// Returns [`ParseError`] for an unknown option, a missing value, no URL or multiple URLs.
/// The version line printed by `-V`/`--version`.
pub const VERSION_LINE: &str = concat!("wget ", env!("CARGO_PKG_VERSION"), " (RavenBSD)");

/// Whether the argument list asks for the version line.
#[must_use]
pub fn version_requested<'a>(args: impl Iterator<Item = &'a str>) -> bool {
    args.into_iter()
        .any(|arg| arg == "-V" || arg == "--version")
}

/// Parse command-line arguments (without the program name).
pub fn parse<'a>(args: impl Iterator<Item = &'a str>) -> Result<Options, ParseError> {
    let args: Vec<&str> = args.collect();
    let mut options = Options {
        url: String::new(),
        output: None,
        quiet: false,
        max_time_secs: None,
        ca_file: String::from(curl::DEFAULT_CA_FILE),
    };
    let mut index = 0usize;
    let mut url = None;
    while index < args.len() {
        let argument = args[index];
        match argument {
            "-O" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingValue)?;
                options.output = Some(String::from(*value));
            }
            "-q" => options.quiet = true,
            "--max-time" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingValue)?;
                let seconds = value
                    .parse::<u32>()
                    .map_err(|_| ParseError::InvalidMaxTime)?;
                if seconds == 0 {
                    return Err(ParseError::InvalidMaxTime);
                }
                options.max_time_secs = Some(seconds);
            }
            "--cacert" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingValue)?;
                if value.is_empty() {
                    return Err(ParseError::MissingValue);
                }
                options.ca_file = String::from(*value);
            }
            "--" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingUrl)?;
                set_url(&mut url, value)?;
            }
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(ParseError::UnknownFlag);
            }
            value => set_url(&mut url, value)?,
        }
        index += 1;
    }
    options.url = url.ok_or(ParseError::MissingUrl)?.to_string();
    Ok(options)
}

fn set_url<'a>(url: &mut Option<&'a str>, value: &'a str) -> Result<(), ParseError> {
    if url.is_some() {
        return Err(ParseError::MultipleUrls);
    }
    *url = Some(value);
    Ok(())
}

/// Derive the default output name from the URL path.
///
/// The last path segment is used; an empty path or a trailing slash becomes `index.html`. A `.` or
/// `..` segment is rejected so a URL cannot escape the working directory.
///
/// # Errors
///
/// Returns [`ParseError::InvalidUrl`] for a malformed URL or an unusable basename.
pub fn default_filename(url: &str) -> Result<String, ParseError> {
    let parsed = raven_http::Url::parse(url).map_err(|_| ParseError::InvalidUrl)?;
    let path = parsed.path.split('?').next().unwrap_or("/");
    let name = path.rsplit('/').next().unwrap_or("");
    if name.is_empty() {
        return Ok(String::from("index.html"));
    }
    if name == "." || name == ".." {
        return Err(ParseError::InvalidUrl);
    }
    Ok(String::from(name))
}

/// Build the `curl` request URL, rewriting an `https://` target for the TLS transport.
///
/// # Errors
///
/// Returns [`ParseError::InvalidUrl`] when the URL cannot be parsed.
pub fn request_url(url: &str) -> Result<String, ParseError> {
    curl::https_http_url(url).map_err(|_| ParseError::InvalidUrl)
}

/// Fetch one URL into a response through the supplied transport and clock.
///
/// Redirects are bounded and followed by default. HTTP status 400 or above is a failure.
///
/// # Errors
///
/// Returns [`FetchError`] for a request, response or HTTP-status failure.
pub fn fetch_with<T: Transport, C: Clock>(
    options: &Options,
    transport: &mut T,
    clock: &C,
) -> Result<Response, FetchError> {
    let request =
        request_url(&options.url).map_err(|_| FetchError::Http(raven_http::Error::InvalidUrl))?;
    let curl_options = CurlOptions {
        url: request,
        output: None,
        silent: options.quiet,
        fail: true,
        headers_only: false,
        location: true,
        max_time_secs: options.max_time_secs,
        headers: Vec::new(),
        ca_file: options.ca_file.clone(),
    };
    curl::fetch_with(&curl_options.url, &curl_options, transport, clock)
}

/// Format the one-line progress/summary shown unless `-q` is set.
#[must_use]
pub fn summary(url: &str, filename: &str, bytes: usize) -> String {
    alloc::format!("wget: {url} -> {filename} ({bytes} bytes)")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
