use embassy_time::Delay;
use i2c_character_display::{CharacterDisplayPCF8574T, LcdDisplayType};

#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum InlandKs0061I2cDisplayError {
    #[error("LCD initialization failed")]
    Initialization,
    #[error("Failed to set LCD backlight")]
    Backlight,
    #[error("Failed to clear LCD display")]
    Clear,
    #[error("Failed to set cursor position on LCD display")]
    SetCursor,
    #[error("Failed to print message on LCD display")]
    Print,
    #[error("Invalid string for LCD display: {0}")]
    InvalidContent(#[from] InlandKs0061ContentError),
}

pub const INLAND_KS0061_COLS: usize = 16;
pub const INLAND_KS0061_ROWS: usize = 2;
pub const INLAND_KS0061_MAX_CHARS_PER_LINE: usize = INLAND_KS0061_COLS;
pub const INLAND_KS0061_MAX_CHARS_TOTAL: usize = INLAND_KS0061_COLS * INLAND_KS0061_ROWS;
pub const INLAND_KS0061_DEFAULT_I2C_ADDRESS: u8 = 0x27;

pub const fn inland_ks0061_default_i2c_address() -> u8 {
    INLAND_KS0061_DEFAULT_I2C_ADDRESS
}

#[derive(Debug, defmt::Format, Clone, PartialEq, Eq)]
pub struct InlandKs0061Line(heapless::String<INLAND_KS0061_MAX_CHARS_PER_LINE>);

#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum InlandKs0061ContentError {
    #[error("String exceeds maximum length for LCD display: {actual_length} > {max_length}")]
    TooLong {
        content: heapless::String<64>,
        actual_length: usize,
        max_length: usize,
    },
    #[error("String contains too many lines for LCD display: {actual_lines} > {max_lines}")]
    TooManyLines {
        content: heapless::String<64>,
        actual_lines: usize,
        max_lines: usize,
    },
    #[error("String contains invalid characters for LCD display: '{invalid_char}'")]
    ContainsInvalidCharacters {
        content: heapless::String<64>,
        invalid_char: char,
    },
}

impl InlandKs0061Line {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for InlandKs0061Line {
    type Error = InlandKs0061ContentError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let len = value.chars().count();
        if len > INLAND_KS0061_MAX_CHARS_PER_LINE {
            return Err(InlandKs0061ContentError::TooLong {
                content: value.chars().take(64).collect(),
                actual_length: len,
                max_length: INLAND_KS0061_MAX_CHARS_PER_LINE,
            });
        }

        // allow only alphanumeric and common punctuation characters
        for c in value.chars() {
            if !(c.is_ascii_graphic() || c == ' ') {
                return Err(InlandKs0061ContentError::ContainsInvalidCharacters {
                    content: value.chars().take(64).collect(),
                    invalid_char: c,
                });
            }
        }

        let mut heapless_str: heapless::String<INLAND_KS0061_MAX_CHARS_PER_LINE> =
            heapless::String::new();
        heapless_str
            .push_str(value)
            .map_err(|_| InlandKs0061ContentError::TooLong {
                content: value.chars().take(64).collect(),
                actual_length: len,
                max_length: INLAND_KS0061_MAX_CHARS_PER_LINE,
            })?;

        Ok(InlandKs0061Line(heapless_str))
    }
}

#[derive(Debug, defmt::Format, Clone, PartialEq, Eq, Default)]
pub struct InlandKs0061Content {
    pub line1: Option<InlandKs0061Line>,
    pub line2: Option<InlandKs0061Line>,
}

impl TryFrom<&str> for InlandKs0061Content {
    type Error = InlandKs0061ContentError;

