//! Motion implementations and operator range resolution.
//!
//! Each [`MotionType`] variant maps to a function that computes a new cursor
//! position given the current buffer and cursor. When combined with an
//! [`Operator`], the motion range is resolved to
//! a character span for editing (delete, yank, change, indent, etc.).

use std::ops::Range;

use bloom_buffer::Buffer;

use super::operator::Operator;

/// All supported cursor motions.
///
/// Each variant maps to a movement function that computes a new cursor
/// position in the buffer. Used both as standalone motions and as the
/// target half of operator+motion commands (e.g. `dw`, `c$`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotionType {
    Left,
    Right,
    Down,
    Up,
    WordForward,
    WordBackward,
    WordEnd,
    WORDForward,
    WORDBackward,
    WORDEnd,
    LineStart,
    LineEnd,
    FirstNonWhitespace,
    DocumentStart,
    DocumentEnd,
    FindForward(char),
    FindBackward(char),
    ToForward(char),
    ToBackward(char),
    MatchingBracket,
    ParagraphForward,
    ParagraphBackward,
    RepeatFind,
    RepeatFindReverse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindCommand {
    pub char_target: char,
    pub forward: bool,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorRangeSpec {
    pub target: usize,
    pub inclusive: bool,
}

// ── helpers ──────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum CharClass {
    Word,
    Punctuation,
    Whitespace,
    Newline,
}

fn classify(c: char) -> CharClass {
    if c == '\n' || c == '\r' {
        CharClass::Newline
    } else if c.is_whitespace() {
        CharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn char_at(buffer: &Buffer, pos: usize) -> Option<char> {
    if pos < buffer.len_chars() {
        Some(buffer.text().char(pos))
    } else {
        None
    }
}

/// Char index of the first char on the line containing `cursor`.
fn line_start(buffer: &Buffer, cursor: usize) -> usize {
    let line_idx = buffer.text().char_to_line(cursor);
    buffer.text().line_to_char(line_idx)
}

/// Char index of the last non-newline char on the line, or line start if empty.
fn line_last(buffer: &Buffer, cursor: usize) -> usize {
    let rope = buffer.text();
    let line_idx = rope.char_to_line(cursor);
    let start = rope.line_to_char(line_idx);
    let line = rope.line(line_idx);
    let mut end = line.len_chars();
    while end > 0 && matches!(line.char(end - 1), '\n' | '\r') {
        end -= 1;
    }
    if end == 0 {
        start
    } else {
        start + end - 1
    }
}

fn line_end_exclusive(buffer: &Buffer, cursor: usize) -> usize {
    let rope = buffer.text();
    let line_idx = rope.char_to_line(cursor);
    let start = rope.line_to_char(line_idx);
    let line = rope.line(line_idx);
    let mut end = line.len_chars();
    while end > 0 && matches!(line.char(end - 1), '\n' | '\r') {
        end -= 1;
    }
    start + end
}

// ── individual motions ───────────────────────────────────────────────

fn motion_h(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let start = line_start(buffer, cursor);
    cursor.saturating_sub(count).max(start)
}

fn motion_l(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let end = line_last(buffer, cursor);
    (cursor + count).min(end)
}

fn motion_l_boundary(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let end = line_end_exclusive(buffer, cursor);
    (cursor + count).min(end)
}

fn motion_j(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let col = cursor - rope.line_to_char(cur_line);
    let target_line = (cur_line + count).min(rope.len_lines().saturating_sub(1));
    let target_start = rope.line_to_char(target_line);
    let target_line_slice = rope.line(target_line);
    let mut line_len = target_line_slice.len_chars();
    while line_len > 0 && matches!(target_line_slice.char(line_len - 1), '\n' | '\r') {
        line_len -= 1;
    }
    let max_col = line_len.saturating_sub(1);
    target_start + col.min(max_col)
}

fn motion_k(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let col = cursor - rope.line_to_char(cur_line);
    let target_line = cur_line.saturating_sub(count);
    let target_start = rope.line_to_char(target_line);
    let target_line_slice = rope.line(target_line);
    let mut line_len = target_line_slice.len_chars();
    while line_len > 0 && matches!(target_line_slice.char(line_len - 1), '\n' | '\r') {
        line_len -= 1;
    }
    let max_col = line_len.saturating_sub(1);
    target_start + col.min(max_col)
}

fn motion_w_boundary(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        if pos >= len {
            break;
        }
        let cls = classify(buffer.text().char(pos));
        // Skip current class
        while pos < len && classify(buffer.text().char(pos)) == cls {
            pos += 1;
        }
        // Skip whitespace/newlines
        while pos < len
            && matches!(
                classify(buffer.text().char(pos)),
                CharClass::Whitespace | CharClass::Newline
            )
        {
            pos += 1;
        }
    }
    pos
}

fn motion_w(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    motion_w_boundary(buffer, cursor, count).min(len.saturating_sub(1))
}

fn motion_b(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..count {
        if pos == 0 {
            break;
        }
        pos -= 1;
        // Skip whitespace/newlines backward
        while pos > 0
            && matches!(
                classify(buffer.text().char(pos)),
                CharClass::Whitespace | CharClass::Newline
            )
        {
            pos -= 1;
        }
        // Skip current class backward
        let cls = classify(buffer.text().char(pos));
        while pos > 0 && classify(buffer.text().char(pos - 1)) == cls {
            pos -= 1;
        }
    }
    pos
}

fn motion_e(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        if pos + 1 >= len {
            break;
        }
        pos += 1;
        // Skip whitespace/newlines
        while pos < len
            && matches!(
                classify(buffer.text().char(pos)),
                CharClass::Whitespace | CharClass::Newline
            )
        {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        // Skip to end of current class
        let cls = classify(buffer.text().char(pos));
        while pos + 1 < len && classify(buffer.text().char(pos + 1)) == cls {
            pos += 1;
        }
    }
    pos.min(len.saturating_sub(1))
}

fn skip_blank_chars(buffer: &Buffer, mut pos: usize) -> usize {
    let len = buffer.len_chars();
    while pos < len
        && matches!(
            classify(buffer.text().char(pos)),
            CharClass::Whitespace | CharClass::Newline
        )
    {
        pos += 1;
    }
    pos
}

fn motion_change_word_end(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    if len == 0 {
        return 0;
    }

    let mut pos = cursor.min(len.saturating_sub(1));
    for step in 0..count {
        pos = skip_blank_chars(buffer, pos);
        if pos >= len {
            return len.saturating_sub(1);
        }

        let cls = classify(buffer.text().char(pos));
        while pos + 1 < len && classify(buffer.text().char(pos + 1)) == cls {
            pos += 1;
        }

        if step + 1 < count {
            pos = skip_blank_chars(buffer, pos.saturating_add(1));
        }
    }

    pos
}

fn is_word_boundary_for_big(c: char) -> bool {
    c.is_whitespace()
}

fn motion_big_w_boundary(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        // Skip non-whitespace
        while pos < len && !is_word_boundary_for_big(buffer.text().char(pos)) {
            pos += 1;
        }
        // Skip whitespace
        while pos < len && is_word_boundary_for_big(buffer.text().char(pos)) {
            pos += 1;
        }
    }
    pos
}

