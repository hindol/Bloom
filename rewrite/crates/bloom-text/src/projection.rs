use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

use crate::coordinates::{RawCharIdx, RawCharRange, VisibleCharIdx, VisibleRange};

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
    let mut in_code_fence = false;
    let mut raw_line_start = 0;
    let mut tokens = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let raw_line_len =
            line.chars().count() + usize::from(raw_lines[line_index].ends_with('\n'));

        if line_index >= body_start {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code_fence = !in_code_fence;
            } else if !in_code_fence {
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
            }
        }

        raw_line_start += raw_line_len;
    }

    tokens
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
        space_pos + 1
    } else if trimmed.starts_with('^') {
        0
    } else {
        return None;
    };

    if is_inside_inline_code(line, caret) {
        return None;
    }

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

fn is_inside_inline_code(line: &str, byte_offset: usize) -> bool {
    let bytes = line.as_bytes();
    let mut open_run = None;
    let mut cursor = 0;

    while cursor < byte_offset {
        if bytes[cursor] != b'`' || is_escaped(bytes, cursor) {
            cursor += 1;
            continue;
        }

        let run_start = cursor;
        while cursor < byte_offset && bytes[cursor] == b'`' {
            cursor += 1;
        }
        let run_length = cursor - run_start;
        if open_run == Some(run_length) {
            open_run = None;
        } else if open_run.is_none() {
            open_run = Some(run_length);
        }
    }

    open_run.is_some()
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

    fn decode_text(text: &str) -> String {
        text.replace("\\n", "\n").replace("\\r", "\r")
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
    fn malformed_markers_and_code_spans_are_not_hidden() {
        let raw = "Bad ^id/part\nBad ^\n`literal ^inside`\n";
        let projection = VisibleProjection::parse(raw);

        assert!(projection.tokens().is_empty());
        assert_eq!(projection.text(), raw);
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
