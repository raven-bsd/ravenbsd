// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! Reference counting and object unloading.

use raven_libc_elf::ObjId;

use crate::loadobj::Loader;
use crate::mapper::Platform;

impl<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// Increment one reference.
    pub fn ref_dag(&mut self, id: ObjId) {
        if let Some(object) = self.object_mut(id) {
            object.refcount = object.refcount.saturating_add(1);
        }
    }

    /// Decrement one reference.
    pub fn unref_dag(&mut self, id: ObjId) {
        if let Some(object) = self.object_mut(id)
            && object.refcount > 0
        {
            object.refcount = object.refcount.saturating_sub(1);
        }
    }

    /// Unload an object when it is unreferenced; static-TLS and NODELETE objects stay.
    pub fn unload_object(&mut self, id: ObjId) -> bool {
        let Some(object) = self.object(id) else {
            return false;
        };
        if object.refcount != 0 || object.nodelete || object.has_tls() {
            return false;
        }
        let base = object.mapped.base;
        let span = object.mapped.span;
        if self
            .object_mut(id)
            .is_some_and(|object| object.refcount == 0)
        {
            if let Some(index) = usize::try_from(id.0).ok()
                && let Some(slot) = self.objects.get_mut(index)
            {
                *slot = None;
                self.count = self.count.saturating_sub(1);
            }
            let _ = self.platform.release(base, span);
            true
        } else {
            false
        }
    }
}
