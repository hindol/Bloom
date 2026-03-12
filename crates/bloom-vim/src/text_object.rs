//! Text object resolution (inner and around variants).
//!
//! Resolves `iw`, `a"`, `i[`, and Bloom-specific objects (`il` for links,
//! `it` for tags, `id` for timestamps, `ih` for headings) to character ranges
//! in the buffer. Used by the Vim grammar for `operator + text-object` commands.

use bloom_buffer::Buffer;
use std::ops::Range;

/// The type of a text object (inner or around).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextObjectType {
    Inner(ObjectKind),
    Around(ObjectKind),
}

/// Kinds of text objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectKind {
    Word,
    WORD,
    Paragraph,
    DoubleQuote,
    SingleQuote,
    Paren,
    Brace,
    Bracket,
    // Bloom-specific
    Link,
    Tag,
    Timestamp,
    Heading,
}

/// Resolve a text object to a character range in the buffer.
pub fn resolve_text_object(
    obj: &TextObjectType,
    buffer: &Buffer,
    cursor: usize,
) -> Option<Range<usize>> {
    match obj {
        TextObjectType::Inner(kind) => inner(kind, buffer, cursor),
        TextObjectType::Around(kind) => around(kind, buffer, cursor),
    }
}

fn inner(kind: &ObjectKind, buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    match kind {
        ObjectKind::Word => inner_word(buffer, cursor, false),
        ObjectKind::WORD => inner_word(buffer, cursor, true),
        ObjectKind::Paragraph => inner_paragraph(buffer, cursor),
        ObjectKind::DoubleQuote => inner_delim_pair(buffer, cursor, '"', '"'),
        ObjectKind::SingleQuote => inner_delim_pair(buffer, cursor, '\'', '\''),
        ObjectKind::Paren => inner_nested_pair(buffer, cursor, '(', ')'),
        ObjectKind::Brace => inner_nested_pair(buffer, cursor, '{', '}'),
        ObjectKind::Bracket => inner_nested_pair(buffer, cursor, '[', ']'),
        ObjectKind::Link => inner_link(buffer, cursor),
        ObjectKind::Tag => inner_tag(buffer, cursor),
        ObjectKind::Timestamp => inner_timestamp(buffer, cursor),
        ObjectKind::Heading => inner_heading(buffer, cursor),
    }
}

