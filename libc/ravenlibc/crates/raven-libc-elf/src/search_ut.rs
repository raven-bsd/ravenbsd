// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The path tables are short fixed literals.

//! Library search order and `$ORIGIN`/`$PLATFORM` expansion.

use alloc::vec::Vec;

use super::*;
use crate::errors::SearchError;
use crate::limits::{MAX_PATH_LEN, MAX_SEARCH_CANDIDATES, MAX_SEARCH_PATH_LEN};
use crate::scope::ObjId;

const DIRS: &[&[u8]] = &[b"/lib", b"/usr/lib", b"/usr/local/lib"];

fn origin_some(_: ObjId) -> Option<Vec<u8>> {
    Some(b"/opt/app".to_vec())
}

fn origin_none(_: ObjId) -> Option<Vec<u8>> {
    None
}

fn make_context<'a>(
    ld: Option<&'a [u8]>,
    origin: &'a dyn Fn(ObjId) -> Option<Vec<u8>>,
) -> SearchCtx<'a> {
    SearchCtx {
        trust: true,
        ld_library_path: ld,
        library_path_rpath: false,
        main_rpath: None,
        default_dirs: DIRS,
        origin_of: origin,
        platform: b"x86_64",
    }
}

fn object<'a>(rpath: Option<&'a [u8]>, runpath: Option<&'a [u8]>) -> RefObj<'a> {
    RefObj {
        id: ObjId(1),
        rpath,
        runpath,
        nodeflib: false,
        is_main: false,
    }
}

fn expand(ctx: &SearchCtx<'_>, name: &[u8], refobj: Option<RefObj<'_>>) -> Vec<Vec<u8>> {
    expand_search(ctx, name, refobj).unwrap()
}

#[test]
fn an_absolute_name_is_returned_alone() {
    let context = make_context(None, &origin_some);
    assert_eq!(
        expand(&context, b"/opt/lib/libx.so.1", None),
        vec![b"/opt/lib/libx.so.1".to_vec()]
    );
}

#[test]
fn a_name_with_a_slash_is_not_searched_in_the_paths() {
    let context = make_context(Some(b"/tmp"), &origin_some);
    let result = expand(
        &context,
        b"sub/liby.so",
        Some(object(Some(b"/rpath"), None)),
    );
    assert_eq!(result, vec![b"sub/liby.so".to_vec()]);
}

#[test]
fn a_relative_slash_name_in_secure_mode_is_rejected() {
    let mut context = make_context(None, &origin_some);
    context.trust = false;
    assert_eq!(
        expand_search(&context, b"sub/liby.so", None),
        Err(SearchError::NotAbsolute)
    );
    // An absolute name is still fine.
    assert!(expand_search(&context, b"/abs/liby.so", None).is_ok());
}

#[test]
fn an_rpath_only_object_searches_its_rpath_then_the_defaults() {
    let context = make_context(None, &origin_some);
    let result = expand(&context, b"libz.so.1", Some(object(Some(b"/rpath"), None)));
    assert_eq!(
        result,
        vec![
            b"/rpath/libz.so.1".to_vec(),
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn an_rpath_is_ignored_when_a_runpath_is_present() {
    let context = make_context(None, &origin_some);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"/rpath"), Some(b"/runpath"))),
    );
    assert_eq!(
        result,
        vec![
            b"/runpath/libz.so.1".to_vec(),
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn the_main_program_rpath_is_used_for_a_dependency_without_runpath() {
    let mut context = make_context(None, &origin_some);
    context.main_rpath = Some(b"/main/rpath");
    let result = expand(&context, b"libz.so.1", Some(object(None, None)));
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/main/rpath/libz.so.1"[..])
    );
}

#[test]
fn the_main_program_rpath_is_not_used_for_the_main_program_itself() {
    let mut context = make_context(None, &origin_some);
    context.main_rpath = Some(b"/main/rpath");
    let mut reference = object(None, None);
    reference.is_main = true;
    let result = expand(&context, b"libz.so.1", Some(reference));
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/lib/libz.so.1"[..])
    );
}

