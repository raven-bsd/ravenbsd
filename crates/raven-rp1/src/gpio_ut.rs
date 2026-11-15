// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the GPIO/pinctrl state machine.

use super::*;

fn name(text: &str) -> Name {
    Name::new(text).expect("name")
}

fn rules() -> Pinctrl {
    let mut rules = Pinctrl::new();
    assert!(rules.push(PinRule {
        pin: 14,
        function: name("uart0"),
        bias: Bias::Disable,
    }));
    assert!(rules.push(PinRule {
        pin: 15,
        function: name("uart0"),
        bias: Bias::PullUp,
    }));
    assert!(rules.push(PinRule {
        pin: 0,
        function: name("i2c0"),
        bias: Bias::PullUp,
    }));
    rules
}

#[test]
fn a_pin_has_exactly_one_owner() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert!(controller.claim(14, name("tty")).is_ok());
    assert_eq!(
        controller.claim(14, name("i2c")),
        Err(GpioError::AlreadyClaimed { pin: 14 })
    );
    assert_eq!(controller.owner(14), Some(name("tty")));
    assert!(controller.release(14, name("tty")).is_ok());
    assert!(controller.claim(14, name("i2c")).is_ok());
}

#[test]
fn only_the_owner_can_release_or_configure() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert!(controller.claim(14, name("tty")).is_ok());
    assert_eq!(
        controller.release(14, name("other")),
        Err(GpioError::NotOwner { pin: 14 })
    );
    assert_eq!(
        controller.configure(14, name("other"), None, Direction::Output, Bias::Disable),
        Err(GpioError::NotOwner { pin: 14 })
    );
}

#[test]
fn an_alternate_function_must_be_declared_for_the_pin() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert!(controller.claim(14, name("tty")).is_ok());
    assert!(
        controller
            .configure(
                14,
                name("tty"),
                Some(name("uart0")),
                Direction::Output,
                Bias::Disable
            )
            .is_ok()
    );
    assert_eq!(
        controller.configure(
            14,
            name("tty"),
            Some(name("i2c0")),
            Direction::Output,
            Bias::Disable
        ),
        Err(GpioError::FunctionNotOnPin {
            pin: 14,
            function: name("i2c0")
        })
    );
}

#[test]
fn plain_gpio_needs_no_pinctrl_rule() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert!(controller.claim(7, name("led")).is_ok());
    assert!(
        controller
            .configure(7, name("led"), None, Direction::Output, Bias::Disable)
            .is_ok()
    );
}

#[test]
fn only_an_output_can_be_driven() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert!(controller.claim(7, name("led")).is_ok());
    assert!(
        controller
            .configure(7, name("led"), None, Direction::Input, Bias::Disable)
            .is_ok()
    );
    assert_eq!(
        controller.set_high(7, name("led"), true),
        Err(GpioError::NotOutput { pin: 7 })
    );
    assert!(
        controller
            .configure(7, name("led"), None, Direction::Output, Bias::Disable)
            .is_ok()
    );
    assert!(controller.set_high(7, name("led"), true).is_ok());
    assert_eq!(controller.config(7).map(|config| config.high), Some(true));
}

#[test]
fn pins_outside_the_declared_range_are_rejected() {
    let mut controller = PinController::new(54, rules()).expect("controller");
    assert_eq!(
        controller.claim(54, name("tty")),
        Err(GpioError::PinOutOfRange(54))
    );
    assert_eq!(controller.config(54), None);
    assert_eq!(
        PinController::new(0, rules()).err(),
        Some(GpioError::PinCount(0))
    );
    assert_eq!(
        PinController::new(65, rules()).err(),
        Some(GpioError::PinCount(65))
    );
}

#[test]
fn the_rule_table_is_bounded() {
    let mut rules = Pinctrl::new();
    let rule = PinRule {
        pin: 1,
        function: name("f"),
        bias: Bias::Unspecified,
    };
    for _ in 0..MAX_RULES {
        assert!(rules.push(rule));
    }
    assert!(!rules.push(rule));
    assert_eq!(rules.len(), MAX_RULES);
    assert!(!rules.is_empty());
}

#[test]
fn the_declared_bias_is_visible() {
    let rules = rules();
    assert_eq!(rules.bias_for(15, name("uart0")), Some(Bias::PullUp));
    assert_eq!(rules.bias_for(14, name("uart1")), None);
}
