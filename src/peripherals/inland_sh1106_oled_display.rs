use core::convert::Infallible;

use embassy_rp::gpio::Output;
use embassy_rp::spi::{self, Spi};
use embassy_time::{Duration, Instant, Timer};
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
pub const INLAND_SH1106_LOGS_REFRESH_INTERVAL_MS: u64 = 75;

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
    logs: [HeaplessString<32>; INLAND_SH1106_MAX_TEXT_LINES],
    head: usize,
    count: usize,
    dirty: bool,
    last_refresh: Option<Instant>,
}

impl<'d, T, M> LogsDisplay<'d, T, M>
where
    T: spi::Instance,
    M: spi::Mode,
{
    pub fn new(display: InlandSh1106OledDisplay<'d, T, M>) -> Self {
        let logs = [const { HeaplessString::new() }; INLAND_SH1106_MAX_TEXT_LINES];
        Self {
            display,
            logs,
            head: 0,
            count: 0,
            dirty: false,
            last_refresh: None,
        }
    }

    pub fn log(&mut self, msg: &str) {
        self.push_log(msg);
        self.dirty = true;
        self.refresh_if_due(false);
    }

    pub fn flush(&mut self) {
        self.refresh_if_due(true);
    }

    fn push_log(&mut self, msg: &str) {
        let insert_at = if self.count < INLAND_SH1106_MAX_TEXT_LINES {
            let idx = (self.head + self.count) % INLAND_SH1106_MAX_TEXT_LINES;
            self.count += 1;
            idx
        } else {
            let idx = self.head;
            self.head = (self.head + 1) % INLAND_SH1106_MAX_TEXT_LINES;
            idx
        };

        self.logs[insert_at].clear();
        for c in msg.chars().take(INLAND_SH1106_MAX_CHARS_PER_LINE) {
            let _ = self.logs[insert_at].push(c);
        }
    }

    fn refresh_if_due(&mut self, force: bool) {
        if !self.dirty {
            return;
        }

        let now = Instant::now();
        if !force {
            if let Some(last_refresh) = self.last_refresh {
                let next_refresh = last_refresh + Duration::from_millis(INLAND_SH1106_LOGS_REFRESH_INTERVAL_MS);
                if now < next_refresh {
                    return;
                }
            }
        }

        let mut lines: [&str; INLAND_SH1106_MAX_TEXT_LINES] = [""; INLAND_SH1106_MAX_TEXT_LINES];
        let pad = INLAND_SH1106_MAX_TEXT_LINES - self.count;
        for i in 0..self.count {
            let idx = (self.head + i) % INLAND_SH1106_MAX_TEXT_LINES;
            lines[pad + i] = self.logs[idx].as_str();
        }

        if self.display.display_str_arr(&lines).is_ok() {
            self.dirty = false;
            self.last_refresh = Some(now);
        }
    }
}
