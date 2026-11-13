// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `layouts.toml`: sizes, alignments and offsets frozen by doc 14 3.A.3.

use crate::clang::Arch;
use crate::manifest::{self, HeaderRow, Status};
use crate::toml_lite;
use std::path::PathBuf;

/// One measured field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Byte offset.
    pub offset: u64,
    /// Optional byte size of the field.
    pub bytes: Option<u64>,
}

/// One `[[type]]` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutRow {
    /// C type name as written in `sizeof`/`_Alignof`.
    pub name: String,
    /// Headers needed to see the type.
    pub headers: Vec<String>,
    /// Size in bytes.
    pub size: u64,
    /// Alignment in bytes.
    pub align: u64,
    /// Architectures the row applies to.
    pub arches: Vec<String>,
    /// Field offsets.
    pub fields: Vec<Field>,
    /// Owner when the row is not C37's (optional data-file field).
    pub owner: Option<String>,
}

/// Load `layouts.toml`.
pub fn load() -> Result<Vec<LayoutRow>, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("layouts.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&text)
}

/// Parse a layouts text.
pub fn parse(text: &str) -> Result<Vec<LayoutRow>, String> {
    let doc = toml_lite::parse(text).map_err(|error| format!("layouts.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("type") {
        let size = table.get_int("size")?;
        let align = table.get_int("align")?;
        let mut fields = Vec::new();
        for field in table.get_tables("field") {
            fields.push(Field {
                name: field.get_str("name")?.to_string(),
                offset: u64::try_from(field.get_int("offset")?)
                    .map_err(|_| "negative field offset".to_string())?,
                bytes: field
                    .opt_int("bytes")?
                    .map(|value| {
                        u64::try_from(value).map_err(|_| "negative field size".to_string())
                    })
                    .transpose()?,
            });
        }
        rows.push(LayoutRow {
            name: table.get_str("name")?.to_string(),
            headers: table.get_str_array("headers")?,
            size: u64::try_from(size).map_err(|_| "negative type size".to_string())?,
            align: u64::try_from(align).map_err(|_| "negative type alignment".to_string())?,
            arches: table.get_str_array("arches")?,
            fields,
            owner: table
                .fields
                .get("owner")
                .map(|_| table.get_str("owner").map(ToString::to_string))
                .transpose()?,
        });
    }
    Ok(rows)
}

/// True when every header of the row is owned and present on the architecture.
#[must_use]
pub fn available(row: &LayoutRow, arch: Arch, headers: &[HeaderRow]) -> bool {
    if row.owner.as_deref().is_some_and(|owner| owner != "C37") {
        return false;
    }
    row.arches.iter().any(|name| name == arch.name())
        && row.headers.iter().all(|path| {
            if manifest::compiler_header(path) {
                return true;
            }
            manifest::find(headers, path).is_some_and(|header| header.status == Status::Owned)
                && manifest::installed(path, Some(arch.name()))
        })
}

/// The ticket that must provide the first missing header of a row.
#[must_use]
pub fn owner(row: &LayoutRow, headers: &[HeaderRow]) -> String {
    row.owner.clone().unwrap_or_else(|| {
        row.headers
            .iter()
            .find_map(|path| manifest::find(headers, path).map(|header| header.owner.clone()))
            .unwrap_or_else(|| "C37".to_string())
    })
}

/// The preamble that makes every measured type visible. It includes the
/// public headers of both header tickets (C37 and C38) instead of restating
/// their bodies; a row is measured only when its own headers are owned.
pub const PREAMBLE: &str = "\
#define __NEED_struct_timeval
#define __NEED_struct_iovec
#define __NEED_sigset_t
#define __NEED_mbstate_t
#define __NEED_va_list
#define __NEED_off64_t
#define __NEED_socklen_t
#define __NEED_sa_family_t
#define __NEED_float_t
#define __NEED_double_t
#include <sys/types.h>
#include <stddef.h>
#include <stdint.h>
#include <time.h>
#include <setjmp.h>
#include <threads.h>
#include <locale.h>
#include <fenv.h>
#include <wchar.h>
#include <signal.h>
#include <semaphore.h>
#include <termios.h>
#include <sys/stat.h>
#include <dirent.h>
#include <sys/statvfs.h>
#include <pwd.h>
#include <grp.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <sys/un.h>
#include <netdb.h>
#include <poll.h>
#include <sys/epoll.h>
#include <aio.h>
#include <glob.h>
#include <spawn.h>
#include <mqueue.h>
#include <sched.h>
#include <fcntl.h>
#include <sys/utsname.h>
#include <sys/select.h>
#include <sys/param.h>
#include <sys/event.h>
#include <net/if.h>
#include <net/route.h>
#include <ifaddrs.h>
#include <sys/time.h>
#include <netinet/tcp.h>
#include <sys/file.h>
#include <elf.h>
#include <sys/sysinfo.h>
/* RavenBSD C41: the stdlib/getopt layout rows (div_t, struct option). */
#include <stdlib.h>
#include <getopt.h>
/* RavenBSD: offsetof(struct stat, __unused) needs the field name, not the
 * cdefs attribute macro. */
#undef __unused
";

/// Generate the static-assert TU for one architecture.
#[must_use]
pub fn assert_source(rows: &[LayoutRow], arch: Arch, headers: &[HeaderRow]) -> String {
    let mut source = String::from(PREAMBLE);
    for row in rows {
        if !available(row, arch, headers) {
            continue;
        }
        let name = &row.name;
        source.push_str(&format!(
            "_Static_assert(sizeof({name}) == {}, \"{name} size\");\n",
            row.size
        ));
        source.push_str(&format!(
            "_Static_assert(_Alignof({name}) == {}, \"{name} align\");\n",
            row.align
        ));
        for field in &row.fields {
            source.push_str(&format!(
                "_Static_assert(offsetof({name}, {}) == {}, \"{name}.{} offset\");\n",
                field.name, field.offset, field.name
            ));
            if let Some(bytes) = field.bytes {
                source.push_str(&format!(
                    "_Static_assert(sizeof((({name} *)0)->{}) == {bytes}, \"{name}.{} size\");\n",
                    field.name, field.name
                ));
            }
        }
    }
    source
}

#[cfg(test)]
#[path = "layout_ut.rs"]
mod tests;
