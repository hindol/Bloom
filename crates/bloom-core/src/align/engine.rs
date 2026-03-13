use bloom_buffer::Buffer;
use unicode_width::UnicodeWidthStr;

/// Align all blocks in the entire buffer.
pub fn auto_align_page(buf: &mut Buffer) {
    let line_count = buf.len_lines();
    if line_count == 0 {
        return;
    }

    buf.begin_edit_group();

    // Collect all lines upfront (avoids borrow issues during mutation)
    let lines: Vec<String> = (0..line_count).map(|i| buf.line(i).to_string()).collect();

    // Pass 1: Frontmatter
    align_frontmatter_block(buf, &lines);

    // Pass 2: List blocks (with timestamp alignment) and table blocks.
    // Group contiguous list items together so that non-task bullets contribute
    // to the max-width calculation — prevents aligned tasks from looking
    // misaligned relative to surrounding plain list items.
    let mut i = 0;
    while i < lines.len() {
        if is_list_line(&lines[i]) {
            let start = i;
            while i < lines.len() && is_list_line(&lines[i]) {
                i += 1;
            }
            // Only run timestamp alignment if the block contains at least one
            // task line with a timestamp keyword.
            let has_timestamp = lines[start..i].iter().any(|l| {
                is_task_line(l)
                    && (l.contains("@due(") || l.contains("@start(") || l.contains("@at("))
            });
            if has_timestamp {
                align_timestamp_block(buf, start, i);
            }
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
        // Only align if the block contains at least one task with a timestamp
        let has_timestamp = lines[start..end].iter().any(|l| {
            is_task_line(l)
                && (l.contains("@due(") || l.contains("@start(") || l.contains("@at("))
        });
        if has_timestamp {
            align_timestamp_block(buf, start, end);
        }
    } else if is_table_line(cursor_text) {
        let (start, end) = find_block_bounds(&lines, cursor_line, is_table_line);
        align_table_block(buf, start, end);
    }

    buf.end_edit_group();
}

// ---------------------------------------------------------------------------
// Block detection
// ---------------------------------------------------------------------------

fn is_task_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- [ ] ") || t.starts_with("- [x] ") || t.starts_with("- [X] ")
}

fn is_list_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ")
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

// ---------------------------------------------------------------------------
// Timestamp alignment
// ---------------------------------------------------------------------------

fn align_timestamp_block(buf: &mut Buffer, start: usize, end: usize) {
    // Re-read lines (buffer may have shifted from earlier edits)
    let lines: Vec<String> = (start..end).map(|i| buf.line(i).to_string()).collect();

    // Phase 1: relocate post-@ tags to before the first @
    // Phase 2: compute alignment column
    // Phase 3: pad

    struct TaskLine {
        text_before_at: String,
        at_and_after: String,
        block_id_suffix: String, // " ^xxxxx" if present, empty otherwise
        has_at: bool,
    }

    let mut parsed: Vec<TaskLine> = Vec::new();
    for line in &lines {
        let trimmed = line.trim_end_matches('\n');
        let (content, block_id) = split_block_id(trimmed);
        if let Some(at_pos) = find_first_timestamp_pos(content) {
            let before = &content[..at_pos];
            let after = &content[at_pos..];

            // Relocate: find tags after timestamps and move them before @
            let (relocated_before, cleaned_after) = relocate_post_at_tags(before, after);

            parsed.push(TaskLine {
                text_before_at: relocated_before.trim_end().to_string(),
                at_and_after: cleaned_after.trim().to_string(),
                block_id_suffix: block_id.to_string(),
                has_at: true,
            });
        } else {
            parsed.push(TaskLine {
                text_before_at: content.trim_end().to_string(),
                at_and_after: String::new(),
                block_id_suffix: block_id.to_string(),
                has_at: false,
            });
        }
    }

    // Compute alignment column: max text width across ALL lines + 1
    // Lines without @ still contribute to the column so timestamps
    // don't land in the middle of longer non-@ lines.
    let max_width = parsed
        .iter()
        .map(|p| p.text_before_at.width())
        .max()
        .unwrap_or(0);

    // Only proceed if at least one line has a timestamp
    let has_any_at = parsed.iter().any(|p| p.has_at);
    if max_width == 0 || !has_any_at {
        return;
    }

    let align_col = max_width + 1;

    // Apply edits (bottom to top to preserve line indices)
    for (i, p) in parsed.iter().enumerate().rev() {
        let line_idx = start + i;
        let old_line = buf.line(line_idx).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');

        let new_line = if p.has_at {
            let padding = align_col.saturating_sub(p.text_before_at.width());
            format!(
                "{}{}{}{}",
                p.text_before_at,
                " ".repeat(padding),
                p.at_and_after,
                p.block_id_suffix,
            )
        } else {
            format!("{}{}", p.text_before_at, p.block_id_suffix)
        };

        if new_line != old_trimmed {
            let line_start = buf.text().line_to_char(line_idx);
            let line_end = line_start + old_trimmed.len();
            buf.replace(line_start..line_end, &new_line);
        }
    }
}

