use std::{error::Error, fmt, ops::Range};

use super::Mode;

const MAX_SOURCE_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptError {
    Empty,
    UnsupportedControl,
    UnsupportedCharacter,
    TooLarge,
    InvalidWordContent,
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "prompt contains no characters to type",
            Self::UnsupportedControl => "prompt contains an unsupported control character",
            Self::UnsupportedCharacter => "this version supports ASCII exercise text only",
            Self::TooLarge => "prompt exceeds the 1 MiB session limit",
            Self::InvalidWordContent => "word prompts must be a single line without tabs",
        })
    }
}

impl Error for PromptError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedUnit {
    pub character: char,
    pub display_offset: usize,
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct Prompt {
    mode: Mode,
    display: String,
    expected: Vec<ExpectedUnit>,
    // ASCII source offsets are deliberately distinct from scored positions.
    scored_at: Vec<Option<usize>>,
    lines: Vec<Range<usize>>,
}

impl Prompt {
    pub fn new(source: &str, mode: Mode) -> Result<Self, PromptError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(PromptError::TooLarge);
        }
        let source = source.replace("\r\n", "\n");
        let mut display = String::new();
        let mut column = 0;
        for c in source.chars() {
            if !c.is_ascii() {
                return Err(PromptError::UnsupportedCharacter);
            }
            match c {
                '\n' => {
                    display.push(c);
                    column = 0;
                }
                '\t' => {
                    let spaces = 4 - column % 4;
                    display.extend(std::iter::repeat_n(' ', spaces));
                    column += spaces;
                }
                ' '..='~' => {
                    display.push(c);
                    column += 1;
                }
                _ => return Err(PromptError::UnsupportedControl),
            }
            if display.len() > MAX_SOURCE_BYTES {
                return Err(PromptError::TooLarge);
            }
        }
        if mode == Mode::Words && source.contains(['\n', '\t']) {
            return Err(PromptError::InvalidWordContent);
        }
        let mut expected = Vec::new();
        let mut scored_at = vec![None; display.len()];
        let mut lines = Vec::new();
        let mut offset = 0;
        for (row, line) in display.split('\n').enumerate() {
            lines.push(offset..offset + line.len());
            let first = if mode == Mode::Code {
                line.len() - line.trim_start_matches(' ').len()
            } else {
                0
            };
            let end = if mode == Mode::Code {
                line.trim_end_matches(' ').len()
            } else {
                line.len()
            };
            for (column, byte) in line.bytes().enumerate().take(end).skip(first) {
                let index = expected.len();
                expected.push(ExpectedUnit {
                    character: char::from(byte),
                    display_offset: offset + column,
                    row,
                    column,
                });
                scored_at[offset + column] = Some(index);
            }
            offset += line.len() + 1;
        }
        if expected.is_empty() {
            return Err(PromptError::Empty);
        }
        Ok(Self {
            mode,
            display,
            expected,
            scored_at,
            lines,
        })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn display(&self) -> &str {
        &self.display
    }
    pub fn expected(&self) -> &[ExpectedUnit] {
        &self.expected
    }
    pub fn lines(&self) -> &[Range<usize>] {
        &self.lines
    }
    pub fn scored_at(&self, offset: usize) -> Option<usize> {
        self.scored_at.get(offset).copied().flatten()
    }

    pub fn append_words(&mut self, words: &str) -> Result<(), PromptError> {
        if self.mode != Mode::Words {
            return Err(PromptError::InvalidWordContent);
        }
        let extra = Self::new(words, Mode::Words)?;
        if self.display.len() + 1 + extra.display.len() > MAX_SOURCE_BYTES {
            return Err(PromptError::TooLarge);
        }
        for c in std::iter::once(' ').chain(extra.display.chars()) {
            let offset = self.display.len();
            let index = self.expected.len();
            self.display.push(c);
            self.scored_at.push(Some(index));
            self.expected.push(ExpectedUnit {
                character: c,
                display_offset: offset,
                row: 0,
                column: offset,
            });
        }
        if let Some(line) = self.lines.first_mut() {
            line.end = self.display.len();
        }
        Ok(())
    }
}
