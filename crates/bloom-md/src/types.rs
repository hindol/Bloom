//! Markdown-domain types used throughout the parser and extensions.

use chrono::{NaiveDate, NaiveDateTime};
use std::fmt;

/// 8-char hex UUID (4 bytes). E.g., "8f3a1b2c"
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PageId(pub [u8; 4]);

impl PageId {
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 8 {
            return None;
        }
        let mut bytes = [0u8; 4];
        for i in 0..4 {
            bytes[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(PageId(bytes))
    }

    pub fn to_hex(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TagName(pub String);

#[derive(Debug, Clone)]
pub enum Timestamp {
    Due(NaiveDate),
    Start(NaiveDate),
    At(NaiveDateTime),
}
