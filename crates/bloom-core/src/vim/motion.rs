use crate::buffer::Buffer;

/// Types of motions the grammar parser can identify.
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

// ── individual motions ───────────────────────────────────────────────

fn motion_h(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let start = line_start(buffer, cursor);
    cursor.saturating_sub(count).max(start)
}

fn motion_l(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let end = line_last(buffer, cursor);
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

fn motion_w(buffer: &Buffer, cursor: usize, count: usize) -> usize {
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
        while pos < len && matches!(classify(buffer.text().char(pos)), CharClass::Whitespace | CharClass::Newline) {
            pos += 1;
        }
    }
    pos.min(len.saturating_sub(1))
}

fn motion_b(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..count {
        if pos == 0 {
            break;
        }
        pos -= 1;
        // Skip whitespace/newlines backward
        while pos > 0 && matches!(classify(buffer.text().char(pos)), CharClass::Whitespace | CharClass::Newline) {
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
        while pos < len && matches!(classify(buffer.text().char(pos)), CharClass::Whitespace | CharClass::Newline) {
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

fn is_word_boundary_for_big(c: char) -> bool {
    c.is_whitespace()
}

fn motion_big_w(buffer: &Buffer, cursor: usize, count: usize) -> usize {
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
    pos.min(len.saturating_sub(1))
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

fn motion_find(buffer: &Buffer, cursor: usize, target: char, forward: bool, count: usize) -> Option<usize> {
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

fn motion_to(buffer: &Buffer, cursor: usize, target: char, forward: bool, count: usize) -> Option<usize> {
    motion_find(buffer, cursor, target, forward, count).map(|pos| {
        if forward {
            pos.saturating_sub(1).max(cursor)
        } else {
            (pos + 1).min(cursor)
        }
    })
}

fn motion_matching_bracket(buffer: &Buffer, cursor: usize) -> usize {
    let c = match char_at(buffer, cursor) {
        Some(c) => c,
        None => return cursor,
    };
    let (target, forward) = match c {
        '(' => (')', true),
        ')' => ('(', false),
        '[' => (']', true),
        ']' => ('[', false),
        '{' => ('}', true),
        '}' => ('{', false),
        _ => return cursor,
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
                    return pos;
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
                    return pos;
                }
            }
        }
    }
    cursor
}

fn motion_paragraph_forward(buffer: &Buffer, cursor: usize, count: usize) -> usize {
    let rope = buffer.text();
    let len = buffer.len_chars();
    let mut pos = cursor;
    for _ in 0..count {
        // Skip non-blank lines
        while pos < len {
            let line_idx = rope.char_to_line(pos);
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0
                || (line.len_chars() == 1 && line.char(0) == '\n');
            if blank {
                break;
            }
            let next_line = line_idx + 1;
            if next_line >= rope.len_lines() {
                return len.saturating_sub(1);
            }
            pos = rope.line_to_char(next_line);
        }
        // Skip blank lines
        while pos < len {
            let line_idx = rope.char_to_line(pos);
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0
                || (line.len_chars() == 1 && line.char(0) == '\n');
            if !blank {
                break;
            }
            let next_line = line_idx + 1;
            if next_line >= rope.len_lines() {
                return len.saturating_sub(1);
            }
            pos = rope.line_to_char(next_line);
        }
    }
    pos.min(len.saturating_sub(1))
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
            let blank = line.len_chars() == 0
                || (line.len_chars() == 1 && line.char(0) == '\n');
            if blank || line_idx == 0 {
                break;
            }
            line_idx -= 1;
        }
        // Skip blank lines backward
        loop {
            let line = rope.line(line_idx);
            let blank = line.len_chars() == 0
                || (line.len_chars() == 1 && line.char(0) == '\n');
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
        MotionType::FindBackward(ch) => motion_find(buffer, cursor, *ch, false, c).unwrap_or(cursor),
        MotionType::ToForward(ch) => motion_to(buffer, cursor, *ch, true, c).unwrap_or(cursor),
        MotionType::ToBackward(ch) => motion_to(buffer, cursor, *ch, false, c).unwrap_or(cursor),
        MotionType::MatchingBracket => motion_matching_bracket(buffer, cursor),
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