fn motion_big_w(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    motion_big_w_boundary(buffer, cursor, count).min(len.saturating_sub(1))
}

fn motion_big_b(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..count {
        if pos == 0 {
            break;
        }
        pos -= 1;
        while pos > 0 && is_word_boundary_for_big(buffer.text().char(pos)) {
            pos -= 1;
        }
        while pos > 0 && !is_word_boundary_for_big(buffer.text().char(pos - 1)) {
            pos -= 1;
        }
    }
    pos
}

fn motion_big_e(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        if pos + 1 >= len {
            break;
        }
        pos += 1;
        while pos < len && is_word_boundary_for_big(buffer.text().char(pos)) {
            pos += 1;
        }
        while pos + 1 < len && !is_word_boundary_for_big(buffer.text().char(pos + 1)) {
            pos += 1;
        }
    }
    pos.min(len.saturating_sub(1))
}

fn skip_big_word_whitespace(buffer: &Buffer, mut pos: usize) -> usize {
    let len = buffer.len_chars();
    while pos < len && is_word_boundary_for_big(buffer.text().char(pos)) {
        pos += 1;
    }
    pos
}

fn motion_change_big_word_end(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    if len == 0 {
        return 0;
    }

    let mut pos = cursor.min(len.saturating_sub(1));
    for step in 0..count {
        pos = skip_big_word_whitespace(buffer, pos);
        if pos >= len {
            return len.saturating_sub(1);
        }

        while pos + 1 < len && !is_word_boundary_for_big(buffer.text().char(pos + 1)) {
            pos += 1;
        }

        if step + 1 < count {
            pos = skip_big_word_whitespace(buffer, pos.saturating_add(1));
        }
    }

    pos
}

fn motion_0(buffer: &Buffer, cursor: usize) -> usize {
    line_start(buffer, cursor)
}

fn motion_dollar(buffer: &Buffer, cursor: usize) -> usize {
    line_last(buffer, cursor)
}