    /// You can pass either:
    /// 1. An empty string (will clear the display)
    /// 2. A string with a single newline separating two lines, each 16 characters max
    /// 3. A string without newlines, 16 characters max (will be displayed on line 1)
    /// 4. A string without newlines, 32 characters max (will be split across line 1 and line 2 at position 16)
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Ok(InlandKs0061Content::default());
        }

        for c in value.chars() {
            if c == '\n' {
                continue;
            }
            if !(c.is_ascii_graphic() || c == ' ') {
                return Err(InlandKs0061ContentError::ContainsInvalidCharacters {
                    content: value.chars().take(64).collect(),
                    invalid_char: c,
                });
            }
        }

        if value.contains('\n') {
            let lines_count = value.lines().count();
            if lines_count > INLAND_KS0061_ROWS {
                return Err(InlandKs0061ContentError::TooManyLines {
                    content: value.chars().take(64).collect(),
                    actual_lines: lines_count,
                    max_lines: INLAND_KS0061_ROWS,
                });
            }
            let mut lines = value.lines();
            let line1_str = lines.next().unwrap_or_default();
            let line2_str = lines.next().unwrap_or_default();

            let line1 = InlandKs0061Line::try_from(line1_str)?;
            let line2 = InlandKs0061Line::try_from(line2_str)?;

            return Ok(InlandKs0061Content {
                line1: Some(line1),
                line2: Some(line2),
            });
        }

        let len = value.chars().count();
        if len <= INLAND_KS0061_MAX_CHARS_PER_LINE {
            let line1 = InlandKs0061Line::try_from(value)?;
            return Ok(InlandKs0061Content {
                line1: Some(line1),
                line2: None,
            });
        }

        if len <= INLAND_KS0061_MAX_CHARS_TOTAL {
            let line1 = InlandKs0061Line::try_from(&value[0..INLAND_KS0061_MAX_CHARS_PER_LINE])?;
            let line2 = InlandKs0061Line::try_from(&value[INLAND_KS0061_MAX_CHARS_PER_LINE..])?;
            return Ok(InlandKs0061Content {
                line1: Some(line1),
                line2: Some(line2),
            });
        }

        Err(InlandKs0061ContentError::TooLong {
            content: value.chars().take(64).collect(),
            actual_length: len,
            max_length: INLAND_KS0061_MAX_CHARS_TOTAL,
        })
    }
}

pub struct InlandKs0061I2cDisplay<I: embedded_hal::i2c::I2c> {
    display: CharacterDisplayPCF8574T<I, Delay>,
}

impl<I: embedded_hal::i2c::I2c> InlandKs0061I2cDisplay<I> {
    pub fn new(i2c: I, address: u8) -> Result<Self, InlandKs0061I2cDisplayError> {
        let delay = Delay;
        let mut lcd_display = CharacterDisplayPCF8574T::new_with_address(
            i2c,
            address,
            LcdDisplayType::Lcd16x2,
            delay,
        );
        lcd_display
            .init()
            .map_err(|_| InlandKs0061I2cDisplayError::Initialization)?;
        lcd_display
            .backlight(true)
            .map_err(|_| InlandKs0061I2cDisplayError::Backlight)?;
        lcd_display
            .clear()
            .map_err(|_| InlandKs0061I2cDisplayError::Clear)?;
        Ok(Self {
            display: lcd_display,
        })
    }

    pub fn new_with_default_address(i2c: I) -> Result<Self, InlandKs0061I2cDisplayError> {
        Self::new(i2c, inland_ks0061_default_i2c_address())
    }

    pub fn clear(&mut self) -> Result<(), InlandKs0061I2cDisplayError> {
        self.display
            .clear()
            .map_err(|_| InlandKs0061I2cDisplayError::Clear)
            .map(|_| ())
    }

    pub fn display_str(&mut self, s: &str) -> Result<(), InlandKs0061I2cDisplayError> {
        let content = InlandKs0061Content::try_from(s)?;
        self.display_content(content)
    }

    pub fn display_content(
        &mut self,
        content: InlandKs0061Content,
    ) -> Result<(), InlandKs0061I2cDisplayError> {
        self.display
            .clear()
            .map_err(|_| InlandKs0061I2cDisplayError::Clear)
            .map(|_| ())?;
        if let Some(line1) = content.line1 {
            self.display
                .home()
                .map_err(|_| InlandKs0061I2cDisplayError::SetCursor)?;
            self.display
                .print(line1.as_str())
                .map_err(|_| InlandKs0061I2cDisplayError::Print)?;
        }
        if let Some(line2) = content.line2 {
            self.display
                .set_cursor(0, 1)
                .map_err(|_| InlandKs0061I2cDisplayError::SetCursor)?;
            self.display
                .print(line2.as_str())
                .map_err(|_| InlandKs0061I2cDisplayError::Print)?;
        }
        Ok(())
    }
}