#[test]
fn ld_library_path_sits_between_rpath_and_runpath() {
    let context = make_context(Some(b"/env"), &origin_some);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"/rpath"), Some(b"/runpath"))),
    );
    assert_eq!(
        result,
        vec![
            b"/env/libz.so.1".to_vec(),
            b"/runpath/libz.so.1".to_vec(),
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn the_pre_conforming_order_puts_ld_library_path_first() {
    let mut context = make_context(Some(b"/env"), &origin_some);
    context.library_path_rpath = true;
    let result = expand(&context, b"libz.so.1", Some(object(Some(b"/rpath"), None)));
    assert_eq!(
        result,
        vec![
            b"/env/libz.so.1".to_vec(),
            b"/rpath/libz.so.1".to_vec(),
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn the_pre_conforming_order_ignores_nodeflib() {
    let mut context = make_context(Some(b"/env"), &origin_some);
    context.library_path_rpath = true;
    let mut reference = object(Some(b"/rpath"), None);
    reference.nodeflib = true;
    let result = expand(&context, b"libz.so.1", Some(reference));
    assert!(result.contains(&b"/lib/libz.so.1".to_vec()));
}

#[test]
fn nodeflib_drops_the_default_directories() {
    let context = make_context(None, &origin_some);
    let mut reference = object(Some(b"/rpath"), None);
    reference.nodeflib = true;
    let result = expand(&context, b"libz.so.1", Some(reference));
    assert_eq!(result, vec![b"/rpath/libz.so.1".to_vec()]);
}

#[test]
fn origin_is_expanded_in_an_rpath() {
    let context = make_context(None, &origin_some);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"$ORIGIN/../lib"), None)),
    );
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/opt/app/../lib/libz.so.1"[..])
    );
}

#[test]
fn the_braced_origin_is_expanded() {
    let context = make_context(None, &origin_some);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"${ORIGIN}/lib"), None)),
    );
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/opt/app/lib/libz.so.1"[..])
    );
}

#[test]
fn an_unresolved_origin_left_a_candidate_out() {
    let context = make_context(None, &origin_none);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"$ORIGIN/lib"), None)),
    );
    assert_eq!(
        result,
        vec![
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn secure_mode_ignores_ld_library_path() {
    let mut context = make_context(Some(b"/env"), &origin_some);
    context.trust = false;
    let result = expand(&context, b"libz.so.1", None);
    assert_eq!(
        result,
        vec![
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn secure_mode_does_not_expand_origin() {
    let mut context = make_context(None, &origin_some);
    context.trust = false;
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"$ORIGIN/lib"), None)),
    );
    assert!(result.iter().all(|path| !path.starts_with(b"/opt/app")));
    assert!(result.iter().all(|path| !path.contains(&b'$')));
}

#[test]
fn secure_mode_skips_a_relative_directory() {
    let mut context = make_context(None, &origin_some);
    context.trust = false;
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"relative"), None)),
    );
    assert_eq!(
        result,
        vec![
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}

#[test]
fn both_separators_are_understood() {
    let context = make_context(Some(b"/one:/two;/three"), &origin_some);
    let result = expand(&context, b"libz.so.1", None);
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/one/libz.so.1"[..])
    );
    assert!(result.contains(&b"/two/libz.so.1".to_vec()));
    assert!(result.contains(&b"/three/libz.so.1".to_vec()));
}

#[test]
fn empty_elements_are_skipped() {
    let context = make_context(Some(b"::/one::/two:"), &origin_some);
    let result = expand(&context, b"libz.so.1", None);
    let expected: Vec<Vec<u8>> = vec![
        b"/one/libz.so.1".to_vec(),
        b"/two/libz.so.1".to_vec(),
        b"/lib/libz.so.1".to_vec(),
        b"/usr/lib/libz.so.1".to_vec(),
        b"/usr/local/lib/libz.so.1".to_vec(),
    ];
    assert_eq!(result, expected);
}

