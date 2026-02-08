use core::convert::Infallible;

use embassy_rp::gpio::Output;
use embassy_rp::spi::{self, Spi};
use embassy_time::Timer;
use embedded_graphics::mono_font::{MonoTextStyle, ascii::FONT_4X6};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use sh1106::{Builder, prelude::*};
use crate::HeaplessString;

pub const INLAND_SH1106_WIDTH: u8 = 128;
pub const INLAND_SH1106_HEIGHT: u8 = 64;
pub const INLAND_SH1106_TEXT_LINE_HEIGHT: i32 = 6;
pub const INLAND_SH1106_MAX_TEXT_LINES: usize = 10;
pub const INLAND_SH1106_MAX_CHARS_PER_LINE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format, thiserror::Error)]
pub enum InlandSh1106OledError {
    #[error("OLED communication with SH1106 failed")]
    Communication,
    #[error("OLED pin operation failed")]
    Pin,
    #[error("String contains too many lines for SH1106 display: {actual_lines} > {max_lines}")]
    TooManyLines {
        actual_lines: usize,
        max_lines: usize,
    },
    #[error("Line {line_index} is too long for SH1106 display: {actual_chars} > {max_chars}")]
    LineTooLong {
        line_index: usize,
        actual_chars: usize,
        max_chars: usize,
    },
}

pub fn inland_sh1106_default_spi_config() -> spi::Config {
    let mut cfg = spi::Config::default();
    cfg.frequency = 10_000_000;
    cfg.phase = spi::Phase::CaptureOnFirstTransition;
    cfg.polarity = spi::Polarity::IdleLow;
    cfg
}

pub async fn inland_sh1106_hardware_reset<RST>(rst: &mut RST) -> Result<(), InlandSh1106OledError>
where
    RST: embedded_hal::digital::OutputPin,
{
    rst.set_low().map_err(|_| InlandSh1106OledError::Pin)?;
    Timer::after_millis(10).await;
    rst.set_high().map_err(|_| InlandSh1106OledError::Pin)?;
    Timer::after_millis(10).await;
    Ok(())
}

pub struct InlandSh1106OledDisplay<'d, T, M>
where
    T: spi::Instance,
    M: spi::Mode,
{
    display: GraphicsMode<SpiInterface<Spi<'d, T, M>, Output<'d>, Output<'d>>>,
}

