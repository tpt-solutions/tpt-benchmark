//! Backend for platforms without a hardware-counter implementation.
//!
//! [`Never`] is uninhabited, so `HardwareCounter` can never hold a live
//! counter here. Its methods are unreachable without panicking code.

use super::{Counter, Unavailable};

#[cfg(target_os = "macos")]
const REASON: &str = "kperf (private API, root required) not implemented in this build";
#[cfg(target_os = "windows")]
const REASON: &str =
    "hardware counters not supported on Windows in this build; see docs/hardware-counters.md";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const REASON: &str = "hardware counters are not supported on this platform";

/// An uninhabited backend: no counter can ever be opened.
#[derive(Debug)]
pub(crate) enum Never {}

impl Never {
    /// Always fails with the platform's documented reason.
    pub(crate) fn open(_counter: Counter) -> Result<Self, Unavailable> {
        Err(Unavailable {
            reason: REASON.to_owned(),
        })
    }

    pub(crate) fn start(&mut self) {
        match *self {}
    }

    pub(crate) fn end(&mut self) -> f64 {
        match *self {}
    }
}
