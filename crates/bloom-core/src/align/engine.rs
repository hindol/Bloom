use bloom_buffer::Buffer;
use bloom_md::parser::extensions::LineElements;
use unicode_width::UnicodeWidthStr;

// ===========================================================================
// Public API
// ===========================================================================

/// Align all blocks in the entire buffer.
pub fn auto_align_page(buf: &mut Buffer) {
    let line_count = buf.len_lines();
    if line_count == 0 {
        return;
    }
    buf.begin_edit_group();
    let lines: Vec<String> = (0..line_count).map(|i| buf.line(i).to_string()).collect();

    align_frontmatter_block(buf, &lines);

    let mut i = 0;
    while i < lines.len() {
        if is_list_line(&lines[i]) {
            let start = i;
            while i < lines.len() && is_list_line(&lines[i]) {
                i += 1;
            }
            align_list_block(buf, start, i);
        } else if is_table_line(&lines[i]) {
            let start = i;
            while i < lines.len() && is_table_line(&lines[i]) {
                i += 1;
            }
            align_table_block(buf, start, i);
        } else {
            i += 1;
        }
    }
    buf.end_edit_group();
}

/// Align only the block around the given cursor line.
pub fn auto_align_block(buf: &mut Buffer, cursor_line: usize) {
    let line_count = buf.len_lines();
    if cursor_line >= line_count {
        return;
    }
    let lines: Vec<String> = (0..line_count).map(|i| buf.line(i).to_string()).collect();
    let cursor_text = &lines[cursor_line];

    buf.begin_edit_group();
    if is_in_frontmatter(&lines, cursor_line) {
        align_frontmatter_block(buf, &lines);
    } else if is_list_line(cursor_text) {
        let (start, end) = find_block_bounds(&lines, cursor_line, is_list_line);
        align_list_block(buf, start, end);
    } else if is_table_line(cursor_text) {
        let (start, end) = find_block_bounds(&lines, cursor_line, is_table_line);
        align_table_block(buf, start, end);
    }
    buf.end_edit_group();
}

// ===========================================================================
// Block detection helpers
// ===========================================================================

fn is_task_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- [ ] ") || t.starts_with("- [x] ") || t.starts_with("- [X] ")
}

fn is_list_line(line: &str) -> bool {
    line.trim_start().starts_with("- ")
}

fn is_table_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.len() > 1
}

fn is_in_frontmatter(lines: &[String], line_idx: usize) -> bool {
    if lines.is_empty() || lines[0].trim() != "---" {
        return false;
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            return line_idx <= i;
        }
    }
    false
}

fn find_block_bounds(lines: &[String], cursor: usize, pred: fn(&str) -> bool) -> (usize, usize) {
    let mut start = cursor;
    while start > 0 && pred(&lines[start - 1]) {
        start -= 1;
    }
    let mut end = cursor + 1;
    while end < lines.len() && pred(&lines[end]) {
        end += 1;
    }
    (start, end)
}

// ===========================================================================
// Line parsing helpers
// ===========================================================================

/// Split trailing ` ^xxxxx` block ID from line content.
fn split_block_id(line: &str) -> (&str, &str) {
    LineElements::split_block_id(line)
}

/// Find the byte position of the first `@due(`, `@start(`, or `@at(`.
fn find_first_timestamp(line: &str) -> Option<usize> {
    LineElements::first_timestamp_pos(line)
}

