// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `dl*` forwarders to C62's loader table.

#[path = "dl_iterate_phdr.rs"]
mod dl_iterate_phdr;
#[path = "dladdr.rs"]
mod dladdr;
#[path = "dlclose.rs"]
mod dlclose;
#[path = "dlerror.rs"]
mod dlerror;
#[path = "dlinfo.rs"]
mod dlinfo;
#[path = "dlopen.rs"]
mod dlopen;
#[path = "dlsym.rs"]
mod dlsym;
#[path = "dlvsym.rs"]
mod dlvsym;
#[path = "table.rs"]
mod table;
