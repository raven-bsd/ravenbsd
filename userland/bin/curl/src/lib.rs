// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable core of the simple `curl` tool.
//!
//! The command-line surface is deliberately small: one URL, a bounded GET, and the options named
//! in K199. HTTP uses `raven-http` over the platform transport; HTTPS is only available in the
//! add-on build that links `raven-tls`, never through an unverified fallback.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(
    not(all(feature = "tls", any(target_os = "none", target_os = "ravenbsd"))),
    forbid(unsafe_code)
)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_http::{Client, Clock, Error as HttpError, RequestBuilder, Response, Transport};

/// Successful exit status.
pub const EXIT_OK: i32 = 0;
/// Request or output failure.
pub const EXIT_ERROR: i32 = 1;
/// Usage error.
pub const EXIT_USAGE: i32 = 2;
/// `-f` HTTP-status failure (the conventional curl code).
pub const EXIT_HTTP_FAILED: i32 = 22;
/// Default total request timeout when `--max-time` is absent.
pub const DEFAULT_MAX_TIME_SECS: u32 = 10;
/// Bounded redirect budget when `-L` is present.
pub const MAX_REDIRECTS: u8 = 5;
/// Default CA bundle path for the TLS add-on build.
pub const DEFAULT_CA_FILE: &str = "/addons/etc/ssl/certs/ca-bundle.pem";
/// User agent sent by both tools.
pub const USER_AGENT: &str = "curl/0.1 (RavenBSD)";

/// One request header supplied by `-H`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderSpec {
    /// Header name.
    pub name: String,
    /// Header value with leading OWS removed.
    pub value: String,
}

/// Parsed `curl` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Absolute URL.
    pub url: String,
    /// `-o FILE`, including `-` for stdout.
    pub output: Option<String>,
    /// `-s`; suppress non-body diagnostics.
    pub silent: bool,
    /// `-f`; treat a status of 400 or above as a failure.
    pub fail: bool,
    /// `-I`; print the status line and headers instead of the body.
    pub headers_only: bool,
    /// `-L`; follow the bounded redirect budget.
    pub location: bool,
    /// `--max-time SECS`.
    pub max_time_secs: Option<u32>,
    /// `-H 'Name: value'` fields.
    pub headers: Vec<HeaderSpec>,
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

    /// True when `-o -` or no output path was given.
    #[must_use]
    pub fn writes_stdout(&self) -> bool {
        self.output.as_deref().is_none_or(|path| path == "-")
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
    /// `-H` does not contain a valid `name: value` pair.
    InvalidHeader,
    /// No URL was given.
    MissingUrl,
    /// More than one URL was given.
    MultipleUrls,
    /// The URL is malformed or uses an unsupported scheme.
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
            Self::InvalidHeader => "-H needs 'Name: value'",
            Self::MissingUrl => "missing URL",
            Self::MultipleUrls => "only one URL is supported",
            Self::InvalidUrl => "invalid URL",
        }
    }
}

/// Parse command-line arguments (without the program name).
///
/// Supported: `-o FILE`, `-s`, `-f`, `-I`, `-L`, `--max-time SECS`, `-H 'Name: value'` and
/// `--cacert FILE` (TLS build). Combined short flags are not supported; this is not GNU curl.
///
/// # Errors
///
/// Returns [`ParseError`] for an unknown option, a missing value, a malformed header, no URL or
/// multiple URLs.
/// The version line printed by `-V`/`--version`.
pub const VERSION_LINE: &str = concat!("curl ", env!("CARGO_PKG_VERSION"), " (RavenBSD)");

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
        silent: false,
        fail: false,
        headers_only: false,
        location: false,
        max_time_secs: None,
        headers: Vec::new(),
        ca_file: String::from(DEFAULT_CA_FILE),
    };
    let mut index = 0usize;
    let mut url = None;
    while index < args.len() {
        let argument = args[index];
        match argument {
            "-o" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingValue)?;
                options.output = Some(String::from(*value));
            }
            "-s" => options.silent = true,
            "-f" => options.fail = true,
            "-I" => options.headers_only = true,
            "-L" => options.location = true,
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
            "-H" => {
                index += 1;
                let value = args.get(index).ok_or(ParseError::MissingValue)?;
                options.headers.push(parse_header(value)?);
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

fn parse_header(value: &str) -> Result<HeaderSpec, ParseError> {
    let (name, raw) = value.split_once(':').ok_or(ParseError::InvalidHeader)?;
    let name = name.trim();
    let value = raw.trim_start();
    if name.is_empty() || value.is_empty() {
        return Err(ParseError::InvalidHeader);
    }
    Ok(HeaderSpec {
        name: String::from(name),
        value: String::from(value),
    })
}

/// A fetch failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The HTTP core rejected the URL, request or response.
    Http(HttpError),
    /// `-f` converted an HTTP error status into a failure.
    HttpStatus(u16),
}