/// Move #tags that appear after @timestamps to before the first @.
fn relocate_post_at_tags(before: &str, after: &str) -> (String, String) {
    let mut tags = Vec::new();
    let mut cleaned = String::new();
    let mut chars = after.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '#' {
            let tag_start = i;
            let mut tag_end = i + ch.len_utf8();
            while let Some(&(j, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    tag_end = j + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            if tag_end > tag_start + 1 {
                tags.push(&after[tag_start..tag_end]);
            } else {
                cleaned.push_str(&after[tag_start..tag_end]);
            }
        } else {
            cleaned.push(ch);
        }
    }

    if tags.is_empty() {
        return (before.to_string(), after.to_string());
    }

    let mut new_before = before.trim_end().to_string();
    for tag in &tags {
        new_before.push(' ');
        new_before.push_str(tag);
    }
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (new_before, cleaned)
}

// ===========================================================================
// Generic column-alignment engine
// ===========================================================================

/// A parsed line split into segments for alignment.
struct AlignSegment {
    /// Text before the aligned element.
    prefix: String,
    /// The aligned element (timestamp, block ID, or empty).
    suffix: String,
    /// Whether this line participates in alignment.
    has_suffix: bool,
}

/// Align a set of segments to a common column. Requires `min_count` participants.
/// Applies edits bottom-to-top to preserve line indices.
fn align_segments(buf: &mut Buffer, start: usize, segments: &[AlignSegment], min_count: usize) {
    let participant_count = segments.iter().filter(|s| s.has_suffix).count();
    if participant_count < min_count {
        return;
    }

    let max_width = segments.iter().map(|s| s.prefix.width()).max().unwrap_or(0);
    if max_width == 0 {
        return;
    }
    let align_col = max_width + 1;

    for (i, seg) in segments.iter().enumerate().rev() {
        if !seg.has_suffix {
            continue;
        }
        let line_idx = start + i;
        let old_line = buf.line(line_idx).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');

        let padding = align_col.saturating_sub(seg.prefix.width());
        let new_line = format!("{}{}{}", seg.prefix, " ".repeat(padding), seg.suffix);

        if new_line != old_trimmed {
            let ls = buf.text().line_to_char(line_idx);
            buf.replace(ls..ls + old_trimmed.len(), &new_line);
        }
    }
}

/// Replace a single line in the buffer (by line index).
fn replace_line(buf: &mut Buffer, line_idx: usize, new_text: &str) {
    let old_line = buf.line(line_idx).to_string();
    let old_trimmed = old_line.trim_end_matches('\n');
    if new_text != old_trimmed {
        let ls = buf.text().line_to_char(line_idx);
        buf.replace(ls..ls + old_trimmed.len(), new_text);
    }
}

// ===========================================================================
// List block alignment (timestamps + block IDs)
// ===========================================================================

/// Align a contiguous list block: timestamps first, then block IDs.
fn align_list_block(buf: &mut Buffer, start: usize, end: usize) {
    align_timestamps(buf, start, end);
    // Re-read after timestamp alignment may have changed content
    align_block_ids(buf, start, end);
}

fn align_timestamps(buf: &mut Buffer, start: usize, end: usize) {
    let lines: Vec<String> = (start..end).map(|i| buf.line(i).to_string()).collect();

    // Check if the block has any timestamps worth aligning
    let has_any = lines.iter().any(|l| {
        is_task_line(l) && find_first_timestamp(l).is_some()
    });
    if !has_any {
        return;
    }

    // Parse each line: split into (text_before_@, @_and_after, block_id)
    // Also relocate tags from after @ to before @.
    let mut segments = Vec::new();
    let mut relocated_lines = Vec::new(); // for non-padded tag relocation

    for line in &lines {
        let trimmed = line.trim_end_matches('\n');
        let (content, block_id) = split_block_id(trimmed);

        if let Some(at_pos) = find_first_timestamp(content) {
            let (relocated_before, cleaned_after) =
                relocate_post_at_tags(&content[..at_pos], &content[at_pos..]);
            let prefix = relocated_before.trim_end().to_string();
            let suffix = format!("{}{}", cleaned_after.trim(), block_id);
            relocated_lines.push(format!("{} {}", prefix, suffix));
            segments.push(AlignSegment {
                prefix,
                suffix,
                has_suffix: true,
            });
        } else {
            let full = format!("{}{}", content.trim_end(), block_id);
            relocated_lines.push(full.clone());
            segments.push(AlignSegment {
                prefix: full,
                suffix: String::new(),
                has_suffix: false,
            });
        }
    }

    // If 2+ timestamps, pad to a common column
    let at_count = segments.iter().filter(|s| s.has_suffix).count();
    if at_count >= 2 {
        align_segments(buf, start, &segments, 2);
    } else {
        // Still apply tag relocation (canonical form) without padding
        for (i, new_text) in relocated_lines.iter().enumerate().rev() {
            replace_line(buf, start + i, new_text);
        }
    }
}

fn align_block_ids(buf: &mut Buffer, start: usize, end: usize) {
    let lines: Vec<String> = (start..end).map(|i| buf.line(i).to_string()).collect();

    let segments: Vec<AlignSegment> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_end_matches('\n');
            let (content, bid) = split_block_id(trimmed);
            AlignSegment {
                prefix: content.to_string(),
                suffix: bid.trim_start().to_string(),
                has_suffix: !bid.is_empty(),
            }
        })
        .collect();

    align_segments(buf, start, &segments, 2);
}

