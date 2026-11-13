// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::cell::RefCell;

struct Hooks {
    order: RefCell<std::vec::Vec<&'static str>>,
    logged: RefCell<std::vec::Vec<u8>>,
}

impl Hooks {
    fn new() -> Self {
        Self {
            order: RefCell::new(std::vec::Vec::new()),
            logged: RefCell::new(std::vec::Vec::new()),
        }
    }
}

impl FailHooks for Hooks {
    fn block_signals_except_abort(&self) {
        self.order.borrow_mut().push("block");
    }
    fn log(&self, msg: &[u8]) {
        self.order.borrow_mut().push("log");
        self.logged.borrow_mut().extend_from_slice(msg);
    }
    fn reset_abort_to_default(&self) {
        self.order.borrow_mut().push("reset");
    }
    fn raise_abort(&self) {
        self.order.borrow_mut().push("raise");
    }
    fn exit_now(&self, status: i32) -> ! {
        self.order.borrow_mut().push("exit");
        panic!("exit {status}")
    }
}

#[test]
fn stack_failure_order_and_message() {
    let hooks = Hooks::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stack_chk_fail(&hooks)));
    assert!(result.is_err());
    assert_eq!(
        *hooks.order.borrow(),
        ["block", "log", "reset", "raise", "exit"]
    );
    assert_eq!(&*hooks.logged.borrow(), MSG_STACK);
}

#[test]
fn chk_failure_message() {
    let hooks = Hooks::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| chk_fail(&hooks)));
    assert!(result.is_err());
    assert_eq!(&*hooks.logged.borrow(), MSG_CHK);
}

#[test]
fn exit_status_is_one_hundred_twenty_seven() {
    let hooks = Hooks::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| chk_fail(&hooks)));
    let payload = result.expect_err("must panic");
    let text = payload
        .downcast_ref::<String>()
        .map(|value| value.as_str())
        .unwrap_or("");
    assert!(text.contains("127"));
}

#[test]
fn messages_are_exact_and_distinct() {
    assert_eq!(MSG_STACK, b"stack overflow detected; terminated");
    assert_eq!(MSG_CHK, b"buffer overflow detected; terminated");
}

#[test]
fn raise_returning_still_reaches_exit() {
    struct Returning;
    impl FailHooks for Returning {
        fn block_signals_except_abort(&self) {}
        fn log(&self, _msg: &[u8]) {}
        fn reset_abort_to_default(&self) {}
        fn raise_abort(&self) {}
        fn exit_now(&self, _status: i32) -> ! {
            panic!("exit reached")
        }
    }
    let result = std::panic::catch_unwind(|| chk_fail(&Returning));
    assert!(result.is_err());
}
