//! servo.rs — hobby-servo driver for embassy-rp PWM
#![allow(dead_code)]

use core::cmp::{max, min};
use embassy_rp::pwm::Pwm;
use embedded_hal::pwm::SetDutyCycle;
use fixed::FixedU16;
use fixed::types::extra::U4;

/// Servo signal specification (all in microseconds / degrees).
#[derive(Copy, Clone, Debug)]
pub struct ServoSpec {
    /// Full frame period (e.g. 20_000 for 50 Hz).
    pub frame_us: u32,

    /// Min pulse width (e.g. 1000).
    pub pulse_min_us: u32,

    /// Max pulse width (e.g. 2000).
    pub pulse_max_us: u32,

    /// Angle corresponding to min pulse (e.g. 0.0).
    pub angle_min_deg: f32,

    /// Angle corresponding to max pulse (e.g. 90.0 or 180.0).
    pub angle_max_deg: f32,
}

impl ServoSpec {
    /// Inland KS0209 Blue 9g Servo Motor
    pub fn inland_ks0209() -> &'static Self {
        const KS0209: ServoSpec = ServoSpec {
            frame_us: 20_000,   // 20 ms
            pulse_min_us: 1000, // 1 ms (0 degree)
            pulse_max_us: 2000, // 2 ms (90 degree)
            angle_min_deg: 0.0,
            angle_max_deg: 90.0,
        };

        &KS0209
    }

    /// MakerHawk MG-995 DIGI Hi-Speed
    pub fn makerhawk_mg995() -> &'static Self {
        const MG995: ServoSpec = ServoSpec {
            frame_us: 20_000,
            pulse_min_us: 500,  // 0.5 ms (0 degree)
            pulse_max_us: 2500, // 2.5 ms (120 degree)
            angle_min_deg: 0.0,
            angle_max_deg: 180.0,
        };

        &MG995
    }
}

#[derive(Debug, Clone)]
/// Servo driver configuration
pub struct ServoConfig {
    /// PWM top value (period - 1)
    pub top: u16,
    /// PWM divider (FixedU16 with 4 fractional bits)
    pub divider: FixedU16<U4>,
    /// Tick rate in Hz
    pub tick_hz: u32,

    /// Minimum angle in degrees
    pub angle_min: f32,
    /// Maximum angle in degrees
    pub angle_max: f32,

    /// Minimum duty cycle count
    pub duty_min: u16,
    /// Maximum duty cycle count
    pub duty_max: u16,
}

impl ServoConfig {
    /// Create + configure the PWM slice.
    ///
    /// - `pwm` is the embassy-rp PWM instance
    /// - `pwm_clock_hz` is the source clock feeding the PWM peripheral (125_000_000)
    /// - This config uses edge-aligned PWM (recommended for servos)
    pub fn new(pwm: &mut Pwm<'_>, pwm_clock_hz: u32, spec: &ServoSpec) -> Self {
        let config = Self::new_precomputed(pwm_clock_hz, spec);

        // Configure PWM with embassy-rp API
        let mut pwm_config = embassy_rp::pwm::Config::default();
        pwm_config.top = config.top;
        pwm_config.divider = config.divider;
        pwm.set_config(&pwm_config);

        config
    }