// ===========================================================================
// Table alignment
// ===========================================================================

fn align_table_block(buf: &mut Buffer, start: usize, end: usize) {
    let lines: Vec<String> = (start..end).map(|i| buf.line(i).to_string()).collect();
    if lines.is_empty() {
        return;
    }

    // Parse cells per row
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim().trim_matches('|');
            trimmed.split('|').map(|c| c.trim().to_string()).collect()
        })
        .collect();

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return;
    }

    // Compute max width per column
    let mut col_widths = vec![0usize; col_count];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                col_widths[i] = col_widths[i].max(cell.width());
            }
        }
    }

    // Rebuild each line
    for (i, row) in rows.iter().enumerate().rev() {
        let line_idx = start + i;
        let old_line = buf.line(line_idx).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');

        let is_separator = old_trimmed.contains("---");
        let cells: Vec<String> = (0..col_count)
            .map(|ci| {
                let w = col_widths[ci];
                if is_separator {
                    "-".repeat(w)
                } else {
                    let cell = row.get(ci).map(|s| s.as_str()).unwrap_or("");
                    let pad = w.saturating_sub(cell.width());
                    format!("{}{}", cell, " ".repeat(pad))
                }
            })
            .collect();

        let new_line = format!("| {} |", cells.join(" | "));
        replace_line(buf, line_idx, &new_line);
    }
}

// ===========================================================================
// Frontmatter alignment
// ===========================================================================