fn around(kind: &ObjectKind, buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    match kind {
        ObjectKind::Word => around_word(buffer, cursor, false),
        ObjectKind::WORD => around_word(buffer, cursor, true),
        ObjectKind::Paragraph => around_paragraph(buffer, cursor),
        ObjectKind::DoubleQuote => around_delim_pair(buffer, cursor, '"', '"'),
        ObjectKind::SingleQuote => around_delim_pair(buffer, cursor, '\'', '\''),
        ObjectKind::Paren => around_nested_pair(buffer, cursor, '(', ')'),
        ObjectKind::Brace => around_nested_pair(buffer, cursor, '{', '}'),
        ObjectKind::Bracket => around_nested_pair(buffer, cursor, '[', ']'),
        ObjectKind::Link => around_link(buffer, cursor),
        ObjectKind::Tag => around_tag(buffer, cursor),
        ObjectKind::Timestamp => around_timestamp(buffer, cursor),
        ObjectKind::Heading => around_heading(buffer, cursor),
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// ── word ─────────────────────────────────────────────────────────────

fn inner_word(buffer: &Buffer, cursor: usize, big: bool) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    if cursor >= len {
        return None;
    }
    let c = buffer.text().char(cursor);
    let pred: Box<dyn Fn(char) -> bool> = if big {
        Box::new(|ch: char| !ch.is_whitespace())
    } else if is_word_char(c) {
        Box::new(is_word_char)
    } else if !c.is_whitespace() {
        Box::new(|ch: char| !is_word_char(ch) && !ch.is_whitespace())
    } else {
        Box::new(|ch: char| ch.is_whitespace() && ch != '\n')
    };
    let mut start = cursor;
    while start > 0 && pred(buffer.text().char(start - 1)) {
        start -= 1;
    }
    let mut end = cursor;
    while end < len && pred(buffer.text().char(end)) {
        end += 1;
    }
    Some(start..end)
}

fn around_word(buffer: &Buffer, cursor: usize, big: bool) -> Option<Range<usize>> {
    let inner = inner_word(buffer, cursor, big)?;
    let len = buffer.len_chars();
    let mut end = inner.end;
    // Include trailing whitespace
    while end < len && buffer.text().char(end).is_whitespace() && buffer.text().char(end) != '\n' {
        end += 1;
    }
    if end == inner.end {
        // No trailing whitespace — include leading
        let mut start = inner.start;
        while start > 0
            && buffer.text().char(start - 1).is_whitespace()
            && buffer.text().char(start - 1) != '\n'
        {
            start -= 1;
        }
        Some(start..inner.end)
    } else {
        Some(inner.start..end)
    }
}

// ── paragraph ────────────────────────────────────────────────────────

fn is_blank_line(buffer: &Buffer, line_idx: usize) -> bool {
    let line = buffer.line(line_idx);
    line.len_chars() == 0 || (line.len_chars() == 1 && line.char(0) == '\n')
}

fn inner_paragraph(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let total = rope.len_lines();
    let cur_blank = is_blank_line(buffer, cur_line);
    let mut start_line = cur_line;
    while start_line > 0 && is_blank_line(buffer, start_line - 1) == cur_blank {
        start_line -= 1;
    }
    let mut end_line = cur_line;
    while end_line + 1 < total && is_blank_line(buffer, end_line + 1) == cur_blank {
        end_line += 1;
    }
    let start = rope.line_to_char(start_line);
    let end = if end_line + 1 < total {
        rope.line_to_char(end_line + 1)
    } else {
        buffer.len_chars()
    };
    Some(start..end)
}

fn around_paragraph(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let total = rope.len_lines();
    let cur_blank = is_blank_line(buffer, cur_line);
    let mut start_line = cur_line;
    while start_line > 0 && is_blank_line(buffer, start_line - 1) == cur_blank {
        start_line -= 1;
    }
    let mut end_line = cur_line;
    while end_line + 1 < total && is_blank_line(buffer, end_line + 1) == cur_blank {
        end_line += 1;
    }
    // Include following blank lines (or preceding if at end)
    let mut after = end_line + 1;
    while after < total && is_blank_line(buffer, after) != cur_blank {
        after += 1;
    }
    let start = rope.line_to_char(start_line);
    let end = if after < total {
        rope.line_to_char(after)
    } else {
        buffer.len_chars()
    };
    Some(start..end)
}

// ── delimiter pairs ──────────────────────────────────────────────────

fn inner_delim_pair(
    buffer: &Buffer,
    cursor: usize,
    open: char,
    close: char,
) -> Option<Range<usize>> {
    let rope = buffer.text();
    let line_idx = rope.char_to_line(cursor);
    let line_start = rope.line_to_char(line_idx);
    let line = rope.line(line_idx);
    let col = cursor - line_start;

    // Search backward for open
    let mut start = None;
    for i in (0..=col).rev() {
        if line.char(i) == open {
            start = Some(line_start + i + 1);
            break;
        }
    }
    let start = start?;

    // Search forward for close
    for i in (col + 1)..line.len_chars() {
        if line.char(i) == close {
            return Some(start..line_start + i);
        }
    }
    // Also handle cursor ON the open delimiter
    if col < line.len_chars() && line.char(col) == open {
        for i in (col + 1)..line.len_chars() {
            if line.char(i) == close {
                return Some(line_start + col + 1..line_start + i);
            }
        }
    }
    None
}

fn around_delim_pair(
    buffer: &Buffer,
    cursor: usize,
    open: char,
    close: char,
) -> Option<Range<usize>> {
    let inner = inner_delim_pair(buffer, cursor, open, close)?;
    if inner.start > 0 {
        Some(inner.start - 1..inner.end + 1)
    } else {
        Some(inner.start..inner.end + 1)
    }
}

fn inner_nested_pair(
    buffer: &Buffer,
    cursor: usize,
    open: char,
    close: char,
) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    // Find open bracket backward
    let mut depth = 0i32;
    let mut start = None;
    let mut pos = cursor;
    loop {
        let c = buffer.text().char(pos);
        if c == close {
            depth += 1;
        } else if c == open {
            if depth == 0 {
                start = Some(pos + 1);
                break;
            }
            depth -= 1;
        }
        if pos == 0 {
            break;
        }
        pos -= 1;
    }
    let start = start?;

    // Find close bracket forward
    depth = 0;
    for p in start..len {
        let c = buffer.text().char(p);
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some(start..p);
            }
            depth -= 1;
        }
    }
    None
}

fn around_nested_pair(
    buffer: &Buffer,
    cursor: usize,
    open: char,
    close: char,
) -> Option<Range<usize>> {
    let inner = inner_nested_pair(buffer, cursor, open, close)?;
    if inner.start > 0 {
        Some(inner.start - 1..inner.end + 1)
    } else {
        Some(inner.start..inner.end + 1)
    }
}

// ── Bloom-specific: link [[...]] ─────────────────────────────────────

fn find_link_bounds(buffer: &Buffer, cursor: usize) -> Option<(usize, usize)> {
    let text: String = buffer.text().slice(..).to_string();
    let cursor_byte = buffer
        .text()
        .char_to_byte(cursor.min(buffer.len_chars().saturating_sub(1)));
    // Find [[ before or at cursor
    let before = &text[..=cursor_byte.min(text.len().saturating_sub(1))];
    let open_byte = before.rfind("[[")?;
    let open_char = buffer.text().byte_to_char(open_byte);
    // Find ]] after cursor
    let search_start = open_byte + 2;
    let close_byte = text[search_start..].find("]]").map(|p| search_start + p)?;
    let close_char = buffer.text().byte_to_char(close_byte);
    // Make sure cursor is between them
    if cursor >= open_char && cursor <= close_char + 1 {
        Some((open_char, close_char))
    } else {
        None
    }
}

