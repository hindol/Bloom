use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

use crate::coordinates::{RawCharIdx, RawCharRange, VisibleCharIdx, VisiblePosition, VisibleRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockIdMarker {
    Block,
    Mirror,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockIdToken {
    raw_range: RawCharRange,
    visible_boundary: VisibleCharIdx,
    starts_line: bool,
    id: String,
    marker: BlockIdMarker,
}

impl BlockIdToken {
    pub const fn raw_range(&self) -> RawCharRange {
        self.raw_range
    }

    pub const fn visible_boundary(&self) -> VisibleCharIdx {
        self.visible_boundary
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn marker(&self) -> BlockIdMarker {
        self.marker
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenDisposition {
    Preserve,
    Move,
    Retire,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenEffect {
    pub token: BlockIdToken,
    pub disposition: TokenDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweredEdit {
    pub raw_segments: Vec<RawCharRange>,
    pub insertion: Option<RawCharIdx>,
    pub token_effects: Vec<TokenEffect>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    InvalidVisibleRange {
        start: usize,
        end: usize,
        length: usize,
    },
    SplitsGrapheme {
        offset: usize,
    },
    ForeignVisiblePosition,
    InsertionAtLeadingToken {
        token_index: usize,
    },
    ForeignVisibleRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Segment {
    visible_start: usize,
    visible_end: usize,
    raw: RawCharRange,
}

#[derive(Clone, Debug)]
pub struct VisibleProjection {
    text: String,
    tokens: Vec<BlockIdToken>,
    segments: Vec<Segment>,
    raw_len: usize,
    identity: Arc<()>,
    grapheme_boundaries: Vec<usize>,
}

impl VisibleProjection {
    pub(crate) fn parse(raw_text: &str) -> Self {
        let raw_chars = raw_text.chars().collect::<Vec<_>>();
        let mut tokens = parse_block_id_tokens(raw_text);
        let mut visible = String::with_capacity(raw_text.len());
        let mut segments = Vec::new();
        let mut raw_cursor = 0;
        let mut visible_cursor = 0;
        let identity = Arc::new(());

        for token in &mut tokens {
            let raw_start = token.raw_range.start().get();
            let raw_end = token.raw_range.end().get();
            if raw_cursor < raw_start {
                let segment_len = raw_start - raw_cursor;
                let visible_start = VisibleCharIdx::new(visible_cursor);
                visible.extend(raw_chars[raw_cursor..raw_start].iter().copied());
                visible_cursor += segment_len;
                segments.push(Segment {
                    visible_start: visible_start.get(),
                    visible_end: visible_cursor,
                    raw: RawCharRange::new(RawCharIdx::new(raw_cursor), RawCharIdx::new(raw_start)),
                });
            }
            token.visible_boundary = VisibleCharIdx::new(visible_cursor);
            raw_cursor = raw_end;
        }

        if raw_cursor < raw_chars.len() {
            let visible_start = VisibleCharIdx::new(visible_cursor);
            visible.extend(raw_chars[raw_cursor..].iter().copied());
            visible_cursor += raw_chars.len() - raw_cursor;
            segments.push(Segment {
                visible_start: visible_start.get(),
                visible_end: visible_cursor,
                raw: RawCharRange::new(
                    RawCharIdx::new(raw_cursor),
                    RawCharIdx::new(raw_chars.len()),
                ),
            });
        }

        let mut grapheme_boundaries = vec![0];
        let mut char_offset = 0;
        for grapheme in visible.graphemes(true) {
            char_offset += grapheme.chars().count();
            grapheme_boundaries.push(char_offset);
        }

        Self {
            text: visible,
            tokens,
            segments,
            raw_len: raw_chars.len(),
            identity,
            grapheme_boundaries,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tokens(&self) -> &[BlockIdToken] {
        &self.tokens
    }

    pub fn range(&self, start: usize, end: usize) -> Result<VisibleRange, ProjectionError> {
        let length = self.text.chars().count();
        if start > end || end > length {
            return Err(ProjectionError::InvalidVisibleRange { start, end, length });
        }
        if self.grapheme_boundaries.binary_search(&start).is_err() {
            return Err(ProjectionError::SplitsGrapheme { offset: start });
        }
        if self.grapheme_boundaries.binary_search(&end).is_err() {
            return Err(ProjectionError::SplitsGrapheme { offset: end });
        }
        Ok(VisibleRange::new(
            VisibleCharIdx::new(start),
            VisibleCharIdx::new(end),
            Arc::clone(&self.identity),
        ))
    }

    pub fn grapheme_count(&self) -> usize {
        self.grapheme_boundaries.len() - 1
    }

    pub fn grapheme_range(
        &self,
        start: usize,
        end: usize,
    ) -> Result<VisibleRange, ProjectionError> {
        if start > end || end > self.grapheme_count() {
            return Err(ProjectionError::InvalidVisibleRange {
                start,
                end,
                length: self.grapheme_count(),
            });
        }
        self.range(
            self.grapheme_boundaries[start],
            self.grapheme_boundaries[end],
        )
    }

    pub fn position(&self, offset: usize) -> Result<VisiblePosition, ProjectionError> {
        let length = self.text.chars().count();
        if offset > length {
            return Err(ProjectionError::InvalidVisibleRange {
                start: offset,
                end: offset,
                length,
            });
        }
        if self.grapheme_boundaries.binary_search(&offset).is_err() {
            return Err(ProjectionError::SplitsGrapheme { offset });
        }
        Ok(VisiblePosition::new(
            VisibleCharIdx::new(offset),
            Arc::clone(&self.identity),
        ))
    }

    pub fn grapheme_position(&self, offset: usize) -> Result<VisiblePosition, ProjectionError> {
        let char_offset = self.grapheme_boundaries.get(offset).copied().ok_or(
            ProjectionError::InvalidVisibleRange {
                start: offset,
                end: offset,
                length: self.grapheme_count(),
            },
        )?;
        self.position(char_offset)
    }

    pub fn lower_position(&self, position: VisiblePosition) -> Result<RawCharIdx, ProjectionError> {
        if !position.belongs_to(&self.identity) {
            return Err(ProjectionError::ForeignVisiblePosition);
        }
        let offset = position.offset().get();
        let lowered = self.lower(self.range(offset, offset)?)?;
        lowered
            .insertion
            .ok_or(ProjectionError::InvalidVisibleRange {
                start: offset,
                end: offset,
                length: self.text.chars().count(),
            })
    }

    /// Lowers visible text to non-overlapping raw spans. Hidden tokens are
    /// never included in those spans; every token at or within the selection
    /// is returned with an explicit default-preserve effect.
    pub fn lower(&self, range: VisibleRange) -> Result<LoweredEdit, ProjectionError> {
        if !range.belongs_to(&self.identity) {
            return Err(ProjectionError::ForeignVisibleRange);
        }
        let start = range.start().get();
        let end = range.end().get();
        let length = self.text.chars().count();
        if start > end || end > length {
            return Err(ProjectionError::InvalidVisibleRange { start, end, length });
        }

        let token_effects = self
            .tokens
            .iter()
            .filter(|token| {
                let boundary = token.visible_boundary.get();
                start <= boundary && boundary <= end
            })
            .cloned()
            .map(|token| TokenEffect {
                token,
                disposition: TokenDisposition::Preserve,
            })
            .collect::<Vec<_>>();

        if start == end {
            if let Some((token_index, _)) = self
                .tokens
                .iter()
                .enumerate()
                .find(|(_, token)| token.visible_boundary.get() == start && token.starts_line)
            {
                return Err(ProjectionError::InsertionAtLeadingToken { token_index });
            }
            let insertion = if let Some(left) = self
                .segments
                .iter()
                .find(|segment| segment.visible_end == start)
            {
                left.raw.end()
            } else if let Some(segment) = self
                .segments
                .iter()
                .find(|segment| segment.visible_start < start && start < segment.visible_end)
            {
                RawCharIdx::new(segment.raw.start().get() + (start - segment.visible_start))
            } else if let Some(right) = self
                .segments
                .iter()
                .find(|segment| segment.visible_start == start)
            {
                right.raw.start()
            } else if let Some((token_index, _)) = self
                .tokens
                .iter()
                .enumerate()
                .find(|(_, token)| token.visible_boundary.get() == start)
            {
                return Err(ProjectionError::InsertionAtLeadingToken { token_index });
            } else if self.segments.is_empty() && self.tokens.is_empty() {
                RawCharIdx::new(0)
            } else if start == length {
                RawCharIdx::new(self.raw_len)
            } else {
                return Err(ProjectionError::InvalidVisibleRange { start, end, length });
            };

            return Ok(LoweredEdit {
                raw_segments: Vec::new(),
                insertion: Some(insertion),
                token_effects,
            });
        }

        let mut raw_segments = Vec::new();
        for segment in &self.segments {
            let segment_start = segment.visible_start;
            let segment_end = segment.visible_end;
            let overlap_start = start.max(segment_start);
            let overlap_end = end.min(segment_end);
            if overlap_start < overlap_end {
                let raw_start = segment.raw.start().get() + (overlap_start - segment_start);
                let raw_end = segment.raw.start().get() + (overlap_end - segment_start);
                raw_segments.push(RawCharRange::new(
                    RawCharIdx::new(raw_start),
                    RawCharIdx::new(raw_end),
                ));
            }
        }

        Ok(LoweredEdit {
            raw_segments,
            insertion: None,
            token_effects,
        })
    }
}

fn parse_block_id_tokens(raw_text: &str) -> Vec<BlockIdToken> {
    let raw_lines = raw_text.split_inclusive('\n').collect::<Vec<_>>();
    let lines = raw_lines
        .iter()
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let body_start = frontmatter_body_start(&lines);
    let mut in_code_fence: Option<CodeFenceDelimiter> = None;
    let mut inline_code_context = InlineCodeContext::default();
    let mut raw_line_start = 0;
    let mut code_spans = Vec::new();
    let mut tokens = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let raw_line_len =
            line.chars().count() + usize::from(raw_lines[line_index].ends_with('\n'));

        if line_index >= body_start {
            let trimmed = line.trim_start();
            let delimiter = code_fence_delimiter(trimmed);
            if let Some(open) = in_code_fence {
                inline_code_context.finish_block(&mut code_spans);
                inline_code_context.reset();
                if delimiter.is_some_and(|close| {
                    close.marker == open.marker
                        && close.length >= open.length
                        && trimmed[close.length..].trim().is_empty()
                }) {
                    in_code_fence = None;
                }
            } else if let Some(open) = delimiter {
                inline_code_context.finish_block(&mut code_spans);
                inline_code_context.reset();
                in_code_fence = Some(open);
            } else {
                let end_inline_block = inline_code_context.prepare_line(line, &mut code_spans);
                inline_code_context.scan(line, raw_line_start);
                if let Some((byte_start, byte_end, id, marker)) = parse_trailing_id_token(line) {
                    let char_start = raw_line_start + line[..byte_start].chars().count();
                    let char_end = raw_line_start + line[..byte_end].chars().count();
                    tokens.push(BlockIdToken {
                        raw_range: RawCharRange::new(
                            RawCharIdx::new(char_start),
                            RawCharIdx::new(char_end),
                        ),
                        visible_boundary: VisibleCharIdx::new(0),
                        starts_line: byte_start == 0,
                        id,
                        marker,
                    });
                }
                if end_inline_block {
                    inline_code_context.finish_block(&mut code_spans);
                }
            }
        } else {
            inline_code_context.finish_block(&mut code_spans);
            inline_code_context.reset();
        }

        raw_line_start += raw_line_len;
    }

    inline_code_context.finish_block(&mut code_spans);
    tokens.retain(|token| {
        !code_spans.iter().any(|span| {
            span.start() <= token.raw_range.start() && token.raw_range.start() < span.end()
        })
    });
    tokens
}

#[derive(Clone, Copy)]
struct CodeFenceDelimiter {
    marker: u8,
    length: usize,
}

#[derive(Default)]
struct InlineCodeContext {
    runs: Vec<BacktickRun>,
    in_blockquote: bool,
}

struct BacktickRun {
    length: usize,
    raw_start: usize,
    raw_end: usize,
}

impl InlineCodeContext {
    fn reset(&mut self) {
        self.runs.clear();
        self.in_blockquote = false;
    }

    fn prepare_line(&mut self, line: &str, code_spans: &mut Vec<RawCharRange>) -> bool {
        let trimmed = line.trim();
        let is_blank = trimmed.is_empty();
        let is_structural_boundary = is_blank
            || is_atx_heading(line)
            || is_thematic_break(trimmed)
            || is_setext_underline(trimmed);
        let is_blockquote_start = line.trim_start().starts_with('>') && !self.in_blockquote;
        let is_list_item = list_item_content_indent(line).is_some();
        if is_structural_boundary || is_blockquote_start || is_list_item {
            self.finish_block(code_spans);
        }
        if is_structural_boundary {
            self.in_blockquote = false;
        }

        if line.trim_start().starts_with('>') {
            self.in_blockquote = true;
        }
        if is_list_item {
            self.in_blockquote = false;
        }
        is_structural_boundary
    }

    fn scan(&mut self, line: &str, raw_line_start: usize) {
        let bytes = line.as_bytes();
        let mut cursor = 0;
        while cursor < bytes.len() {
            if bytes[cursor] != b'`' || is_escaped(bytes, cursor) {
                cursor += 1;
                continue;
            }

            let run_start = cursor;
            while cursor < bytes.len() && bytes[cursor] == b'`' {
                cursor += 1;
            }
            let run_length = cursor - run_start;
            self.runs.push(BacktickRun {
                length: run_length,
                raw_start: raw_line_start + line[..run_start].chars().count(),
                raw_end: raw_line_start + line[..cursor].chars().count(),
            });
        }
    }

    fn finish_block(&mut self, code_spans: &mut Vec<RawCharRange>) {
        let mut next_by_length = std::collections::HashMap::new();
        let mut next_matches = vec![None; self.runs.len()];
        for run_index in (0..self.runs.len()).rev() {
            next_matches[run_index] = next_by_length.insert(self.runs[run_index].length, run_index);
        }

        let mut run_index = 0;
        while run_index < self.runs.len() {
            let run = &self.runs[run_index];
            if let Some(close_index) = next_matches[run_index] {
                code_spans.push(RawCharRange::new(
                    RawCharIdx::new(run.raw_start),
                    RawCharIdx::new(self.runs[close_index].raw_end),
                ));
                run_index = close_index + 1;
            } else {
                run_index += 1;
            }
        }
        self.runs.clear();
    }
}

fn code_fence_delimiter(line: &str) -> Option<CodeFenceDelimiter> {
    let marker = *line.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = line
        .as_bytes()
        .iter()
        .take_while(|character| **character == marker)
        .count();
    (length >= 3).then_some(CodeFenceDelimiter { marker, length })
}

fn is_atx_heading(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let heading = &line[indent..];
    let marker_count = heading.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&marker_count)
        && (heading.len() == marker_count || heading.as_bytes()[marker_count].is_ascii_whitespace())
}

fn is_thematic_break(line: &str) -> bool {
    let mut marker = None;
    let mut count = 0;
    for byte in line.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if !matches!(byte, b'*' | b'-' | b'_') || marker.is_some_and(|seen| seen != byte) {
            return false;
        }
        marker = Some(byte);
        count += 1;
    }
    count >= 3
}

fn is_setext_underline(line: &str) -> bool {
    let mut found_marker = false;
    for byte in line.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte != b'=' {
            return false;
        }
        found_marker = true;
    }
    found_marker
}

fn list_item_content_indent(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let item = &line[indent..];
    let bytes = item.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'+' | b'*') && bytes[1].is_ascii_whitespace()
    {
        return Some(indent + 2);
    }

    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits > 0
        && bytes
            .get(digits)
            .is_some_and(|byte| matches!(byte, b'.' | b')'))
        && bytes.get(digits + 1).is_some_and(u8::is_ascii_whitespace)
    {
        return Some(indent + digits + 2);
    }
    None
}

fn frontmatter_body_start(lines: &[&str]) -> usize {
    if lines.first().is_none_or(|line| line.trim() != "---") {
        return 0;
    }
    lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim() == "---")
        .map_or(0, |(line_index, _)| line_index + 1)
}

fn parse_trailing_id_token(line: &str) -> Option<(usize, usize, String, BlockIdMarker)> {
    let trimmed = line.trim_end();
    let caret = if let Some(space_pos) = trimmed.rfind(" ^") {
        Some(space_pos + 1)
    } else if trimmed.starts_with('^') {
        Some(0)
    } else {
        None
    };

    let caret = caret?;

    let (id, marker) = if let Some(id) = trimmed[caret + 1..].strip_prefix('=') {
        (id, BlockIdMarker::Mirror)
    } else {
        (&trimmed[caret + 1..], BlockIdMarker::Block)
    };
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_alphanumeric() || character == '-' || character == '_')
    {
        return None;
    }

    let token_start = if caret > 0 && line.as_bytes()[caret - 1] == b' ' {
        caret - 1
    } else {
        caret
    };
    Some((token_start, trimmed.len(), id.to_owned(), marker))
}

fn is_escaped(bytes: &[u8], offset: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = offset;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::{BlockIdMarker, ProjectionError, TokenDisposition, VisibleProjection};
    use crate::Doc;

    const ADR3_VIM_ORACLE: &str = include_str!("../../../../testdata/adr3_vim_oracle.v1");
    const ADR3_IDENTITY_ORACLE: &str =
        include_str!("../../../../testdata/adr3_vim_identity_oracle.v1");

    fn decode_text(text: &str) -> String {
        text.replace("\\n", "\n").replace("\\r", "\r")
    }

    fn identity_oracle_case(name: &str) -> Vec<&str> {
        ADR3_IDENTITY_ORACLE
            .lines()
            .find(|line| line.starts_with(&format!("{name}|")))
            .unwrap_or_else(|| panic!("missing identity oracle case {name}"))
            .split('|')
            .collect()
    }

    fn parse_edits(encoded: &str) -> Vec<(usize, usize, char, usize)> {
        encoded
            .split(';')
            .map(|edit| {
                let values = edit
                    .split(',')
                    .map(|value| value.parse::<usize>().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(values.len(), 4);
                (
                    values[0],
                    values[1],
                    char::from_u32(values[2] as u32).unwrap(),
                    values[3],
                )
            })
            .collect()
    }

    #[test]
    fn hides_legacy_ids_and_mirror_markers_without_changing_canonical_text() {
        let raw = "Heading ^my-block\n^=legacy_id\n";
        let projection = VisibleProjection::parse(raw);

        assert_eq!(projection.text(), "Heading\n\n");
        assert_eq!(projection.tokens().len(), 2);
        assert_eq!(projection.tokens()[0].id(), "my-block");
        assert_eq!(projection.tokens()[0].marker(), BlockIdMarker::Block);
        assert_eq!(projection.tokens()[1].id(), "legacy_id");
        assert_eq!(projection.tokens()[1].marker(), BlockIdMarker::Mirror);
        assert_eq!(raw, "Heading ^my-block\n^=legacy_id\n");
    }

    #[test]
    fn token_parser_respects_frontmatter_fences_and_inline_code() {
        let raw = concat!(
            "---\n",
            "title: ^frontmatter\n",
            "---\n",
            "Inline `^code` stays visible\n",
            "````rust\n",
            "sample ^fenced\n",
            "````\n",
            "Real block ^legacy-ID_2\n",
        );
        let projection = VisibleProjection::parse(raw);

        assert_eq!(projection.tokens().len(), 1);
        assert_eq!(projection.tokens()[0].id(), "legacy-ID_2");
        assert_eq!(
            projection.text(),
            concat!(
                "---\n",
                "title: ^frontmatter\n",
                "---\n",
                "Inline `^code` stays visible\n",
                "````rust\n",
                "sample ^fenced\n",
                "````\n",
                "Real block\n",
            )
        );
    }

    #[test]
    fn fenced_code_requires_a_matching_marker_and_sufficient_closing_length() {
        let raw_lines = [
            "prologue ^before",
            "````rust",
            "sample ^literal1",
            "```",
            "another ^literal2",
            "~~~~~~",
            "mirror ^=literal3",
            "````text",
            "next ^literal4",
            "`````",
            "normal ^block",
            "~~~lang",
            "```",
            "mixed ^literal5",
            "~~~not-close",
            "same ^literal6",
            "~~~~",
            "after ^last",
        ];
        let visible_lines = [
            "prologue",
            "````rust",
            "sample ^literal1",
            "```",
            "another ^literal2",
            "~~~~~~",
            "mirror ^=literal3",
            "````text",
            "next ^literal4",
            "`````",
            "normal",
            "~~~lang",
            "```",
            "mixed ^literal5",
            "~~~not-close",
            "same ^literal6",
            "~~~~",
            "after",
        ];

        for ending in ["\n", "\r\n"] {
            for trailing in ["", ending] {
                let raw = format!("{}{trailing}", raw_lines.join(ending));
                let expected = format!("{}{trailing}", visible_lines.join(ending));
                let projection = Doc::new(&raw).visible_projection();
                assert_eq!(
                    projection
                        .tokens()
                        .iter()
                        .map(|token| token.id())
                        .collect::<Vec<_>>(),
                    ["before", "block", "last"]
                );
                assert_eq!(projection.text(), expected);
            }
        }
    }

    #[test]
    fn unmatched_fence_keeps_ids_literal_through_end_of_file() {
        for ending in ["\n", "\r\n"] {
            for (open, lookalike) in [("````", "```"), ("~~~~", "~~~"), ("~~~", "```")] {
                let raw = format!("keep ^id{ending}{open}{ending}{lookalike}{ending}literal ^code");
                let projection = Doc::new(&raw).visible_projection();
                assert_eq!(projection.tokens().len(), 1);
                assert_eq!(projection.tokens()[0].id(), "id");
                assert_eq!(
                    projection.text(),
                    format!("keep{ending}{open}{ending}{lookalike}{ending}literal ^code")
                );
            }
        }
    }

    #[test]
    fn matching_fence_closes_with_leading_and_trailing_whitespace() {
        let raw = "  ```rust\r\nliteral ^code\r\n \t``` \t\r\noutside ^id\r\n";
        let projection = Doc::new(raw).visible_projection();

        assert_eq!(projection.tokens().len(), 1);
        assert_eq!(projection.tokens()[0].id(), "id");
        assert_eq!(
            projection.text(),
            "  ```rust\r\nliteral ^code\r\n \t``` \t\r\noutside\r\n"
        );
    }

    #[test]
    fn fenced_code_boundary_model_covers_marker_lengths_and_suffixes() {
        let mut cases = 0;
        for ending in ["\n", "\r\n"] {
            for opening_marker in ["`", "~"] {
                for opening_length in 3..=5 {
                    for closing_marker in ["`", "~"] {
                        for closing_length in 2..=6 {
                            for suffix in ["", " \t", "lang"] {
                                let opening = opening_marker.repeat(opening_length);
                                let candidate =
                                    format!("{}{suffix}", closing_marker.repeat(closing_length));
                                let closes = opening_marker == closing_marker
                                    && closing_length >= opening_length
                                    && suffix.trim().is_empty();
                                let raw = format!(
                                    "{opening}{ending}{candidate}{ending}middle ^inside{ending}{opening}{ending}outside ^outside"
                                );
                                let (middle, outside, expected_id) = if closes {
                                    ("middle", "outside ^outside", "inside")
                                } else {
                                    ("middle ^inside", "outside", "outside")
                                };
                                let expected = format!(
                                    "{opening}{ending}{candidate}{ending}{middle}{ending}{opening}{ending}{outside}"
                                );
                                let projection = Doc::new(&raw).visible_projection();
                                assert_eq!(projection.tokens().len(), 1, "{raw:?}");
                                assert_eq!(projection.tokens()[0].id(), expected_id, "{raw:?}");
                                assert_eq!(projection.text(), expected, "{raw:?}");
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 360);
    }

    #[test]
    fn malformed_markers_and_code_spans_are_not_hidden() {
        let raw = "Bad ^id/part\nBad ^\n`literal ^inside`\n";
        let projection = VisibleProjection::parse(raw);

        assert!(projection.tokens().is_empty());
        assert_eq!(projection.text(), raw);
    }

    #[test]
    fn multiline_inline_code_keeps_trailing_marker_literal() {
        let mut failures = Vec::new();
        for ending in ["\n", "\r\n"] {
            for delimiter in ["`", "``"] {
                let raw = format!(
                    "α {delimiter}start{ending}literal ^inside{ending}end{delimiter}{ending}"
                );
                let projection = Doc::new(&raw).visible_projection();
                if projection.text() != raw || !projection.tokens().is_empty() {
                    failures.push(format!(
                        "{ending:?}, {delimiter:?}: visible {:?}, IDs {:?}",
                        projection.text(),
                        projection
                            .tokens()
                            .iter()
                            .map(|token| token.id())
                            .collect::<Vec<_>>()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn trailing_marker_after_multiline_inline_code_is_hidden() {
        let mut failures = Vec::new();
        for ending in ["\n", "\r\n"] {
            for delimiter in ["`", "``"] {
                let raw = format!("α {delimiter}start{ending}end{delimiter} ^outside{ending}");
                let expected = format!("α {delimiter}start{ending}end{delimiter}{ending}");
                let projection = Doc::new(&raw).visible_projection();
                let ids = projection
                    .tokens()
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>();
                if projection.text() != expected || ids != ["outside"] {
                    failures.push(format!(
                        "{ending:?}, {delimiter:?}: visible {:?}, IDs {ids:?}",
                        projection.text()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn unmatched_inline_code_and_structural_boundaries_do_not_hide_ids() {
        for ending in ["\n", "\r\n"] {
            let raw = format!(
                "unmatched ` opener{ending}visible ^paragraph{ending}# Heading ^heading{ending}- item ^list"
            );
            let projection = Doc::new(&raw).visible_projection();
            assert_eq!(
                projection
                    .tokens()
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>(),
                ["paragraph", "heading", "list"]
            );
        }
    }

    #[test]
    fn multiline_inline_code_uses_exact_delimiter_runs_and_block_context() {
        for ending in ["\n", "\r\n"] {
            let raw = format!(
                "> start ``{ending}> literal ` ^inside{ending}> end ``{ending}> visible ^outside{ending}{ending}new ` opener{ending}- new block ^list"
            );
            let projection = Doc::new(&raw).visible_projection();
            assert_eq!(
                projection
                    .tokens()
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>(),
                ["outside", "list"]
            );
            assert!(projection.text().contains("^inside"));
            assert!(!projection.text().contains("^outside"));
        }
    }

    #[test]
    fn escaped_backticks_and_fences_do_not_carry_inline_code_context() {
        for ending in ["\n", "\r\n"] {
            let escaped = format!(r"escaped \` ^escaped{ending}visible ^plain{ending}");
            let projection = Doc::new(&escaped).visible_projection();
            assert_eq!(
                projection
                    .tokens()
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>(),
                ["escaped", "plain"]
            );
            assert_eq!(
                projection.text(),
                format!("escaped \\`{ending}visible{ending}")
            );

            let across_fence = format!(
                "` unmatched{ending}```{ending}literal ^fenced{ending}```{ending}outside ^id"
            );
            let projection = Doc::new(&across_fence).visible_projection();
            assert_eq!(
                projection
                    .tokens()
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>(),
                ["id"]
            );
        }
    }

    #[test]
    fn typed_span_fallback_cannot_recover_ambiguous_identity_from_markdown_alone() {
        struct TypedSpanSnapshot<'a> {
            canonical: &'a str,
            token_spans: Vec<std::ops::Range<usize>>,
        }

        impl TypedSpanSnapshot<'_> {
            fn visible(&self) -> String {
                let mut visible = String::new();
                let mut previous_end = 0;
                for span in &self.token_spans {
                    assert!(previous_end <= span.start && span.start <= span.end);
                    assert!(span.end <= self.canonical.len());
                    assert!(self.canonical.is_char_boundary(span.start));
                    assert!(self.canonical.is_char_boundary(span.end));
                    visible.push_str(&self.canonical[previous_end..span.start]);
                    previous_end = span.end;
                }
                visible.push_str(&self.canonical[previous_end..]);
                visible
            }
        }

        for ending in ["\n", "\r\n"] {
            let original = format!("α `βγ` ^split1{ending}");
            let original_projection = Doc::new(&original).visible_projection();
            assert_eq!(original_projection.text(), format!("α `βγ`{ending}"));
            assert_eq!(original_projection.tokens().len(), 1);
            assert_eq!(original_projection.tokens()[0].id(), "split1");

            let generated_canonical =
                original.replacen("βγ` ^split1", &format!("β ^split1{ending}γ` ^split2"), 1);
            let literal_canonical = format!("α `β ^split1{ending}γ` ^split2{ending}");
            let first = generated_canonical.find(" ^split1").unwrap();
            let second = generated_canonical.find(" ^split2").unwrap();
            let literal = TypedSpanSnapshot {
                canonical: &literal_canonical,
                token_spans: std::iter::once(second..second + " ^split2".len()).collect(),
            };
            let generated = TypedSpanSnapshot {
                canonical: &generated_canonical,
                token_spans: vec![
                    first..first + " ^split1".len(),
                    second..second + " ^split2".len(),
                ],
            };
            let expected_literal = format!("α `β ^split1{ending}γ`{ending}");
            let expected_generated = format!("α `β{ending}γ`{ending}");

            assert_eq!(literal.canonical, generated.canonical);
            assert_eq!(literal.visible(), expected_literal);
            assert_eq!(generated.visible(), expected_generated);
            assert_ne!(literal.visible(), generated.visible());

            let reloaded_literal = Doc::new(literal.canonical).visible_projection();
            let reloaded_generated = Doc::new(generated.canonical).visible_projection();
            assert_eq!(reloaded_generated.text(), reloaded_literal.text());
            assert!(
                reloaded_literal.text() != literal.visible()
                    || reloaded_generated.text() != generated.visible()
            );
        }
    }

    #[test]
    fn lowering_splits_visible_ranges_around_hidden_tokens() {
        let raw = "one ^first\ntwo ^=second\nthree\n";
        let projection = VisibleProjection::parse(raw);
        let visible = projection.text();
        let range = projection.range(1, visible.chars().count() - 1).unwrap();
        let lowered = projection.lower(range).unwrap();

        assert_eq!(lowered.raw_segments.len(), 3);
        assert_eq!(lowered.token_effects.len(), 2);
        assert!(lowered
            .token_effects
            .iter()
            .all(|effect| effect.disposition == TokenDisposition::Preserve));
        for segment in &lowered.raw_segments {
            assert!(projection.tokens().iter().all(|token| {
                segment.end() <= token.raw_range().start()
                    || token.raw_range().end() <= segment.start()
            }));
        }
    }

    #[test]
    fn all_visible_ranges_round_trip_to_their_raw_segments() {
        let raw = "α ^one\nβ ^=two\nend";
        let projection = VisibleProjection::parse(raw);
        let visible = projection.text().chars().collect::<Vec<_>>();
        let raw_chars = raw.chars().collect::<Vec<_>>();

        for start in 0..=visible.len() {
            for end in start..=visible.len() {
                let range = projection.range(start, end).unwrap();
                let lowered = projection.lower(range).unwrap();
                if start == end {
                    continue;
                }
                let reconstructed = lowered
                    .raw_segments
                    .iter()
                    .flat_map(|segment| {
                        raw_chars[segment.start().get()..segment.end().get()]
                            .iter()
                            .copied()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(reconstructed, visible[start..end], "range {start}..{end}");
            }
        }
    }

    #[test]
    fn insertion_before_trailing_token_is_explicitly_preserving() {
        let projection = VisibleProjection::parse("Text ^id");
        let range = projection.range(4, 4).unwrap();
        let lowered = projection.lower(range).unwrap();

        assert!(lowered.raw_segments.is_empty());
        assert_eq!(lowered.insertion.unwrap().get(), 4);
        assert_eq!(lowered.token_effects.len(), 1);
        assert_eq!(
            lowered.token_effects[0].disposition,
            TokenDisposition::Preserve
        );
    }

    #[test]
    fn token_boundary_deletions_only_remove_visible_characters() {
        let raw = "hello ^id";
        let projection = VisibleProjection::parse(raw);
        let raw_chars = raw.chars().collect::<Vec<_>>();

        for (operation, start, end) in [("x", 4, 5), ("X", 4, 5), ("D", 1, 5)] {
            let range = projection.range(start, end).unwrap();
            let lowered = projection.lower(range).unwrap();
            let mut edited = raw_chars.clone();
            for segment in lowered.raw_segments.iter().rev() {
                edited.drain(segment.start().get()..segment.end().get());
            }

            let edited = edited.into_iter().collect::<String>();
            assert!(edited.ends_with("^id"), "{operation} removed the token");
            assert!(lowered
                .token_effects
                .iter()
                .all(|effect| effect.disposition == TokenDisposition::Preserve));
        }
    }

    #[test]
    fn serialized_legacy_boundary_edits_lower_without_touching_block_ids() {
        assert_eq!(
            ADR3_IDENTITY_ORACLE.lines().next(),
            Some("BLOOM_VIM_IDENTITY_ORACLE_V1")
        );

        for name in ["x", "X", "D"] {
            let fields = identity_oracle_case(name);
            assert_eq!(fields.len(), 7);
            let raw = decode_text(fields[1]);
            let visible = decode_text(fields[2]);
            let expected_visible = decode_text(fields[5]);
            let projection = Doc::new(&raw).visible_projection();
            assert_eq!(projection.text(), visible, "{name} fixture projection");
            let token_ids = projection
                .tokens()
                .iter()
                .map(|token| token.id().to_owned())
                .collect::<Vec<_>>();
            let edits = fields[6].split(';').collect::<Vec<_>>();
            assert_eq!(edits.len(), 1, "{name} should be one visible edit");
            let edit = edits[0].splitn(4, ',').collect::<Vec<_>>();
            assert_eq!(edit.len(), 4);
            let start = edit[0].parse::<usize>().unwrap();
            let end = edit[1].parse::<usize>().unwrap();
            let replacement = decode_text(edit[2]);
            assert!(replacement.is_empty(), "{name} oracle is a deletion");

            let lowered = projection
                .lower(projection.range(start, end).unwrap())
                .unwrap();
            assert!(lowered
                .token_effects
                .iter()
                .all(|effect| effect.disposition == TokenDisposition::Preserve));
            let mut raw_chars = raw.chars().collect::<Vec<_>>();
            for segment in lowered.raw_segments.iter().rev() {
                raw_chars.drain(segment.start().get()..segment.end().get());
            }
            let edited = raw_chars.into_iter().collect::<String>();
            let edited_projection = Doc::new(&edited).visible_projection();

            assert_eq!(edited_projection.text(), expected_visible, "{name}");
            assert_eq!(
                edited_projection
                    .tokens()
                    .iter()
                    .map(|token| token.id().to_owned())
                    .collect::<Vec<_>>(),
                token_ids,
                "{name} changed canonical token identity"
            );
        }
    }

    #[test]
    fn insertion_at_an_id_only_line_requires_an_explicit_structural_decision() {
        let projection = VisibleProjection::parse("^id\n");
        let range = projection.range(0, 0).unwrap();

        assert_eq!(
            projection.lower(range),
            Err(ProjectionError::InsertionAtLeadingToken { token_index: 0 })
        );
    }

    #[test]
    fn invalid_ranges_are_rejected_before_lowering() {
        let projection = VisibleProjection::parse("visible ^id");

        assert_eq!(
            projection.range(5, 3),
            Err(ProjectionError::InvalidVisibleRange {
                start: 5,
                end: 3,
                length: 7,
            })
        );
        assert!(matches!(
            projection.range(0, 8),
            Err(ProjectionError::InvalidVisibleRange { .. })
        ));
    }

    #[test]
    fn ranges_cannot_be_lowered_against_a_different_projection_snapshot() {
        let first = VisibleProjection::parse("one ^a");
        let second = VisibleProjection::parse("two ^b");
        let range = first.range(0, 1).unwrap();

        assert_eq!(
            second.lower(range),
            Err(ProjectionError::ForeignVisibleRange)
        );
    }

    #[test]
    fn line_endings_unicode_and_trailing_spaces_survive_projection() {
        let raw = "é ^id  \r\nlast ^=other";
        let projection = VisibleProjection::parse(raw);

        assert_eq!(projection.text(), "é  \r\nlast");
        assert_eq!(projection.tokens().len(), 2);
        assert_eq!(
            projection
                .tokens()
                .iter()
                .map(|token| token.id())
                .collect::<Vec<_>>(),
            ["id", "other"]
        );
    }

    #[test]
    fn visible_ranges_cannot_split_a_unicode_grapheme() {
        let projection = VisibleProjection::parse("e\u{301}🙂 ^id");

        assert_eq!(projection.text(), "e\u{301}🙂");
        assert_eq!(projection.grapheme_count(), 2);
        assert_eq!(
            projection.range(1, 2),
            Err(ProjectionError::SplitsGrapheme { offset: 1 })
        );
        let first = projection.grapheme_range(0, 1).unwrap();
        let lowered = projection.lower(first).unwrap();
        assert_eq!(lowered.raw_segments.len(), 1);
        assert_eq!(lowered.raw_segments[0].start().get(), 0);
        assert_eq!(lowered.raw_segments[0].end().get(), 2);
    }

    #[test]
    fn projected_cursor_ranges_reject_every_interior_grapheme_offset() {
        let projection = VisibleProjection::parse("e\u{301}👩\u{200d}💻 ^id");
        assert_eq!(projection.text(), "e\u{301}👩\u{200d}💻");
        assert_eq!(projection.tokens().len(), 1);
        assert_eq!(projection.tokens()[0].id(), "id");

        for offset in 0..=5 {
            if [0, 2, 5].contains(&offset) {
                assert!(projection.range(offset, offset).is_ok(), "{offset}");
            } else {
                assert_eq!(
                    projection.range(offset, offset),
                    Err(ProjectionError::SplitsGrapheme { offset })
                );
            }
        }
    }

    #[test]
    fn projected_positions_lower_only_to_raw_grapheme_and_token_boundaries() {
        for ending in ["\n", "\r\n"] {
            let raw = format!("e\u{301}👩\u{200d}💻 ^id{ending}^only{ending}");
            let document = Doc::new(&raw);
            let projection = document.visible_projection();
            let snapshot = document.snapshot();
            assert_eq!(
                projection.text(),
                format!("e\u{301}👩\u{200d}💻{ending}{ending}")
            );

            for grapheme_offset in 0..=projection.grapheme_count() {
                let position = projection.grapheme_position(grapheme_offset).unwrap();
                match projection.lower_position(position) {
                    Ok(raw_position) => {
                        assert!(
                            snapshot.is_grapheme_boundary(raw_position).unwrap(),
                            "raw offset {} is not a grapheme boundary ({ending:?})",
                            raw_position.get()
                        );
                        assert!(projection.tokens().iter().all(|token| {
                            let offset = raw_position.get();
                            offset <= token.raw_range().start().get()
                                || offset >= token.raw_range().end().get()
                        }));
                    }
                    Err(ProjectionError::InsertionAtLeadingToken { token_index }) => {
                        assert_eq!(token_index, 1);
                        assert_eq!(grapheme_offset, 3);
                    }
                    Err(error) => panic!("unexpected position lowering error: {error:?}"),
                }
            }
        }
    }

    #[test]
    fn projected_positions_reject_interior_graphemes_and_foreign_snapshots() {
        let first = VisibleProjection::parse("e\u{301} ^id");
        let second = VisibleProjection::parse("e\u{301} ^id");

        assert_eq!(
            first.position(1),
            Err(ProjectionError::SplitsGrapheme { offset: 1 })
        );
        assert_eq!(
            first.grapheme_position(2),
            Err(ProjectionError::InvalidVisibleRange {
                start: 2,
                end: 2,
                length: 1,
            })
        );
        assert_eq!(
            second.lower_position(first.position(0).unwrap()),
            Err(ProjectionError::ForeignVisiblePosition)
        );
    }

    #[test]
    fn serialized_legacy_vim_edits_lower_without_touching_block_or_mirror_ids() {
        assert_eq!(ADR3_VIM_ORACLE.lines().next(), Some("BLOOM_VIM_ORACLE_V1"));
        for line in ADR3_VIM_ORACLE.lines().skip(1) {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 6);
            let raw = decode_text(fields[1]);
            let visible_before = decode_text(fields[2]);
            let expected_raw_after = decode_text(fields[3]);
            let expected_visible_after = decode_text(fields[4]);
            let mut document = Doc::new(&raw);
            assert_eq!(document.visible_projection().text(), visible_before);

            for (start, end, character, cursor_after) in parse_edits(fields[5]) {
                assert_eq!(start, end, "append oracle should emit insertion edits only");
                let projection = document.visible_projection();
                let range = projection.range(start, end).unwrap();
                let lowered = projection.lower(range).unwrap();
                assert!(lowered.raw_segments.is_empty());
                assert_eq!(lowered.token_effects.len(), 1);
                assert_eq!(
                    lowered.token_effects[0].disposition,
                    TokenDisposition::Preserve
                );
                let insertion = lowered.insertion.unwrap();
                document
                    .insert(
                        document.raw_char_idx(insertion.get()).unwrap(),
                        &character.to_string(),
                    )
                    .unwrap();
                let updated_projection = document.visible_projection();
                assert_eq!(
                    cursor_after + 1,
                    updated_projection.text().chars().count(),
                    "legacy Vim cursor should remain before the final newline"
                );
            }

            assert_eq!(document.text(), expected_raw_after);
            assert_eq!(document.visible_projection().text(), expected_visible_after);
            assert_eq!(
                document.visible_projection().tokens()[0].id(),
                fields[0]
                    .strip_prefix("append-")
                    .map(|kind| if kind == "block" { "blk01" } else { "mir01" })
                    .unwrap()
            );
        }
    }
}