fn align_frontmatter_block(buf: &mut Buffer, lines: &[String]) {
    if lines.is_empty() || lines[0].trim() != "---" {
        return;
    }

    let end = lines.iter().enumerate().skip(1).find_map(|(i, l)| {
        if l.trim() == "---" { Some(i) } else { None }
    }).unwrap_or(0);
    if end == 0 {
        return;
    }

    let max_key_len = lines[1..end]
        .iter()
        .filter_map(|l| l.find(": ").map(|p| l[..p].trim().width()))
        .max()
        .unwrap_or(0);
    if max_key_len == 0 {
        return;
    }

    for i in (1..end).rev() {
        let old_line = buf.line(i).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');
        if let Some(colon) = old_trimmed.find(": ") {
            let key = &old_trimmed[..colon];
            let value = old_trimmed[colon + 1..].trim_start();
            let padding = max_key_len.saturating_sub(key.trim().width());
            let new_line = format!("{}:{}{}", key.trim(), " ".repeat(padding + 1), value);
            replace_line(buf, i, &new_line);
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_buffer::Buffer;

    #[test]
    fn test_timestamp_alignment() {
        let mut buf = Buffer::from_text(
            "- [ ] Short task @due(2026-03-05)\n\
             - [ ] A much longer task description @due(2026-03-10)\n\
             - [x] Done @due(2026-03-04)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(positions.len(), 3);
        assert!(positions[0] == positions[1] && positions[1] == positions[2]);
    }

    #[test]
    fn test_timestamp_no_at_line_untouched() {
        let mut buf = Buffer::from_text(
            "- [ ] Has timestamp @due(2026-03-05)\n\
             - [ ] No timestamp here\n\
             - [ ] Another @due(2026-03-10)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let line2 = text.lines().nth(1).unwrap();
        assert_eq!(line2, "- [ ] No timestamp here");
    }

    #[test]
    fn test_tag_relocation() {
        let mut buf = Buffer::from_text("- [ ] Fix parser @due(2026-03-10) #rust\n");
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let line = text.lines().next().unwrap();
        let tag_pos = line.find("#rust").unwrap();
        let at_pos = line.find("@due").unwrap();
        assert!(tag_pos < at_pos, "tag should be before @due: {}", line);
    }

    #[test]
    fn test_table_alignment() {
        let mut buf = Buffer::from_text(
            "| Key | Action |\n|---|---|\n| `w` | Next word start |\n| `b` | Previous word start |\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        let pipe_positions: Vec<usize> = lines
            .iter()
            .filter_map(|l| {
                let first = l.find('|')?;
                l[first + 1..].find('|').map(|p| p + first + 1)
            })
            .collect();
        assert!(pipe_positions.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn test_frontmatter_alignment() {
        let mut buf = Buffer::from_text(
            "---\nid: abc\ntitle: \"My Page\"\ncreated: 2026-03-01\ntags: [rust]\n---\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let colon_positions: Vec<usize> = text
            .lines()
            .filter(|l| l.contains(':') && *l != "---")
            .filter_map(|l| l.find(':'))
            .collect();
        assert!(!colon_positions.is_empty());
    }

    #[test]
    fn test_idempotent() {
        let mut buf = Buffer::from_text(
            "---\nid: abc\ntitle: \"Page\"\n---\n\n- [ ] Short @due(2026-03-05)\n- [ ] Longer task @due(2026-03-10)\n",
        );
        auto_align_page(&mut buf);
        let first = buf.text().to_string();
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), first);
    }

    #[test]
    fn test_block_mode() {
        let mut buf = Buffer::from_text(
            "- [ ] Task A @due(2026-03-05)\n- [ ] Task B long name @due(2026-03-10)\n\nUnrelated paragraph\n",
        );
        auto_align_block(&mut buf, 0);
        let text = buf.text().to_string();
        let positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], positions[1]);
    }

    #[test]
    fn test_no_align_when_no_blocks() {
        let mut buf = Buffer::from_text("Just a plain line\nAnother line\n");
        let before = buf.text().to_string();
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), before);
    }

    #[test]
    fn test_longest_line_without_at_sets_column() {
        let mut buf = Buffer::from_text(
            "- [ ] Short @due(2026-03-05)\n\
             - This is a really long line without a timestamp at all\n\
             - [ ] Medium @due(2026-03-10)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let at_positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(at_positions.len(), 2);
        assert_eq!(at_positions[0], at_positions[1]);
        let long_line_width = "- This is a really long line without a timestamp at all".len();
        assert!(at_positions[0] > long_line_width);
    }

    #[test]
    fn test_start_before_due_aligns_on_earliest() {
        let mut buf = Buffer::from_text(
            "- [ ] Task A @start(2026-03-01) @due(2026-03-05)\n\
             - [ ] Task B @due(2026-03-10)\n\
             - [ ] Task C @start(2026-03-08) @due(2026-03-15)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        let at_pos_0 = lines[0].find("@").unwrap();
        let at_pos_1 = lines[1].find("@").unwrap();
        let at_pos_2 = lines[2].find("@").unwrap();
        assert_eq!(at_pos_0, at_pos_1);
        assert_eq!(at_pos_1, at_pos_2);
    }

    #[test]
    fn test_frontmatter_no_extra_space() {
        let mut buf = Buffer::from_text(
            "---\nid: abc\ntitle: \"My Page\"\ncreated: 2026-03-01\ntags: [rust]\n---\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        for line in text.lines() {
            if line.contains(':') && line != "---" {
                let colon_pos = line.find(':').unwrap();
                let after_colon = &line[colon_pos + 1..];
                let value_start = after_colon.find(|c: char| c != ' ').unwrap_or(0);
                assert!(value_start >= 1);
            }
        }
        let first = text.clone();
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), first);
    }

    #[test]
    fn test_timestamp_alignment_with_block_ids() {
        let mut buf = Buffer::from_text(
            "- [ ] Short task @due(2026-03-05) ^a1b2c\n\
             - [ ] A much longer task description @due(2026-03-10) ^d3e4f\n\
             - [x] Done @due(2026-03-04) ^g5h6i\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], positions[1]);
        assert_eq!(positions[1], positions[2]);
        for line in text.lines() {
            assert!(line.contains(" ^"), "block ID should be preserved: {}", line);
        }
    }

    #[test]
    fn test_timestamp_alignment_mixed_block_ids() {
        let mut buf = Buffer::from_text(
            "- [ ] Review ropey API #rust @due(2026-03-05) ^a1b2c\n\
             - [ ] Fix parser @due(2026-03-10) #rust ^d3e4f\n\
             - [ ] Read DDIA @start(2026-03-02) @due(2026-03-15)\n\
             - [ ] Write tests #testing ^g5h6i\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let at_positions: Vec<usize> = text
            .lines()
            .filter_map(|l| find_first_timestamp(l))
            .collect();
        assert_eq!(at_positions.len(), 3);
        assert_eq!(at_positions[0], at_positions[1]);
        assert_eq!(at_positions[1], at_positions[2]);
    }

    #[test]
    fn test_non_task_list_items_contribute_to_alignment_width() {
        let mut buf = Buffer::from_text(
            "- This is a regular note that is quite long\n\
             - [ ] Short task @due(2026-03-05)\n\
             - [ ] Another task @due(2026-03-10)\n\
             - And another plain bullet point\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let at_positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(at_positions.len(), 2);
        assert_eq!(at_positions[0], at_positions[1]);
        let longest_plain = "- This is a regular note that is quite long".len();
        assert!(at_positions[0] > longest_plain);
    }

    #[test]
    fn test_block_ids_excluded_from_alignment_width() {
        let input = "- [ ] Short @due(2026-03-05)\n\
                      - [ ] Medium task @due(2026-03-10)\n\
                      - [ ] Write tests #testing ^g5h6i\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        // Timestamps should align
        let at_positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(at_positions.len(), 2);
        assert_eq!(at_positions[0], at_positions[1]);
        // Block ID should be preserved
        assert!(text.contains("^g5h6i"), "block ID preserved");
        // Idempotent
        let first = text.clone();
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), first, "should be idempotent");
    }

    #[test]
    fn test_single_timestamp_no_padding() {
        let input = "- [ ] Only task @due(2026-03-05)\n- Regular note\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        assert!(!text.contains("  @due"), "single timestamp: no extra padding\n{}", text);
    }

    #[test]
    fn test_block_id_alignment_in_list() {
        let mut buf = Buffer::from_text(
            "- Short item ^a1b2c\n\
             - A much longer list item here ^d3e4f\n\
             - Medium item ^g5h6i\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let caret_positions: Vec<usize> = text.lines().filter_map(|l| l.rfind(" ^")).collect();
        assert_eq!(caret_positions.len(), 3);
        assert_eq!(caret_positions[0], caret_positions[1]);
        assert_eq!(caret_positions[1], caret_positions[2]);
    }

    #[test]
    fn test_single_block_id_no_padding() {
        let input = "- Only item ^a1b2c\n- No block id here\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        assert_eq!(text.lines().next().unwrap(), "- Only item ^a1b2c");
    }
}
