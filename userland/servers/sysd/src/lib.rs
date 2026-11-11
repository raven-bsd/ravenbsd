// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure content helpers for `sysd`.
//!
//! Every `sys:` file is text with a trailing newline. `hostname` is the only writable one; the
//! rest are generated from `UNAME`, `CLOCK_GET` or the current hostname at read time.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use raven_abi::uname::Uname;

/// Maximum hostname length in bytes.
pub const HOSTNAME_MAX: usize = 63;
/// Directory names in `sys:`, in listing order.
pub const FILES: [&str; 5] = ["hostname", "uname", "version", "uptime", "kernel"];

/// Validate and normalize a hostname write.
///
/// Trailing `\n`/`\r` bytes are removed. An empty result, a NUL, a slash or a value longer than
/// [`HOSTNAME_MAX`] is rejected.
pub fn sanitize_hostname(input: &[u8]) -> Option<Vec<u8>> {
    let mut end = input.len();
    while end > 0 && matches!(input.get(end - 1), Some(b'\n' | b'\r')) {
        end -= 1;
    }
    let value = input.get(..end)?;
    if value.is_empty()
        || value.len() > HOSTNAME_MAX
        || value.contains(&0)
        || value.contains(&b'/')
        || value.iter().any(|byte| byte.is_ascii_control())
    {
        return None;
    }
    Some(value.to_vec())
}

/// Render the hostname file.
pub fn hostname_text(hostname: &[u8]) -> Vec<u8> {
    let mut text = Vec::with_capacity(hostname.len() + 1);
    text.extend_from_slice(hostname);
    text.push(b'\n');
    text
}

/// Render the five `UNAME` fields, one per line: sysname, nodename, release, version, machine.
pub fn uname_text(uname: &Uname) -> Vec<u8> {
    let fields = [
        &uname.sysname,
        &uname.nodename,
        &uname.release,
        &uname.version,
        &uname.machine,
    ];
    let mut text = Vec::new();
    for field in fields {
        text.extend_from_slice(Uname::field_str(field).unwrap_or("").as_bytes());
        text.push(b'\n');
    }
    text
}

/// Render the `version` file (the `UNAME` version field).
pub fn version_text(uname: &Uname) -> Vec<u8> {
    let mut text = Uname::field_str(&uname.version)
        .unwrap_or("")
        .as_bytes()
        .to_vec();
    text.push(b'\n');
    text
}

/// Render the `kernel` file (`sysname release version` on one line).
pub fn kernel_text(uname: &Uname) -> Vec<u8> {
    let line = format!(
        "{} {} {}",
        Uname::field_str(&uname.sysname).unwrap_or(""),
        Uname::field_str(&uname.release).unwrap_or(""),
        Uname::field_str(&uname.version).unwrap_or("")
    );
    let mut text = line.into_bytes();
    text.push(b'\n');
    text
}

/// Render the `uptime` file.
pub fn uptime_text(seconds: u64) -> Vec<u8> {
    format!("{seconds}\n").into_bytes()
}

/// True when `name` is one of the files in `sys:`.
pub fn is_file(name: &str) -> bool {
    FILES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_abi::uname::Uname;

    fn uname() -> Uname {
        let mut uname = Uname::zeroed();
        Uname::set_field(&mut uname.sysname, "RavenBSD");
        Uname::set_field(&mut uname.nodename, "raven");
        Uname::set_field(&mut uname.release, "0.1");
        Uname::set_field(&mut uname.version, "RavenBSD 0.1 (BASE) #1: test");
        Uname::set_field(&mut uname.machine, "x86_64");
        uname
    }

    fn text(bytes: &[u8]) -> &str {
        core::str::from_utf8(bytes).unwrap()
    }

    #[test]
    fn hostname_keeps_plain_value() {
        assert_eq!(sanitize_hostname(b"raven"), Some(b"raven".to_vec()));
    }

    #[test]
    fn hostname_trims_line_endings() {
        assert_eq!(
            sanitize_hostname(b"coretest\r\n"),
            Some(b"coretest".to_vec())
        );
    }

    #[test]
    fn empty_hostname_is_rejected() {
        assert_eq!(sanitize_hostname(b""), None);
        assert_eq!(sanitize_hostname(b"\n"), None);
    }

    #[test]
    fn hostname_length_boundary() {
        let max = vec![b'a'; HOSTNAME_MAX];
        assert_eq!(sanitize_hostname(&max), Some(max));
        let too_long = vec![b'a'; HOSTNAME_MAX + 1];
        assert_eq!(sanitize_hostname(&too_long), None);
    }

    #[test]
    fn hostname_rejects_controls_and_slashes() {
        assert_eq!(sanitize_hostname(b"bad\0name"), None);
        assert_eq!(sanitize_hostname(b"bad/name"), None);
        assert_eq!(sanitize_hostname(b"bad\tname"), None);
    }

    #[test]
    fn hostname_text_has_a_newline() {
        assert_eq!(text(&hostname_text(b"raven")), "raven\n");
    }

    #[test]
    fn uname_has_five_lines() {
        assert_eq!(
            text(&uname_text(&uname())),
            "RavenBSD\nraven\n0.1\nRavenBSD 0.1 (BASE) #1: test\nx86_64\n"
        );
    }

    #[test]
    fn version_is_the_uname_version_line() {
        assert_eq!(
            text(&version_text(&uname())),
            "RavenBSD 0.1 (BASE) #1: test\n"
        );
    }

    #[test]
    fn kernel_combines_sysname_release_and_version() {
        assert_eq!(
            text(&kernel_text(&uname())),
            "RavenBSD 0.1 RavenBSD 0.1 (BASE) #1: test\n"
        );
    }

    #[test]
    fn uptime_is_seconds_and_newline() {
        assert_eq!(text(&uptime_text(0)), "0\n");
        assert_eq!(text(&uptime_text(42)), "42\n");
    }

    #[test]
    fn file_list_is_stable() {
        assert_eq!(FILES, ["hostname", "uname", "version", "uptime", "kernel"]);
        assert!(is_file("hostname"));
        assert!(!is_file("missing"));
    }

    #[test]
    fn empty_uname_fields_still_render() {
        let text = uname_text(&Uname::zeroed());
        assert_eq!(text, b"\n\n\n\n\n");
    }
}