    /// Pre-compute servo configuration without needing a PWM instance.
    /// Returns a ServoConfig that can be used to create a PWM with the correct settings.
    ///
    /// - `pwm_clock_hz` is the source clock feeding the PWM peripheral (125_000_000)
    /// - This config uses edge-aligned PWM (recommended for servos)
    pub fn new_precomputed(pwm_clock_hz: u32, spec: &ServoSpec) -> Self {
        // Sanity clamps
        let frame_us = max(1, spec.frame_us);
        let pulse_min_us = min(spec.pulse_min_us, frame_us.saturating_sub(1));
        let pulse_max_us = min(max(spec.pulse_max_us, pulse_min_us + 1), frame_us);

        // Choose a PWM tick rate and divider so that TOP fits in u16.
        // We prefer 1 MHz (1 tick = 1 us) when possible.
        let mut divider_q4: u32;
        let mut tick_hz: u32;
        let mut top: u32;

        // Start with a target tick rate, then adjust divider until TOP fits.
        let mut target_tick_hz: u32 = 1_000_000;

        // If frame is longer than 65536us, 1MHz won't fit in u16 TOP.
        // Drop tick rate so frame_us * tick_hz <= 65536 * 1_000_000.
        // (i.e. TOP <= 65535)
        let max_tick_hz_for_top = ((u16::MAX as u64 + 1) * 1_000_000u64 / frame_us as u64) as u32;
        target_tick_hz = min(target_tick_hz, max(1, max_tick_hz_for_top));

        // Compute an initial divider (Q4 fixed-point: int.frac/16)
        // divider_q4 ~= clock_hz * 16 / target_tick_hz
        divider_q4 = (((pwm_clock_hz as u64) * 16u64 + (target_tick_hz as u64 / 2))
            / (target_tick_hz as u64)) as u32;

        // Divider must be at least 1.0 (16 in Q4) and at most 255.9375 (255*16 + 15).
        divider_q4 = min(max(divider_q4, 16), 255 * 16 + 15);

        // Now bump divider upward until TOP fits (or we hit max divider).
        loop {
            tick_hz = ((pwm_clock_hz as u64) * 16u64 / divider_q4 as u64) as u32;
            // period_ticks = frame_us * tick_hz / 1_000_000
            let period_ticks = (frame_us as u64) * (tick_hz as u64) / 1_000_000u64;
            top = period_ticks.saturating_sub(1) as u32;

            if top <= u16::MAX as u32 || divider_q4 >= (255 * 16 + 15) {
                break;
            }
            divider_q4 += 1; // slightly slower tick -> smaller TOP
        }

        let div_int = (divider_q4 / 16) as u8;
        let div_frac = (divider_q4 % 16) as u8;

        let top_u16 = min(top, u16::MAX as u32) as u16;

        // divider is a FixedU16 representing the divider value
        let divider_val = FixedU16::<U4>::from_bits(((div_int as u16) << 4) | (div_frac as u16));

        // Convert pulse widths to duty counts.
        let duty_min = us_to_counts(pulse_min_us, tick_hz, top_u16);
        let duty_max = us_to_counts(pulse_max_us, tick_hz, top_u16);

        Self {
            top: top_u16,
            divider: divider_val,
            tick_hz,
            angle_min: spec.angle_min_deg,
            angle_max: spec.angle_max_deg,
            duty_min,
            duty_max,
        }
    }
}

/// Servo driver
pub struct Servo<'a> {
    pwm: Pwm<'a>,
    config: ServoConfig,
}

impl<'a> Servo<'a> {
    pub fn new(pwm: Pwm<'a>, config: ServoConfig) -> Self {
        Self { pwm, config }
    }

    /// Set the servo angle in degrees. Values outside the spec are clamped.
    pub fn set_angle(&mut self, angle_deg: f32) -> Result<(), ServoError> {
        // Handle weird specs safely.
        let (a0, a1) = (self.config.angle_min, self.config.angle_max);
        if (a1 - a0).abs() < f32::EPSILON {
            self.pwm
                .set_duty_cycle(self.config.duty_min)
                .map_err(|_| ServoError::SetDutyCycle)?;
            return Ok(());
        }

        // Clamp + normalize
        let a = angle_deg.clamp(a0.min(a1), a0.max(a1));
        let t = (a - a0) / (a1 - a0); // 0..1, works even if a1 < a0

        // Interpolate duty
        let d0 = self.config.duty_min as i32;
        let d1 = self.config.duty_max as i32;
        let duty = libm::round((d0 as f32 + t * ((d1 - d0) as f32)) as f64) as i32;

        // Clamp to [0..TOP] just in case
        let duty = duty.clamp(0, self.config.top as i32) as u16;
        self.pwm
            .set_duty_cycle(duty)
            .map_err(|_| ServoError::SetDutyCycle)?;
        Ok(())
    }
}

#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum ServoError {
    #[error("Failed to set duty cycle")]
    SetDutyCycle,
}

fn us_to_counts(pulse_us: u32, tick_hz: u32, top: u16) -> u16 {
    // counts = pulse_us * tick_hz / 1_000_000, rounded
    let counts = ((pulse_us as u64) * (tick_hz as u64) + 500_000u64) / 1_000_000u64;
    let counts = min(counts as u32, top as u32);
    counts as u16
}
