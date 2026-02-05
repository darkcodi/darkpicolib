//! button.rs — simple GPIO button driver for rp2040
#![allow(dead_code)]

use embedded_hal::digital::InputPin;

/// Simple button driver with pull-up configuration (active-low).
pub struct Button<P> {
    pin: P,
}

impl<P> Button<P>
where
    P: InputPin,
{
    /// Create a new button wrapper.
    /// Caller must configure the pin as pull-up input before calling this.
    pub fn new(pin: P) -> Self {
        Self { pin }
    }

    /// Returns true if the button is currently pressed.
    /// Assumes active-low wiring (button connects to GND).
    pub fn is_pressed(&mut self) -> bool {
        // Active low - button pressed = low logic level
        self.pin.is_low().unwrap_or(false)
    }

    /// Returns true if the button is NOT pressed.
    pub fn is_released(&mut self) -> bool {
        !self.is_pressed()
    }
}