fn motion_caret(buffer: &Buffer, cursor: usize) -> usize {
    let rope = buffer.text();
    let line_idx = rope.char_to_line(cursor);
    let start = rope.line_to_char(line_idx);
    let line = rope.line(line_idx);
    for i in 0..line.len_chars() {
        let c = line.char(i);
        if c == '\n' || c == '\r' {
            return start;
        }
        if !c.is_whitespace() {
            return start + i;
        }
    }
    start
}

fn motion_gg(buffer: &Buffer, count: Option<usize>) -> usize {
    match count {
        Some(n) => {
            let line = (n.saturating_sub(1)).min(buffer.len_lines().saturating_sub(1));
            buffer.text().line_to_char(line)
        }
        None => 0,
    }
}

fn motion_big_g(buffer: &Buffer, count: Option<usize>) -> usize {
    match count {
        Some(n) => {
            let line = (n.saturating_sub(1)).min(buffer.len_lines().saturating_sub(1));
            buffer.text().line_to_char(line)
        }
        None => {
            let last_line = buffer.len_lines().saturating_sub(1);
            buffer.text().line_to_char(last_line)
        }
    }
}

fn motion_find(
    buffer: &Buffer,
    cursor: usize,
    target: char,
    forward: bool,
    count: usize,
) -> Option<usize> {
    let rope = buffer.text();
    let line_idx = rope.char_to_line(cursor);
    let line_start = rope.line_to_char(line_idx);
    let line = rope.line(line_idx);
    let line_len = line.len_chars();
    let col = cursor - line_start;
    let mut found = 0;

    if forward {
        for i in (col + 1)..line_len {
            if line.char(i) == target {
                found += 1;
                if found == count {
                    return Some(line_start + i);
                }
            }
        }
    } else {
        for i in (0..col).rev() {
            if line.char(i) == target {
                found += 1;
                if found == count {
                    return Some(line_start + i);
                }
            }
        }
    }
    None
}

fn motion_to(
    buffer: &Buffer,
    cursor: usize,
    target: char,
    forward: bool,
    count: usize,
) -> Option<usize> {
    motion_find(buffer, cursor, target, forward, count).map(|pos| {
        if forward {
            pos.saturating_sub(1).max(cursor)
        } else {
            (pos + 1).min(cursor)
        }
    })
}

