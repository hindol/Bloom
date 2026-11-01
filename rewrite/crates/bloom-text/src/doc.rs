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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditDelta {
    pub start_line: usize,
    pub old_line_count: usize,
    pub new_line_count: usize,
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
        self.insert_with_delta(at, text).map(|(cursor, _)| cursor)
    }

    pub fn insert_with_delta(
        &mut self,
        at: CharIdx,
        text: &str,
    ) -> Result<(CharIdx, Option<EditDelta>), DocError> {
        let position = self.char_offset(at)?;
        if text.is_empty() {
            return Ok((at, None));
        }
        let start_line = self.rope.char_to_line(position);
        self.rope.insert(position, text);
        let cursor = position + text.chars().count();
        let end_line = self.rope.char_to_line(cursor);
        Ok((
            CharIdx::new(cursor),
            Some(EditDelta {
                start_line,
                old_line_count: 1,
                new_line_count: end_line - start_line + 1,
            }),
        ))
    }

    pub fn delete_previous_char(&mut self, at: CharIdx) -> Result<CharIdx, DocError> {
        self.delete_previous_char_with_delta(at)
            .map(|(cursor, _)| cursor)
    }

    pub fn delete_previous_char_with_delta(
        &mut self,
        at: CharIdx,
    ) -> Result<(CharIdx, Option<EditDelta>), DocError> {
        let position = self.char_offset(at)?;
        if position == 0 {
            return Ok((at, None));
        }

        let start_line = self.rope.char_to_line(position - 1);
        let old_end_line = self.rope.char_to_line(position);
        self.rope.remove(position - 1..position);
        let cursor = CharIdx::new(position - 1);
        let new_end_line = self.rope.char_to_line(position - 1);
        Ok((
            cursor,
            Some(EditDelta {
                start_line,
                old_line_count: old_end_line - start_line + 1,
                new_line_count: new_end_line - start_line + 1,
            }),
        ))
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

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_for_char(&self, position: CharIdx) -> Result<usize, DocError> {
        let position = self.char_offset(position)?;
        Ok(self.rope.char_to_line(position))
    }

    pub fn line_start(&self, line: usize) -> Option<CharIdx> {
        (line < self.line_count()).then(|| CharIdx::new(self.rope.line_to_char(line)))
    }

    pub fn line_char_len(&self, line: usize) -> Option<usize> {
        (line < self.line_count()).then(|| self.rope.line(line).len_chars())
    }

    pub fn line_text(&self, line: usize) -> Option<String> {
        let mut text = (line < self.line_count()).then(|| self.rope.line(line).to_string())?;
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{Doc, DocError, EditDelta};

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

    #[test]
    fn line_snapshots_preserve_empty_lines_and_strip_newline_delimiters() {
        let snapshot = Doc::new("one\r\n\nlast\n").snapshot();

        assert_eq!(snapshot.line_count(), 4);
        assert_eq!(
            (0..snapshot.line_count())
                .map(|line| snapshot.line_text(line).unwrap())
                .collect::<Vec<_>>(),
            ["one", "", "last", ""]
        );
        assert_eq!(snapshot.line_char_len(0), Some(5));
        assert_eq!(snapshot.line_start(2), Some(snapshot.char_idx(6).unwrap()));
        assert_eq!(snapshot.line_for_char(snapshot.char_idx(6).unwrap()), Ok(2));
    }

    #[test]
    fn edit_deltas_cover_inserted_and_merged_physical_lines() {
        let mut doc = Doc::new("ab\ncd");
        let (cursor, insert_delta) = doc
            .insert_with_delta(doc.char_idx(1).unwrap(), "\nnew\n")
            .unwrap();
        assert_eq!(
            insert_delta,
            Some(EditDelta {
                start_line: 0,
                old_line_count: 1,
                new_line_count: 3,
            })
        );

        let (cursor, delete_delta) = doc.delete_previous_char_with_delta(cursor).unwrap();
        assert_eq!(
            delete_delta,
            Some(EditDelta {
                start_line: 1,
                old_line_count: 2,
                new_line_count: 1,
            })
        );
        assert_eq!(doc.text(), "a\nnewb\ncd");
        assert_eq!(doc.char_offset(cursor), Ok(cursor.get()));
    }

    #[test]
    fn empty_insert_reports_no_layout_invalidation() {
        let mut doc = Doc::new("note");
        let cursor = doc.char_idx(2).unwrap();

        assert_eq!(doc.insert_with_delta(cursor, ""), Ok((cursor, None)));
        assert_eq!(doc.text(), "note");
    }
}