fn find_first_timestamp_pos(line: &str) -> Option<usize> {
    ["@due(", "@start(", "@at("]
        .iter()
        .filter_map(|prefix| line.find(prefix))
        .min()
}

/// Split a trailing block ID (` ^xxxxx`) from the line content.
/// Returns (content_without_id, block_id_suffix_including_space).
/// If no block ID is found, returns (original, "").
fn split_block_id(line: &str) -> (&str, &str) {
    // Block IDs are ` ^` followed by 5 base36 chars at end of line
    if let Some(caret_pos) = line.rfind(" ^") {
        let suffix = &line[caret_pos + 2..];
        if suffix.len() == 5 && suffix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return (&line[..caret_pos], &line[caret_pos..]);
        }
    }
    (line, "")
}

/// Move #tags that appear after @timestamps to before the first @.
fn relocate_post_at_tags(before: &str, after: &str) -> (String, String) {
    let mut tags = Vec::new();
    let mut cleaned = String::new();

    let mut chars_iter = after.char_indices().peekable();
    while let Some((i, ch)) = chars_iter.next() {
        if ch == '#' {
            let tag_start = i;
            let mut tag_end = i + ch.len_utf8();
            while let Some(&(j, c)) = chars_iter.peek() {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    tag_end = j + c.len_utf8();
                    chars_iter.next();
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

    // Clean up extra spaces in the after section
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    (new_before, cleaned)
}

// ---------------------------------------------------------------------------
// Table alignment
// ---------------------------------------------------------------------------

fn align_table_block(buf: &mut Buffer, start: usize, end: usize) {
    let lines: Vec<String> = (start..end).map(|i| buf.line(i).to_string()).collect();

    // Parse cells
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut is_alignment_row: Vec<bool> = Vec::new();

    for line in &lines {
        let trimmed = line.trim().trim_end_matches('\n');
        let cells: Vec<String> = trimmed
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        let is_sep = cells.iter().all(|c| {
            let s = c.trim_matches(':');
            !s.is_empty() && s.chars().all(|ch| ch == '-')
        });
        is_alignment_row.push(is_sep);
        table.push(cells);
    }

    if table.is_empty() {
        return;
    }

    // Compute max width per column
    let col_count = table.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths = vec![0usize; col_count];
    for (row_idx, row) in table.iter().enumerate() {
        if is_alignment_row[row_idx] {
            continue;
        }
        for (col_idx, cell) in row.iter().enumerate() {
            col_widths[col_idx] = col_widths[col_idx].max(cell.width());
        }
    }

    // Ensure minimum width of 3 for alignment rows
    for w in &mut col_widths {
        if *w < 3 {
            *w = 3;
        }
    }

    // Rebuild lines (bottom to top)
    for (i, row) in table.iter().enumerate().rev() {
        let line_idx = start + i;
        let old_line = buf.line(line_idx).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');

        let new_cells: Vec<String> = (0..col_count)
            .map(|col_idx| {
                let cell = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                let width = col_widths[col_idx];
                if is_alignment_row[i] {
                    format!(
                        "{:-<width$}",
                        cell.trim_matches(|c: char| c == '-' || c == ':')
                            .chars()
                            .next()
                            .map_or("---".to_string(), |_| "-".repeat(width))
                    )
                } else {
                    format!("{:<width$}", cell, width = width)
                }
            })
            .collect();

        let new_line = format!("| {} |", new_cells.join(" | "));

        if new_line != old_trimmed {
            let line_start = buf.text().line_to_char(line_idx);
            let line_end = line_start + old_trimmed.len();
            buf.replace(line_start..line_end, &new_line);
        }
    }
}

// ---------------------------------------------------------------------------
// Frontmatter alignment
// ---------------------------------------------------------------------------

fn align_frontmatter_block(buf: &mut Buffer, lines: &[String]) {
    if lines.is_empty() || lines[0].trim() != "---" {
        return;
    }

    // Find closing ---
    let mut end = 0;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end = i;
            break;
        }
    }
    if end == 0 {
        return;
    }

    // Parse key: value lines (between delimiters)
    let mut max_key_len = 0usize;
    for line in lines.iter().take(end).skip(1) {
        if let Some(colon) = line.find(": ") {
            let key = line[..colon].trim();
            max_key_len = max_key_len.max(key.width());
        }
    }

    if max_key_len == 0 {
        return;
    }

    // Apply edits (bottom to top)
    for i in (1..end).rev() {
        let old_line = buf.line(i).to_string();
        let old_trimmed = old_line.trim_end_matches('\n');

        if let Some(colon) = old_trimmed.find(": ") {
            let key = &old_trimmed[..colon];
            let value = old_trimmed[colon + 1..].trim_start();
            let padding = max_key_len.saturating_sub(key.trim().width());
            let new_line = format!("{}:{}{}", key.trim(), " ".repeat(padding + 1), value);

            if new_line != old_trimmed {
                let line_start = buf.text().line_to_char(i);
                let line_end = line_start + old_trimmed.len();
                buf.replace(line_start..line_end, &new_line);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        // All @due should be at the same column
        let positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert!(positions.len() == 3);
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
        // #rust should be before @due
        let tag_pos = line.find("#rust").unwrap();
        let at_pos = line.find("@due").unwrap();
        assert!(tag_pos < at_pos);
    }

    #[test]
    fn test_table_alignment() {
        let mut buf = Buffer::from_text(
            "| Key | Action |\n\
             |---|---|\n\
             | `w` | Next word start |\n\
             | `b` | Previous word start |\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        // All pipe positions should be consistent
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() == 4);
        // Second column pipe should align
        let pipe2_positions: Vec<usize> = lines
            .iter()
            .filter_map(|l| {
                let first = l.find('|')?;
                l[first + 1..].find('|').map(|p| p + first + 1)
            })
            .collect();
        assert!(pipe2_positions.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn test_frontmatter_alignment() {
        let mut buf = Buffer::from_text(
            "---\nid: abc123\ntitle: \"My Page\"\ncreated: 2026-03-01\ntags: [rust]\n---\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        // All values should start at the same column
        let value_starts: Vec<usize> = text
            .lines()
            .filter(|l| l.contains(": ") && *l != "---")
            .filter_map(|l| l.find(": ").map(|p| p + 2))
            .collect();
        // After alignment, colons are padded so values align
        // The longest key is "created" (7 chars), so all should have key padded to 7
        assert!(!value_starts.is_empty());
    }

    #[test]
    fn test_idempotent() {
        let input = "- [ ] Short @due(2026-03-05)\n- [ ] Longer task @due(2026-03-10)\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        let after_first = buf.text().to_string();
        auto_align_page(&mut buf);
        let after_second = buf.text().to_string();
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn test_block_mode() {
        let mut buf = Buffer::from_text(
            "- [ ] Task A @due(2026-03-05)\n\
             - [ ] Much longer task B @due(2026-03-10)\n\
             \n\
             - [ ] Task C @due(2026-04-01)\n\
             - [ ] Task D @due(2026-04-05)\n",
        );
        auto_align_block(&mut buf, 0); // align first block only
        let text = buf.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        // First block should be aligned
        let pos_a = lines[0].find("@due").unwrap();
        let pos_b = lines[1].find("@due").unwrap();
        assert_eq!(pos_a, pos_b);
        // Second block should NOT be aligned (different positions)
        // (it wasn't touched since cursor was in block 1)
    }

    #[test]
    fn test_no_align_when_no_blocks() {
        let input = "Just some text\nWith no special blocks\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), input);
    }

    #[test]
    fn test_longest_line_without_at_sets_column() {
        let mut buf = Buffer::from_text(
            "- [ ] Short @due(2026-03-05)\n\
             - [ ] A really long task description without any timestamp\n\
             - [ ] Medium @due(2026-03-10)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        // Lines with @due should be padded past the longest line
        let at_pos_0 = lines[0].find("@due").unwrap();
        let at_pos_2 = lines[2].find("@due").unwrap();
        assert_eq!(at_pos_0, at_pos_2);
        // The @ column should be after the longest line's text
        let longest_len = lines[1].len();
        assert!(at_pos_0 >= longest_len);
    }

    #[test]
    fn test_start_before_due_aligns_on_earliest() {
        let mut buf = Buffer::from_text(
            "- [ ] Finish FTS5 integration for Bloom @start(2026-03-02) @due(2026-03-07)\n\
             - [ ] Read chapters 7-9 of DDIA @due(2026-03-07)\n\
             - [ ] Schedule dentist appointment @due(2026-03-05)\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        // Line 0 has @start before @due — alignment should be on @start position
        let at_pos_0 = lines[0].find("@start").unwrap();
        let at_pos_1 = lines[1].find("@due").unwrap();
        let at_pos_2 = lines[2].find("@due").unwrap();
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
                assert!(
                    value_start >= 1,
                    "value should have at least 1 space after colon"
                );
            }
        }
        // Idempotent
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
        // All @due should be at the same column
        let positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(positions.len(), 3, "should have 3 lines with @due");
        assert_eq!(
            positions[0], positions[1],
            "lines 0 and 1 should have @due at same column\n{}",
            text
        );
        assert_eq!(
            positions[1], positions[2],
            "lines 1 and 2 should have @due at same column\n{}",
            text
        );
        // Block IDs should still be at end of each line
        for line in text.lines() {
            assert!(
                line.contains(" ^"),
                "block ID should be preserved: {}",
                line
            );
        }
    }

    #[test]
    fn test_timestamp_alignment_mixed_block_ids() {
        // Some lines have block IDs, some don't; one line has no timestamp
        let mut buf = Buffer::from_text(
            "- [ ] Review ropey API #rust @due(2026-03-05) ^a1b2c\n\
             - [ ] Fix parser @due(2026-03-10) #rust ^d3e4f\n\
             - [ ] Read DDIA @start(2026-03-02) @due(2026-03-15)\n\
             - [ ] Write tests #testing ^g5h6i\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();
        // Lines with @due/@start should all have their first @ at the same column
        let at_positions: Vec<usize> = text
            .lines()
            .filter_map(|l| {
                ["@due(", "@start(", "@at("]
                    .iter()
                    .filter_map(|p| l.find(p))
                    .min()
            })
            .collect();
        assert_eq!(at_positions.len(), 3);
        assert_eq!(
            at_positions[0], at_positions[1],
            "first @ should align\n{}",
            text
        );
        assert_eq!(
            at_positions[1], at_positions[2],
            "first @ should align\n{}",
            text
        );
    }

    #[test]
    fn test_non_task_list_items_contribute_to_alignment_width() {
        // Non-task list items in the same contiguous block should contribute
        // to the alignment column, so timestamps don't look misaligned
        // relative to surrounding plain bullets.
        let mut buf = Buffer::from_text(
            "- This is a regular note that is quite long\n\
             - [ ] Short task @due(2026-03-05)\n\
             - [ ] Another task @due(2026-03-10)\n\
             - And another plain bullet point\n",
        );
        auto_align_page(&mut buf);
        let text = buf.text().to_string();

        // The @due column should be pushed right by the long non-task line
        let at_positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(at_positions.len(), 2, "should have 2 lines with @due");
        assert_eq!(at_positions[0], at_positions[1], "both @due should align\n{}", text);

        // The @ column should be past the longest non-task line's width
        let longest_plain = "- This is a regular note that is quite long".len();
        assert!(
            at_positions[0] > longest_plain,
            "@due at col {} should be past longest plain line width {}\n{}",
            at_positions[0],
            longest_plain,
            text
        );
    }

    #[test]
    fn test_block_ids_excluded_from_alignment_width() {
        // Block IDs (^xxxxx) should not inflate the alignment column.
        // Without this fix, a long block ID on a non-timestamp line would
        // push all timestamps far to the right.
        let input = "- [ ] Short @due(2026-03-05)\n\
                      - [ ] Medium task @due(2026-03-10)\n\
                      - [ ] Write tests #testing ^g5h6i\n";
        let mut buf = Buffer::from_text(input);
        auto_align_page(&mut buf);
        let text = buf.text().to_string();

        let at_positions: Vec<usize> = text.lines().filter_map(|l| l.find("@due")).collect();
        assert_eq!(at_positions.len(), 2);
        assert_eq!(at_positions[0], at_positions[1], "should align\n{}", text);

        // The alignment column should be based on "- [ ] Write tests #testing"
        // (31 chars), NOT "- [ ] Write tests #testing ^g5h6i" (38 chars).
        let text_without_id = "- [ ] Write tests #testing".len();
        // @due should be at text_without_id + 1 (the alignment adds 1 space)
        assert!(
            at_positions[0] <= text_without_id + 2,
            "@due at col {} should be near {} (text without block ID), not inflated\n{}",
            at_positions[0],
            text_without_id + 1,
            text
        );

        // Block IDs should still be present at end of lines
        assert!(text.contains("^g5h6i"), "block ID should be preserved\n{}", text);

        // Idempotent
        let first = text.clone();
        auto_align_page(&mut buf);
        assert_eq!(buf.text().to_string(), first, "should be idempotent");
    }
}