fn inner_link(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let (open, close) = find_link_bounds(buffer, cursor)?;
    Some(open + 2..close)
}

fn around_link(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let (open, close) = find_link_bounds(buffer, cursor)?;
    Some(open..close + 2)
}

// ── Bloom-specific: tag #name ────────────────────────────────────────

fn inner_tag(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let len = buffer.len_chars();
    // Find # at or before cursor
    let mut hash_pos = None;
    let mut pos = cursor;
    loop {
        let c = buffer.text().char(pos);
        if c == '#' {
            hash_pos = Some(pos);
            break;
        }
        if c.is_whitespace() || pos == 0 {
            break;
        }
        pos -= 1;
    }
    let hash_pos = hash_pos?;
    let start = hash_pos + 1;
    let mut end = start;
    while end < len {
        let c = buffer.text().char(end);
        if c.is_whitespace() || c == '#' {
            break;
        }
        end += 1;
    }
    if start < end {
        Some(start..end)
    } else {
        None
    }
}

fn around_tag(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let inner = inner_tag(buffer, cursor)?;
    if inner.start > 0 {
        Some(inner.start - 1..inner.end) // include the #
    } else {
        Some(inner)
    }
}

// ── Bloom-specific: timestamp @due(...) ──────────────────────────────

fn inner_timestamp(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let text: String = buffer.text().slice(..).to_string();
    let cursor_byte = buffer
        .text()
        .char_to_byte(cursor.min(buffer.len_chars().saturating_sub(1)));
    // Find @word( before cursor
    let before = &text[..=cursor_byte.min(text.len().saturating_sub(1))];
    // Search backward for @
    let at_byte = before.rfind('@')?;
    let paren_byte = text[at_byte..].find('(')?;
    let open = at_byte + paren_byte + 1;
    let close = text[open..].find(')')? + open;
    let open_char = buffer.text().byte_to_char(open);
    let close_char = buffer.text().byte_to_char(close);
    if cursor >= buffer.text().byte_to_char(at_byte) && cursor <= close_char {
        Some(open_char..close_char)
    } else {
        None
    }
}

fn around_timestamp(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let text: String = buffer.text().slice(..).to_string();
    let cursor_byte = buffer
        .text()
        .char_to_byte(cursor.min(buffer.len_chars().saturating_sub(1)));
    let before = &text[..=cursor_byte.min(text.len().saturating_sub(1))];
    let at_byte = before.rfind('@')?;
    let close = text[at_byte..].find(')')? + at_byte + 1;
    let at_char = buffer.text().byte_to_char(at_byte);
    let close_char = buffer.text().byte_to_char(close);
    if cursor >= at_char && cursor <= close_char {
        Some(at_char..close_char)
    } else {
        None
    }
}

// ── Bloom-specific: heading section ──────────────────────────────────

fn heading_level(buffer: &Buffer, line_idx: usize) -> Option<usize> {
    let line = buffer.line(line_idx);
    let mut level = 0;
    for i in 0..line.len_chars() {
        if line.char(i) == '#' {
            level += 1;
        } else {
            break;
        }
    }
    if level > 0 && level < line.len_chars() && line.char(level) == ' ' {
        Some(level)
    } else {
        None
    }
}

fn inner_heading(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let total = rope.len_lines();

    // Find the heading line at or above cursor
    let mut heading_line = cur_line;
    while heading_level(buffer, heading_line).is_none() && heading_line > 0 {
        heading_line -= 1;
    }
    let level = heading_level(buffer, heading_line)?;

    // Content starts on the line after the heading
    let start_line = heading_line + 1;
    if start_line >= total {
        return None;
    }

    // Find the end: next heading of same or higher level
    let mut end_line = start_line;
    while end_line < total {
        if let Some(l) = heading_level(buffer, end_line) {
            if l <= level {
                break;
            }
        }
        end_line += 1;
    }

    let start = rope.line_to_char(start_line);
    let end = if end_line < total {
        rope.line_to_char(end_line)
    } else {
        buffer.len_chars()
    };
    Some(start..end)
}

fn around_heading(buffer: &Buffer, cursor: usize) -> Option<Range<usize>> {
    let rope = buffer.text();
    let cur_line = rope.char_to_line(cursor);
    let total = rope.len_lines();

    let mut heading_line = cur_line;
    while heading_level(buffer, heading_line).is_none() && heading_line > 0 {
        heading_line -= 1;
    }
    let level = heading_level(buffer, heading_line)?;

    let mut end_line = heading_line + 1;
    while end_line < total {
        if let Some(l) = heading_level(buffer, end_line) {
            if l <= level {
                break;
            }
        }
        end_line += 1;
    }

    let start = rope.line_to_char(heading_line);
    let end = if end_line < total {
        rope.line_to_char(end_line)
    } else {
        buffer.len_chars()
    };
    Some(start..end)
}