impl<'d, T, M> InlandSh1106OledDisplay<'d, T, M>
where
    T: spi::Instance,
    M: spi::Mode,
{
    pub fn new(spi: Spi<'d, T, M>, dc: Output<'d>, cs: Output<'d>) -> Self {
        let display: GraphicsMode<_> = Builder::new().connect_spi(spi, dc, cs).into();
        Self { display }
    }

    pub fn init(&mut self) -> Result<(), InlandSh1106OledError> {
        self.display
            .init()
            .map_err(map_sh1106_error::<embassy_rp::spi::Error, Infallible>)?;
        self.display
            .flush()
            .map_err(map_sh1106_error::<embassy_rp::spi::Error, Infallible>)?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), InlandSh1106OledError> {
        self.display.clear();
        self.flush()
    }

    pub fn flush(&mut self) -> Result<(), InlandSh1106OledError> {
        self.display
            .flush()
            .map_err(map_sh1106_error::<embassy_rp::spi::Error, Infallible>)
    }

    /// Display multi-line text using the 4x6 mono font.
    ///
    /// Lines are separated by `\n`, up to 10 lines total and 32 chars per line.
    pub fn display_str(&mut self, content: &str) -> Result<(), InlandSh1106OledError> {
        let mut line_count = 0usize;
        for (line_index, line) in content.split('\n').enumerate() {
            line_count += 1;
            if line_count > INLAND_SH1106_MAX_TEXT_LINES {
                return Err(InlandSh1106OledError::TooManyLines {
                    actual_lines: line_count,
                    max_lines: INLAND_SH1106_MAX_TEXT_LINES,
                });
            }

            let chars = line.chars().count();
            if chars > INLAND_SH1106_MAX_CHARS_PER_LINE {
                return Err(InlandSh1106OledError::LineTooLong {
                    line_index,
                    actual_chars: chars,
                    max_chars: INLAND_SH1106_MAX_CHARS_PER_LINE,
                });
            }
        }

        self.display.clear();
        let style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

        for (line_index, line) in content.split('\n').enumerate() {
            let y = ((line_index as i32) + 1) * INLAND_SH1106_TEXT_LINE_HEIGHT;
            let _ = Text::new(line, Point::new(0, y), style).draw(&mut self.display);
        }

        self.flush()
    }

    pub fn display_str_arr(&mut self, lines: &[&str]) -> Result<(), InlandSh1106OledError> {
        let line_count = lines.len();
        if line_count > INLAND_SH1106_MAX_TEXT_LINES {
            return Err(InlandSh1106OledError::TooManyLines {
                actual_lines: line_count,
                max_lines: INLAND_SH1106_MAX_TEXT_LINES,
            });
        }

        for (line_index, line) in lines.iter().enumerate() {
            let chars = line.chars().count();
            if chars > INLAND_SH1106_MAX_CHARS_PER_LINE {
                return Err(InlandSh1106OledError::LineTooLong {
                    line_index,
                    actual_chars: chars,
                    max_chars: INLAND_SH1106_MAX_CHARS_PER_LINE,
                });
            }
        }

        self.display.clear();
        let style = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);

        for (line_index, line) in lines.iter().enumerate() {
            let y = ((line_index as i32) + 1) * INLAND_SH1106_TEXT_LINE_HEIGHT;
            let _ = Text::new(line, Point::new(0, y), style).draw(&mut self.display);
        }

        self.flush()
    }

    pub fn display_mut(
        &mut self,
    ) -> &mut GraphicsMode<SpiInterface<Spi<'d, T, M>, Output<'d>, Output<'d>>> {
        &mut self.display
    }
}

fn map_sh1106_error<CommE, PinE>(err: sh1106::Error<CommE, PinE>) -> InlandSh1106OledError {
    match err {
        sh1106::Error::Comm(_) => InlandSh1106OledError::Communication,
        sh1106::Error::Pin(_) => InlandSh1106OledError::Pin,
    }
}

pub struct LogsDisplay<'d, T, M>
where
    T: spi::Instance,
    M: spi::Mode,
{
    display: InlandSh1106OledDisplay<'d, T, M>,
    logs: [HeaplessString<32>; 10],
}

impl<'d, T, M> LogsDisplay<'d, T, M>
where
    T: spi::Instance,
    M: spi::Mode,
{
    pub fn new(display: InlandSh1106OledDisplay<'d, T, M>) -> Self {
        let logs = [const { HeaplessString::new() }; 10];
        Self { display, logs }
    }

    pub fn log(&mut self, msg: &str) {
        // Shift existing logs up
        for i in 0..(self.logs.len() - 1) {
            self.logs[i] = self.logs[i + 1].clone();
        }
        // Add new log at the bottom
        let mut last_log_str: HeaplessString<32> = HeaplessString::new();
        for c in msg.chars().take(32) {
            let _ = last_log_str.push(c); // Truncate if message is too long
        }
        self.logs[9] = last_log_str;

        // Display logs on OLED
        let logs_arr: [&str; 10] = [
            self.logs[0].as_str(),
            self.logs[1].as_str(),
            self.logs[2].as_str(),
            self.logs[3].as_str(),
            self.logs[4].as_str(),
            self.logs[5].as_str(),
            self.logs[6].as_str(),
            self.logs[7].as_str(),
            self.logs[8].as_str(),
            self.logs[9].as_str(),
        ];
        let _ = self.display.display_str_arr(&logs_arr); // Ignore display errors
    }
}
