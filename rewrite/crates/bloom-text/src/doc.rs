use ropey::Rope;

use crate::coordinates::CharIdx;

#[derive(Clone, Debug)]
/// Plain rope text whose positions count Unicode scalar values.
pub struct Doc {
    rope: Rope,
}

#[derive(Clone, Debug)]
/// Read-only rope snapshot for rendering without copying the entire document.
pub struct DocSnapshot {
    rope: Rope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocError {
    PositionOutOfBounds { position: usize, length: usize },
}

impl Doc {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }

    pub fn start(&self) -> CharIdx {
        CharIdx::new(0)
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn char_idx(&self, position: usize) -> Result<CharIdx, DocError> {
        if position > self.len_chars() {
            return Err(DocError::PositionOutOfBounds {
                position,
                length: self.len_chars(),
            });
        }

        Ok(CharIdx::new(position))
    }

    pub fn char_offset(&self, position: CharIdx) -> Result<usize, DocError> {
        self.char_idx(position.get()).map(CharIdx::get)
    }

    pub fn insert(&mut self, at: CharIdx, text: &str) -> Result<CharIdx, DocError> {
        let position = self.char_offset(at)?;
        self.rope.insert(position, text);
        Ok(CharIdx::new(position + text.chars().count()))
    }

    pub fn delete_previous_char(&mut self, at: CharIdx) -> Result<CharIdx, DocError> {
        let position = self.char_offset(at)?;
        if position == 0 {
            return Ok(at);
        }

        self.rope.remove(position - 1..position);
        Ok(CharIdx::new(position - 1))
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn snapshot(&self) -> DocSnapshot {
        DocSnapshot {
            rope: self.rope.clone(),
        }
    }
}

impl DocSnapshot {
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn char_idx(&self, position: usize) -> Result<CharIdx, DocError> {
        if position > self.len_chars() {
            return Err(DocError::PositionOutOfBounds {
                position,
                length: self.len_chars(),
            });
        }

        Ok(CharIdx::new(position))
    }

    pub fn char_offset(&self, position: CharIdx) -> Result<usize, DocError> {
        self.char_idx(position.get()).map(CharIdx::get)
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{Doc, DocError};

    #[test]
    fn edits_rope_by_unicode_scalar_coordinate() {
        let mut doc = Doc::new("aé🙂");
        let cursor = doc.char_idx(doc.len_chars()).unwrap();

        let cursor = doc.delete_previous_char(cursor).unwrap();
        assert_eq!(doc.text(), "aé");
        assert_eq!(doc.char_offset(cursor), Ok(2));

        let cursor = doc.insert(cursor, "中").unwrap();
        assert_eq!(doc.text(), "aé中");
        assert_eq!(doc.char_offset(cursor), Ok(3));
    }

    #[test]
    fn rejects_positions_past_document_end() {
        let doc = Doc::new("abc");

        assert_eq!(
            doc.char_idx(4),
            Err(DocError::PositionOutOfBounds {
                position: 4,
                length: 3
            })
        );
    }

    #[test]
    fn snapshot_owns_character_offset_conversion() {
        let snapshot = Doc::new("é🙂").snapshot();

        assert_eq!(snapshot.len_chars(), 2);
        assert_eq!(snapshot.char_offset(snapshot.char_idx(2).unwrap()), Ok(2));
        assert_eq!(
            snapshot.char_idx(3),
            Err(DocError::PositionOutOfBounds {
                position: 3,
                length: 2,
            })
        );
    }
}
