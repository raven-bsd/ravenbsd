// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NDJSON output for `ls --json`.

use std::os::unix::fs::MetadataExt;

use uucore::error::{UResult, USimpleError};
use uucore::raven::{Record, Value};

use crate::output::{EntryInfo, LsOutput, StreamMode};

/// One NDJSON file record per listed entry.
pub(crate) struct JsonOutput;

impl LsOutput for JsonOutput {
    fn stream_mode(&self) -> StreamMode {
        StreamMode::Streaming
    }

    fn write_entry(&mut self, entry: &EntryInfo) -> UResult<()> {
        let metadata = entry.metadata.as_ref();
        let kind = if entry.is_dir() {
            "directory"
        } else if entry
            .file_type
            .as_ref()
            .is_some_and(std::fs::FileType::is_symlink)
        {
            "symlink"
        } else if entry.is_file() {
            "file"
        } else {
            "other"
        };
        let mut record = Record::new("file")
            .string("path", &entry.path.to_string_lossy())
            .string("name", &entry.display_name.to_string_lossy())
            .string("type", kind)
            .u64("size", metadata.map_or(0, std::fs::Metadata::len))
            .u64(
                "mode",
                metadata.map_or(0, |metadata| u64::from(metadata.mode())),
            )
            .u64(
                "uid",
                metadata.map_or(0, |metadata| u64::from(metadata.uid())),
            )
            .u64(
                "gid",
                metadata.map_or(0, |metadata| u64::from(metadata.gid())),
            )
            .i64("mtime", metadata.map_or(0, std::fs::Metadata::mtime));
        if let Ok(target) = std::fs::read_link(&entry.path) {
            record = record.field(
                "target",
                Value::String(target.to_string_lossy().into_owned()),
            );
        }
        uucore::raven::write_records(&[record])
            .map_err(|error| USimpleError::new(1, error.to_string()))
    }
}
