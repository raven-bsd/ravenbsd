// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

use crate::refs;

pub(crate) struct ReedlineValidator<SE: brush_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
}

impl<SE: brush_core::ShellExtensions> reedline::Validator for ReedlineValidator<SE> {
    fn validate(&self, line: &str) -> reedline::ValidationResult {
        if crate::completeness::needs_more_input(&self.shell, line) {
            reedline::ValidationResult::Incomplete
        } else {
            reedline::ValidationResult::Complete
        }
    }
}