impl FetchError {
    /// A short user-facing message.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Http(error) => http_error_message(error),
            Self::HttpStatus(_) => "HTTP status indicates failure",
        }
    }
}

const fn http_error_message(error: HttpError) -> &'static str {
    match error {
        HttpError::InvalidUrl => "invalid URL",
        HttpError::InvalidRequest => "invalid request",
        HttpError::InvalidResponse => "invalid HTTP response",
        HttpError::HeaderLimit => "response header limit exceeded",
        HttpError::BodyLimit => "response body limit exceeded",
        HttpError::Timeout => "request timed out",
        HttpError::Transport => "connection failed",
        HttpError::UnsupportedScheme => "unsupported URL scheme",
        HttpError::TooManyRedirects => "redirect limit exceeded",
        HttpError::MissingLocation => "redirect without Location",
        HttpError::Truncated => "truncated response",
        HttpError::Chunked => "malformed chunked response",
        HttpError::Overflow => "length overflow",
    }
}

/// Fetch one URL with the given transport and clock.
///
/// The URL must be an `http://` URL. The TLS build rewrites an `https://` URL to the equivalent
/// `http://host:443/` request and lets the transport own the verified TLS connection.
///
/// # Errors
///
/// Returns [`FetchError::Http`] for a request/response failure and [`FetchError::HttpStatus`]
/// when `-f` is set and the final status is 400 or above.
pub fn fetch_with<T: Transport, C: Clock>(
    url: &str,
    options: &Options,
    transport: &mut T,
    clock: &C,
) -> Result<Response, FetchError> {
    let mut builder = RequestBuilder::new(url)
        .map_err(FetchError::Http)?
        .timeout_ms(options.timeout_ms())
        .max_redirects(if options.location { MAX_REDIRECTS } else { 0 })
        .user_agent(USER_AGENT)
        .map_err(FetchError::Http)?;
    for header in &options.headers {
        builder = builder
            .header(&header.name, &header.value)
            .map_err(FetchError::Http)?;
    }
    let request = builder.build();
    let response = Client::new(transport, clock)
        .get(&request)
        .map_err(FetchError::Http)?;
    if options.fail && response.status >= 400 {
        return Err(FetchError::HttpStatus(response.status));
    }
    Ok(response)
}

/// Format the status line and headers as HTTP wire text.
#[must_use]
pub fn format_head(response: &Response) -> String {
    let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, response.reason);
    for header in &response.headers {
        out.push_str(&header.name);
        out.push_str(": ");
        out.push_str(&header.value);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    out
}

/// True when `url` names HTTPS.
#[must_use]
pub fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

/// Rewrite an `https://` URL to the `http://host:443/` form used by `raven-http`.
///
/// The transport performs the verified TLS handshake; the HTTP core stays scheme-agnostic. The
/// explicit port also produces the correct `Host` header. A plain `http://` URL is returned as
/// is.
///
/// # Errors
///
/// Returns [`ParseError::InvalidUrl`] when the URL cannot be parsed.
pub fn https_http_url(url: &str) -> Result<String, ParseError> {
    let parsed = raven_http::Url::parse(url).map_err(|_| ParseError::InvalidUrl)?;
    if parsed.scheme != raven_http::Scheme::Https {
        return Ok(String::from(url));
    }
    Ok(format!(
        "http://{}:{}{}",
        parsed.host, parsed.port, parsed.path
    ))
}

/// True when this build links the TLS add-on.
pub const TLS_AVAILABLE: bool = cfg!(feature = "tls");

#[cfg(all(feature = "tls", any(target_os = "none", target_os = "ravenbsd")))]
pub mod tls;

/// Create the base-build plain-HTTP transport.
///
/// The base build uses the `net:`/`resolv:` adapter from `raven-http-net`; the TLS add-on build
/// provides its own scheme transport so it can share the connected stream with OpenSSL.
#[cfg(all(target_os = "none", not(feature = "tls")))]
pub fn live_http_transport() -> impl Transport {
    raven_http_net::live_transport()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
