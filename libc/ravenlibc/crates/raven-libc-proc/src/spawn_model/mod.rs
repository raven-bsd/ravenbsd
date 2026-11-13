// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `posix_spawn` model.

pub mod api;
pub mod apply;
pub mod driver;
pub mod order;

pub use api::{FileAction, FileActions, SpawnAttr};
pub use apply::{FdEntry, Scratch, SpawnEnv, apply_actions, resolve};
pub use driver::{SpawnCtx, posix_spawn_model, spawn_model_pid, spawn_with_mask};
pub use order::{AttrStep, attr_steps};
