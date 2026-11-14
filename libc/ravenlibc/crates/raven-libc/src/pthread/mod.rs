// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The pthread shell family (C64); this pass binds the main-thread TLS initialiser.

#[path = "__raven_init_main_thread.rs"]
mod init_main_thread;
#[path = "pthread_equal.rs"]
mod pthread_equal;
#[path = "pthread_exit.rs"]
mod pthread_exit;
#[path = "pthread_self.rs"]
mod pthread_self;
#[path = "pthread_yield.rs"]
mod pthread_yield;