fn motion_matching_bracket(buffer: &Buffer, cursor: usize) -> Option<usize> {
    let c = char_at(buffer, cursor)?;
    let (target, forward) = match c {
        '(' => (')', true),
        ')' => ('(', false),
        '[' => (']', true),
        ']' => ('[', false),
        '{' => ('}', true),
        '}' => ('{', false),
        _ => return None,
    };
    let len = buffer.len_chars();
    let mut depth: i32 = 1;
    if forward {
        let mut pos = cursor + 1;
        while pos < len {
            let ch = buffer.text().char(pos);
            if ch == c {
                depth += 1;
            } else if ch == target {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            pos += 1;
        }
    } else {
        let mut pos = cursor;
        while pos > 0 {
            pos -= 1;
            let ch = buffer.text().char(pos);
            if ch == c {
                depth += 1;
            } else if ch == target {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
        }
    }
    None
}

fn motion_paragraph_forward_boundary(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let rope = buffer.text();
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        // Skip non-blank lines
        while pos < len {
            let line_idx = rope.char_to_line(pos);
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0 || (line.len_chars() == 1 && line.char(0) == '\n');
            if blank {
                break;
            }
            let next_line = line_idx + 1;
            if next_line >= rope.len_lines() {
                return len;
            }
            pos = rope.line_to_char(next_line);
        }
        // Skip blank lines
        while pos < len {
            let line_idx = rope.char_to_line(pos);
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0 || (line.len_chars() == 1 && line.char(0) == '\n');
            if !blank {
                break;
            }
            let next_line = line_idx + 1;
            if next_line >= rope.len_lines() {
                return len;
            }
            pos = rope.line_to_char(next_line);
        }
    }
    pos
}

fn motion_paragraph_forward(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let len = buffer.len_chars();
    motion_paragraph_forward_boundary(buffer, cursor, count).min(len.saturating_sub(1))
}

fn motion_paragraph_backward(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let rope = buffer.text();
    let mut pos = cursor;
    for _ in 0..count {
        let mut line_idx = rope.char_to_line(pos);
        if line_idx == 0 {
            return 0;
        }
        line_idx -= 1;
        // Skip non-blank lines backward
        loop {
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0 || (line.len_chars() == 1 && line.char(0) == '\n');
            if blank || line_idx == 0 {
                break;
            }
            line_idx -= 1;
        }
        // Skip blank lines backward
        loop {
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0 || (line.len_chars() == 1 && line.char(0) == '\n');
            if !blank || line_idx == 0 {
                break;
            }
            line_idx -= 1;
        }
        pos = rope.line_to_char(line_idx);
    }
    pos
}

// ── public dispatch ──────────────────────────────────────────────────

/// Execute a motion, returning the new cursor position.
pub fn execute_motion(
    motion: &MotionType,
    buffer: &Buffer,
    cursor: usize,
    count: Option<usize>,
    last_find: &Option<FindCommand>,
) -> usize {
    let c = count.unwrap_or(1);
    match motion {
        MotionType::Left => motion_h(buffer, cursor, c),
        MotionType::Right => motion_l(buffer, cursor, c),
        MotionType::Down => motion_j(buffer, cursor, c),
        MotionType::Up => motion_k(buffer, cursor, c),
        MotionType::WordForward => motion_w(buffer, cursor, c),
        MotionType::WordBackward => motion_b(buffer, cursor, c),
        MotionType::WordEnd => motion_e(buffer, cursor, c),
        MotionType::WORDForward => motion_big_w(buffer, cursor, c),
        MotionType::WORDBackward => motion_big_b(buffer, cursor, c),
        MotionType::WORDEnd => motion_big_e(buffer, cursor, c),
        MotionType::LineStart => motion_0(buffer, cursor),
        MotionType::LineEnd => motion_dollar(buffer, cursor),
        MotionType::FirstNonWhitespace => motion_caret(buffer, cursor),
        MotionType::DocumentStart => motion_gg(buffer, count),
        MotionType::DocumentEnd => motion_big_g(buffer, count),
        MotionType::FindForward(ch) => motion_find(buffer, cursor, *ch, true, c).unwrap_or(cursor),
        MotionType::FindBackward(ch) => {
            motion_find(buffer, cursor, *ch, false, c).unwrap_or(cursor)
        }
        MotionType::ToForward(ch) => motion_to(buffer, cursor, *ch, true, c).unwrap_or(cursor),
        MotionType::ToBackward(ch) => motion_to(buffer, cursor, *ch, false, c).unwrap_or(cursor),
        MotionType::MatchingBracket => motion_matching_bracket(buffer, cursor).unwrap_or(cursor),
        MotionType::ParagraphForward => motion_paragraph_forward(buffer, cursor, c),
        MotionType::ParagraphBackward => motion_paragraph_backward(buffer, cursor, c),
        MotionType::RepeatFind => {
            if let Some(fc) = last_find {
                if fc.inclusive {
                    motion_find(buffer, cursor, fc.char_target, fc.forward, c).unwrap_or(cursor)
                } else {
                    motion_to(buffer, cursor, fc.char_target, fc.forward, c).unwrap_or(cursor)
                }
            } else {
                cursor
            }
        }
        MotionType::RepeatFindReverse => {
            if let Some(fc) = last_find {
                if fc.inclusive {
                    motion_find(buffer, cursor, fc.char_target, !fc.forward, c).unwrap_or(cursor)
                } else {
                    motion_to(buffer, cursor, fc.char_target, !fc.forward, c).unwrap_or(cursor)
                }
            } else {
                cursor
            }
        }
    }
}

pub fn execute_operator_motion(
    operator: Operator,
    motion: &MotionType,
    buffer: &Buffer,
    cursor: usize,
    count: Option<usize>,
    last_find: &Option<FindCommand>,
) -> OperatorRangeSpec {
    let c = count.unwrap_or(1);

    match motion {
        MotionType::Left => OperatorRangeSpec {
            target: motion_h(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::Right => OperatorRangeSpec {
            target: motion_l_boundary(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::Down => OperatorRangeSpec {
            target: motion_j(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::Up => OperatorRangeSpec {
            target: motion_k(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::WordForward => {
            if operator == Operator::Change && change_uses_word_end(buffer, cursor) {
                OperatorRangeSpec {
                    target: motion_change_word_end(buffer, cursor, c),
                    inclusive: true,
                }
            } else {
                OperatorRangeSpec {
                    target: motion_w_boundary(buffer, cursor, c),
                    inclusive: false,
                }
            }
        }
        MotionType::WordBackward => OperatorRangeSpec {
            target: motion_b(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::WordEnd => OperatorRangeSpec {
            target: motion_e(buffer, cursor, c),
            inclusive: true,
        },
        MotionType::WORDForward => {
            if operator == Operator::Change && change_uses_word_end(buffer, cursor) {
                OperatorRangeSpec {
                    target: motion_change_big_word_end(buffer, cursor, c),
                    inclusive: true,
                }
            } else {
                OperatorRangeSpec {
                    target: motion_big_w_boundary(buffer, cursor, c),
                    inclusive: false,
                }
            }
        }
        MotionType::WORDBackward => OperatorRangeSpec {
            target: motion_big_b(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::WORDEnd => OperatorRangeSpec {
            target: motion_big_e(buffer, cursor, c),
            inclusive: true,
        },
        MotionType::LineStart => OperatorRangeSpec {
            target: motion_0(buffer, cursor),
            inclusive: false,
        },
        MotionType::LineEnd => OperatorRangeSpec {
            target: motion_dollar(buffer, cursor),
            inclusive: true,
        },
        MotionType::FirstNonWhitespace => OperatorRangeSpec {
            target: motion_caret(buffer, cursor),
            inclusive: false,
        },
        MotionType::DocumentStart => OperatorRangeSpec {
            target: motion_gg(buffer, count),
            inclusive: false,
        },
        MotionType::DocumentEnd => OperatorRangeSpec {
            target: motion_big_g(buffer, count),
            inclusive: false,
        },
        MotionType::FindForward(ch) => motion_find(buffer, cursor, *ch, true, c)
            .map(|target| OperatorRangeSpec {
                target,
                inclusive: true,
            })
            .unwrap_or(OperatorRangeSpec {
                target: cursor,
                inclusive: false,
            }),
        MotionType::FindBackward(ch) => motion_find(buffer, cursor, *ch, false, c)
            .map(|target| OperatorRangeSpec {
                target,
                inclusive: true,
            })
            .unwrap_or(OperatorRangeSpec {
                target: cursor,
                inclusive: false,
            }),
        MotionType::ToForward(ch) => motion_to(buffer, cursor, *ch, true, c)
            .map(|target| OperatorRangeSpec {
                target,
                inclusive: true,
            })
            .unwrap_or(OperatorRangeSpec {
                target: cursor,
                inclusive: false,
            }),
        MotionType::ToBackward(ch) => motion_to(buffer, cursor, *ch, false, c)
            .map(|target| OperatorRangeSpec {
                target,
                inclusive: true,
            })
            .unwrap_or(OperatorRangeSpec {
                target: cursor,
                inclusive: false,
            }),
        MotionType::MatchingBracket => motion_matching_bracket(buffer, cursor)
            .map(|target| OperatorRangeSpec {
                target,
                inclusive: true,
            })
            .unwrap_or(OperatorRangeSpec {
                target: cursor,
                inclusive: false,
            }),
        MotionType::ParagraphForward => OperatorRangeSpec {
            target: motion_paragraph_forward_boundary(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::ParagraphBackward => OperatorRangeSpec {
            target: motion_paragraph_backward(buffer, cursor, c),
            inclusive: false,
        },
        MotionType::RepeatFind => repeat_find_operator_motion(buffer, cursor, c, last_find, false),
        MotionType::RepeatFindReverse => {
            repeat_find_operator_motion(buffer, cursor, c, last_find, true)
        }
    }
}

fn change_uses_word_end(buffer: &Buffer, cursor: usize) -> bool {
    matches!(
        char_at(buffer, cursor).map(classify),
        Some(CharClass::Word | CharClass::Punctuation)
    )
}

fn repeat_find_operator_motion(
    buffer: &Buffer,
    cursor: usize,
    count: usize,
    last_find: &Option<FindCommand>,
    reverse: bool,
) -> OperatorRangeSpec {
    let Some(fc) = last_find else {
        return OperatorRangeSpec {
            target: cursor,
            inclusive: false,
        };
    };

    let target = if fc.inclusive {
        motion_find(buffer, cursor, fc.char_target, fc.forward ^ reverse, count)
    } else {
        motion_to(buffer, cursor, fc.char_target, fc.forward ^ reverse, count)
    };

    target
        .map(|target| OperatorRangeSpec {
            target,
            inclusive: true,
        })
        .unwrap_or(OperatorRangeSpec {
            target: cursor,
            inclusive: false,
        })
}

pub fn range_from_operator_spec(anchor: usize, spec: OperatorRangeSpec) -> Range<usize> {
    if spec.inclusive {
        ordered_inclusive_range(anchor, spec.target)
    } else {
        ordered_exclusive_range(anchor, spec.target)
    }
}

fn ordered_inclusive_range(a: usize, b: usize) -> Range<usize> {
    if a <= b {
        a..b.saturating_add(1)
    } else {
        b..a.saturating_add(1)
    }
}

fn ordered_exclusive_range(a: usize, b: usize) -> Range<usize> {
    if a <= b {
        a..b
    } else {
        b..a
    }
}
