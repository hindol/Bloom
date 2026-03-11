use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

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

pub type Version = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize)]
pub struct PaneId(pub u64);

pub type UndoNodeId = u64;

#[derive(Debug, Clone)]
pub struct PageMeta {
    pub id: PageId,
    pub title: String,
    pub created: NaiveDate,
    pub tags: Vec<TagName>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LinkTarget {
    pub page: PageId,
    pub section: Option<BlockId>,
    pub display_hint: String,
}

#[derive(Debug, Clone)]
pub enum Timestamp {
    Due(NaiveDate),
    Start(NaiveDate),
    At(NaiveDateTime),
}

#[derive(Debug, Clone)]
pub struct Task {
    pub text: String,
    pub done: bool,
    pub timestamps: Vec<Timestamp>,
    pub source_page: PageId,
    pub line: usize,
}

/// Key event for input processing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Default::default()
    }
    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Default::default()
        }
    }
    pub fn alt() -> Self {
        Self {
            alt: true,
            ..Default::default()
        }
    }
    pub fn shift() -> Self {
        Self {
            shift: true,
            ..Default::default()
        }
    }
}

impl KeyEvent {
    pub fn char(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: Modifiers::none(),
        }
    }
    pub fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: Modifiers::ctrl(),
        }
    }
    pub fn esc() -> Self {
        Self {
            code: KeyCode::Esc,
            modifiers: Modifiers::none(),
        }
    }
    pub fn enter() -> Self {
        Self {
            code: KeyCode::Enter,
            modifiers: Modifiers::none(),
        }
    }
    pub fn tab() -> Self {
        Self {
            code: KeyCode::Tab,
            modifiers: Modifiers::none(),
        }
    }
    pub fn backspace() -> Self {
        Self {
            code: KeyCode::Backspace,
            modifiers: Modifiers::none(),
        }
    }
}

impl fmt::Display for KeyEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut prefix = String::new();
        if self.modifiers.ctrl {
            prefix.push_str("C-");
        }
        if self.modifiers.alt {
            prefix.push_str("A-");
        }
        if self.modifiers.shift {
            prefix.push_str("S-");
        }
        if self.modifiers.meta {
            prefix.push_str("M-");
        }
        let key = match &self.code {
            KeyCode::Char(' ') => "SPC".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::F(n) => format!("F{n}"),
        };
        write!(f, "{prefix}{key}")
    }
}
