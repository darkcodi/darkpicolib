use embassy_time::Delay;
use i2c_character_display::{CharacterDisplayPCF8574T, LcdDisplayType};

#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum LcdError {
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
    InvalidContent(#[from] LcdStringError),
}

#[derive(Debug, defmt::Format, Clone, PartialEq, Eq)]
pub struct LcdString(heapless::String<16>);

#[derive(Debug, defmt::Format, thiserror::Error)]
pub enum LcdStringError {
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

impl LcdString {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for LcdString {
    type Error = LcdStringError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let len = value.len();
        if len > 16 {
            return Err(LcdStringError::TooLong {
                content: value.chars().take(64).collect(),
                actual_length: len,
                max_length: 16,
            });
        }

        // allow only alphanumeric and common punctuation characters
        for c in value.chars() {
            if !(c.is_ascii_graphic() || c == ' ') {
                return Err(LcdStringError::ContainsInvalidCharacters {
                    content: value.chars().take(64).collect(),
                    invalid_char: c,
                });
            }
        }

        let mut heapless_str: heapless::String<16> = heapless::String::new();
        heapless_str
            .push_str(value)
            .map_err(|_| LcdStringError::TooLong {
                content: value.chars().take(64).collect(),
                actual_length: len,
                max_length: 16,
            })?;

        Ok(LcdString(heapless_str))
    }
}

#[derive(Debug, defmt::Format, Clone, PartialEq, Eq, Default)]
pub struct LcdContent {
    pub line1: Option<LcdString>,
    pub line2: Option<LcdString>,
}

impl TryFrom<&str> for LcdContent {
    type Error = LcdStringError;

    /// You can pass either:
    /// 1. An empty string (will clear the display)
    /// 2. A string with a single newline separating two lines, each 16 characters max
    /// 3. A string without newlines, 16 characters max (will be displayed on line 1)
    /// 4. A string without newlines, 32 characters max (will be split across line 1 and line 2 at position 16)
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Ok(LcdContent::default());
        }

        if value.contains('\n') {
            let lines_count = value.lines().count();
            if lines_count > 2 {
                return Err(LcdStringError::TooManyLines {
                    content: value.chars().take(64).collect(),
                    actual_lines: lines_count,
                    max_lines: 2,
                });
            }
            let mut lines = value.lines();
            let line1_str = lines.next().unwrap_or_default();
            let line2_str = lines.next().unwrap_or_default();

            let line1 = LcdString::try_from(line1_str)?;
            let line2 = LcdString::try_from(line2_str)?;

            return Ok(LcdContent {
                line1: Some(line1),
                line2: Some(line2),
            });
        }

        let len = value.len();
        if len <= 16 {
            let line1 = LcdString::try_from(value)?;
            return Ok(LcdContent {
                line1: Some(line1),
                line2: None,
            });
        }

        if len <= 32 {
            let line1 = LcdString::try_from(&value[0..16])?;
            let line2 = LcdString::try_from(&value[16..])?;
            return Ok(LcdContent {
                line1: Some(line1),
                line2: Some(line2),
            });
        }

        Err(LcdStringError::TooLong {
            content: value.chars().take(64).collect(),
            actual_length: len,
            max_length: 32,
        })
    }
}

pub struct Lcd<I: embedded_hal::i2c::I2c> {
    lcd: CharacterDisplayPCF8574T<I, Delay>,
}

impl<I: embedded_hal::i2c::I2c> Lcd<I> {
    pub fn new(i2c: I, address: u8) -> Result<Self, LcdError> {
        let delay = Delay;
        let mut lcd_display = CharacterDisplayPCF8574T::new_with_address(
            i2c,
            address,
            LcdDisplayType::Lcd16x2,
            delay,
        );
        lcd_display.init().map_err(|_| LcdError::Initialization)?;
        lcd_display
            .backlight(true)
            .map_err(|_| LcdError::Backlight)?;
        lcd_display.clear().map_err(|_| LcdError::Clear)?;
        Ok(Self { lcd: lcd_display })
    }

    pub fn clear(&mut self) -> Result<(), LcdError> {
        self.lcd.clear().map_err(|_| LcdError::Clear).map(|_| ())
    }

    pub fn display_str(&mut self, s: &str) -> Result<(), LcdError> {
        let content = LcdContent::try_from(s)?;
        self.display_content(content)
    }

    pub fn display_content(&mut self, content: LcdContent) -> Result<(), LcdError> {
        self.lcd.clear().map_err(|_| LcdError::Clear).map(|_| ())?;
        if let Some(line1) = content.line1 {
            self.lcd.home().map_err(|_| LcdError::SetCursor)?;
            self.lcd
                .print(line1.as_str())
                .map_err(|_| LcdError::Print)?;
        }
        if let Some(line2) = content.line2 {
            self.lcd.set_cursor(0, 1).map_err(|_| LcdError::SetCursor)?;
            self.lcd
                .print(line2.as_str())
                .map_err(|_| LcdError::Print)?;
        }
        Ok(())
    }
}