#[test]
fn a_leading_and_trailing_separator_is_skipped() {
    let context = make_context(Some(b";/only;"), &origin_some);
    let result = expand(&context, b"libz.so.1", None);
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/only/libz.so.1"[..])
    );
    assert_eq!(result.len(), 4);
}

#[test]
fn a_sixty_four_kibibyte_library_path_is_accepted() {
    let mut path = vec![b'/'];
    path.extend_from_slice(b"lib");
    while path.len() < MAX_SEARCH_PATH_LEN {
        path.push(b'a');
    }
    let context = make_context(Some(&path), &origin_some);
    // One very long element; its candidate exceeds PATH_MAX and is skipped, but no error.
    let result = expand_search(&context, b"libz.so.1", None).unwrap();
    assert!(result.iter().all(|path| path.len() <= MAX_PATH_LEN));
}

#[test]
fn a_path_longer_than_the_cap_is_rejected() {
    let path = vec![b'/'; MAX_SEARCH_PATH_LEN + 1];
    let context = make_context(Some(&path), &origin_some);
    assert_eq!(
        expand_search(&context, b"libz.so.1", None),
        Err(SearchError::PathTooLong)
    );
}

#[test]
fn more_than_the_candidate_cap_is_rejected() {
    let mut path = Vec::new();
    for _ in 0..(MAX_SEARCH_CANDIDATES + 10) {
        if !path.is_empty() {
            path.push(b':');
        }
        path.extend_from_slice(b"/d");
    }
    let context = make_context(Some(&path), &origin_some);
    assert_eq!(
        expand_search(&context, b"libz.so.1", None),
        Err(SearchError::TooManyCandidates)
    );
}

#[test]
fn a_candidate_over_path_max_is_skipped() {
    let mut long = Vec::new();
    long.push(b'/');
    while long.len() < MAX_PATH_LEN - 4 {
        long.push(b'a');
    }
    let mut path = long.clone();
    path.push(b':');
    path.extend_from_slice(b"/ok");
    let context = make_context(Some(&path), &origin_some);
    let result = expand(&context, b"libz.so.1", None);
    assert!(
        result
            .iter()
            .all(|candidate| candidate.len() <= MAX_PATH_LEN)
    );
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/ok/libz.so.1"[..])
    );
}

#[test]
fn platform_is_expanded() {
    let context = make_context(None, &origin_some);
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"/$PLATFORM/lib"), None)),
    );
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/x86_64/lib/libz.so.1"[..])
    );
    let result = expand(
        &context,
        b"libz.so.1",
        Some(object(Some(b"/${PLATFORM}/lib"), None)),
    );
    assert_eq!(
        result.first().map(Vec::as_slice),
        Some(&b"/x86_64/lib/libz.so.1"[..])
    );
}

#[test]
fn basename_returns_the_last_component() {
    assert_eq!(basename(b"/a/b/libz.so.1"), b"libz.so.1");
    assert_eq!(basename(b"libz.so.1"), b"libz.so.1");
    assert_eq!(basename(b"/"), b"");
    assert_eq!(basename(b""), b"");
}

#[test]
fn origin_subst_leaves_unknown_tokens_and_secure_input_alone() {
    let context = make_context(None, &origin_some);
    assert_eq!(
        origin_subst(&context, None, b"$ORIGIN/x").unwrap(),
        b"$ORIGIN/x"
    );
    assert_eq!(
        origin_subst(&context, Some(ObjId(1)), b"$LIB/x").unwrap(),
        b"$LIB/x"
    );
    let mut secure = make_context(None, &origin_some);
    secure.trust = false;
    assert_eq!(
        origin_subst(&secure, Some(ObjId(1)), b"$ORIGIN/x").unwrap(),
        b"$ORIGIN/x"
    );
}

#[test]
fn a_reference_without_attributes_still_gets_the_defaults() {
    let context = make_context(None, &origin_some);
    let result = expand(&context, b"libz.so.1", None);
    assert_eq!(
        result,
        vec![
            b"/lib/libz.so.1".to_vec(),
            b"/usr/lib/libz.so.1".to_vec(),
            b"/usr/local/lib/libz.so.1".to_vec()
        ]
    );
}
