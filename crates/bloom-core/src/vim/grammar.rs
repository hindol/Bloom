//! Vim grammar parser.
//!
//! Parses the pending key buffer into structured commands: optional count prefix,
//! operator, motion or text-object, mode switches, and standalone commands.
//! Returns [`ParseResult::Complete`], [`Incomplete`](ParseResult::Incomplete),
//! or [`Invalid`](ParseResult::Invalid) to drive the state machine.

use super::motion::MotionType;
use super::operator::Operator;
use super::text_object::{ObjectKind, TextObjectType};

/// Result of parsing the pending key buffer.
#[derive(Debug)]
pub enum ParseResult {
    /// Fully parsed command.
    Complete(ParsedCommand),
    /// Need more input.
    Incomplete,
    /// Not a valid command.
    Invalid,
}

/// Parsed grammar commands.
#[derive(Debug)]
pub enum ParsedCommand {
    /// A standalone motion.
    Motion {
        motion: MotionType,
        count: Option<usize>,
    },
    /// An operator applied to a motion range.
    OperatorMotion {
        operator: Operator,
        motion: MotionType,
        count: Option<usize>,
    },
    /// An operator applied line-wise (doubled: dd, yy, etc.).
    OperatorLine {
        operator: Operator,
        count: Option<usize>,
    },
    /// An operator applied to a text object.
    OperatorTextObject {
        operator: Operator,
        object: TextObjectType,
        count: Option<usize>,
    },
    /// A mode switch (i, a, o, v, :, etc.).
    ModeSwitch(ModeSwitch),
    /// A standalone command with optional count.
    Standalone {
        cmd: StandaloneCmd,
        count: Option<usize>,
    },
}

#[derive(Debug)]
pub enum ModeSwitch {
    InsertBefore,
    InsertAfter,
    InsertLineStart,
    InsertLineEnd,
    OpenBelow,
    OpenAbove,
    Visual,
    VisualLine,
    Command,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum StandaloneCmd {
    Undo,
    Redo,
    RepeatLast,
    PasteAfter,
    PasteBefore,
    ReplaceChar(char),
    SearchForward,
    SearchBackward,
    NextMatch,
    PrevMatch,
    StartMacro(char),
    StopMacro,
    PlayMacro(char),
}

// ── count parsing ────────────────────────────────────────────────────

fn eat_count(chars: &[char], pos: &mut usize) -> Option<usize> {
    let start = *pos;
    if *pos < chars.len() && chars[*pos] >= '1' && chars[*pos] <= '9' {
        *pos += 1;
        while *pos < chars.len() && chars[*pos].is_ascii_digit() {
            *pos += 1;
        }
        let s: String = chars[start..*pos].iter().collect();
        Some(s.parse().unwrap())
    } else {
        None
    }
}

fn combine_counts(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x * y),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

// ── main parser ──────────────────────────────────────────────────────

/// Parse the accumulated pending keys into a command.
/// `is_recording` affects how `q` is interpreted.
pub fn parse_pending(pending: &str, is_recording: bool) -> ParseResult {
    let chars: Vec<char> = pending.chars().collect();
    let n = chars.len();
    if n == 0 {
        return ParseResult::Invalid;
    }

    let mut pos = 0;

    // 1. Optional count prefix
    let count1 = eat_count(&chars, &mut pos);
    if pos >= n {
        return ParseResult::Incomplete;
    }

    let first = chars[pos];
    let remaining = n - pos;

    // 2. Multi-char standalone commands
    match first {
        'r' => {
            if remaining == 1 {
                return ParseResult::Incomplete;
            }
            if remaining == 2 {
                return ParseResult::Complete(ParsedCommand::Standalone {
                    cmd: StandaloneCmd::ReplaceChar(chars[pos + 1]),
                    count: count1,
                });
            }
            return ParseResult::Invalid;
        }
        'q' if is_recording => {
            if remaining == 1 {
                return ParseResult::Complete(ParsedCommand::Standalone {
                    cmd: StandaloneCmd::StopMacro,
                    count: None,
                });
            }
            return ParseResult::Invalid;
        }
        'q' => {
            if remaining == 1 {
                return ParseResult::Incomplete;
            }
            if remaining == 2 && chars[pos + 1].is_ascii_lowercase() {
                return ParseResult::Complete(ParsedCommand::Standalone {
                    cmd: StandaloneCmd::StartMacro(chars[pos + 1]),
                    count: None,
                });
            }
            return ParseResult::Invalid;
        }
        '@' => {
            if remaining == 1 {
                return ParseResult::Incomplete;
            }
            if remaining == 2 && chars[pos + 1].is_ascii_lowercase() {
                return ParseResult::Complete(ParsedCommand::Standalone {
                    cmd: StandaloneCmd::PlayMacro(chars[pos + 1]),
                    count: count1,
                });
            }
            return ParseResult::Invalid;
        }
        _ => {}
    }

    // 3. Single-char standalones, shortcuts, and mode switches
    if remaining == 1 {
        match first {
            // Mode switches
            'i' => return complete_mode(ModeSwitch::InsertBefore),
            'a' => return complete_mode(ModeSwitch::InsertAfter),
            'I' => return complete_mode(ModeSwitch::InsertLineStart),
            'A' => return complete_mode(ModeSwitch::InsertLineEnd),
            'o' => return complete_mode(ModeSwitch::OpenBelow),
            'O' => return complete_mode(ModeSwitch::OpenAbove),
            'v' => return complete_mode(ModeSwitch::Visual),
            'V' => return complete_mode(ModeSwitch::VisualLine),
            ':' => return complete_mode(ModeSwitch::Command),
            // Standalone commands
            'u' => return complete_standalone(StandaloneCmd::Undo, count1),
            '.' => return complete_standalone(StandaloneCmd::RepeatLast, count1),
            'p' => return complete_standalone(StandaloneCmd::PasteAfter, count1),
            'P' => return complete_standalone(StandaloneCmd::PasteBefore, count1),
            'n' => return complete_standalone(StandaloneCmd::NextMatch, count1),
            'N' => return complete_standalone(StandaloneCmd::PrevMatch, count1),
            '/' => return complete_standalone(StandaloneCmd::SearchForward, None),
            '?' => return complete_standalone(StandaloneCmd::SearchBackward, None),
            // Shortcuts: desugar to operator+motion
            'x' => {
                return ParseResult::Complete(ParsedCommand::OperatorMotion {
                    operator: Operator::Delete,
                    motion: MotionType::Right,
                    count: count1,
                })
            }
            'X' => {
                return ParseResult::Complete(ParsedCommand::OperatorMotion {
                    operator: Operator::Delete,
                    motion: MotionType::Left,
                    count: count1,
                })
            }
            's' => {
                return ParseResult::Complete(ParsedCommand::OperatorMotion {
                    operator: Operator::Change,
                    motion: MotionType::Right,
                    count: count1,
                })
            }
            'S' => {
                return ParseResult::Complete(ParsedCommand::OperatorLine {
                    operator: Operator::Change,
                    count: count1,
                })
            }
            'D' => {
                return ParseResult::Complete(ParsedCommand::OperatorMotion {
                    operator: Operator::Delete,
                    motion: MotionType::LineEnd,
                    count: None,
                })
            }
            'C' => {
                return ParseResult::Complete(ParsedCommand::OperatorMotion {
                    operator: Operator::Change,
                    motion: MotionType::LineEnd,
                    count: None,
                })
            }
            'Y' => {
                return ParseResult::Complete(ParsedCommand::OperatorLine {
                    operator: Operator::Yank,
                    count: count1,
                })
            }
            _ => {} // fall through to operator / motion parsing
        }
    }

    // 4. Try operator
    let op_start = pos;
    let maybe_op = match first {
        'd' => {
            pos += 1;
            Some(Operator::Delete)
        }
        'c' => {
            pos += 1;
            Some(Operator::Change)
        }
        'y' => {
            pos += 1;
            Some(Operator::Yank)
        }
        '>' => {
            pos += 1;
            Some(Operator::Indent)
        }
        '<' => {
            pos += 1;
            Some(Operator::Dedent)
        }
        '=' => {
            pos += 1;
            Some(Operator::AutoIndent)
        }
        'g' if pos + 1 < n && chars[pos + 1] == 'q' => {
            pos += 2;
            Some(Operator::Reflow)
        }
        _ => None,
    };

    if let Some(op) = maybe_op {
        if pos >= n {
            return ParseResult::Incomplete;
        }

        // Check for doubled operator
        let doubled = match op {
            Operator::Reflow => {
                chars[pos] == 'q' || (pos + 1 < n && chars[pos] == 'g' && chars[pos + 1] == 'q')
            }
            _ => chars[pos] == first,
        };

        if doubled {
            // Consume the doubled char(s)
            if op == Operator::Reflow && chars[pos] == 'g' {
                pos += 2;
            } else {
                pos += 1;
            }
            return if pos == n {
                ParseResult::Complete(ParsedCommand::OperatorLine {
                    operator: op,
                    count: count1,
                })
            } else {
                ParseResult::Invalid
            };
        }

        // Optional second count
        let count2 = eat_count(&chars, &mut pos);
        if pos >= n {
            return ParseResult::Incomplete;
        }

        let total = combine_counts(count1, count2);

        // Parse target: text object or motion
        return parse_target(&chars, &mut pos, n, op, total);
    }

    // 5. Parse as standalone motion
    pos = op_start;
    parse_motion_from(&chars, &mut pos, n, None, count1)
}

// ── target parsing (after operator) ──────────────────────────────────

fn parse_target(
    chars: &[char],
    pos: &mut usize,
    n: usize,
    op: Operator,
    count: Option<usize>,
) -> ParseResult {
    let c = chars[*pos];

    // Text object prefix
    if c == 'i' || c == 'a' {
        if *pos + 1 >= n {
            return ParseResult::Incomplete;
        }
        let inner = c == 'i';
        *pos += 1;
        let obj_char = chars[*pos];
        *pos += 1;

        let kind = match obj_char {
            'w' => ObjectKind::Word,
            'W' => ObjectKind::WORD,
            'p' => ObjectKind::Paragraph,
            '"' => ObjectKind::DoubleQuote,
            '\'' => ObjectKind::SingleQuote,
            '(' | ')' | 'b' => ObjectKind::Paren,
            '{' | '}' | 'B' => ObjectKind::Brace,
            '[' | ']' => ObjectKind::Bracket,
            'l' => ObjectKind::Link,
            '#' => ObjectKind::Tag,
            '@' => ObjectKind::Timestamp,
            'h' => ObjectKind::Heading,
            _ => return ParseResult::Invalid,
        };

        if *pos != n {
            return ParseResult::Invalid;
        }

        let text_obj = if inner {
            TextObjectType::Inner(kind)
        } else {
            TextObjectType::Around(kind)
        };
        return ParseResult::Complete(ParsedCommand::OperatorTextObject {
            operator: op,
            object: text_obj,
            count,
        });
    }

    // Otherwise, parse as motion
    parse_motion_from(chars, pos, n, Some(op), count)
}

// ── motion parsing ───────────────────────────────────────────────────

fn parse_motion_from(
    chars: &[char],
    pos: &mut usize,
    n: usize,
    op: Option<Operator>,
    count: Option<usize>,
) -> ParseResult {
    if *pos >= n {
        return ParseResult::Incomplete;
    }

    let c = chars[*pos];
    *pos += 1;

    let motion = match c {
        'h' => MotionType::Left,
        'l' => MotionType::Right,
        'j' => MotionType::Down,
        'k' => MotionType::Up,
        'w' => MotionType::WordForward,
        'b' => MotionType::WordBackward,
        'e' => MotionType::WordEnd,
        'W' => MotionType::WORDForward,
        'B' => MotionType::WORDBackward,
        'E' => MotionType::WORDEnd,
        '0' => MotionType::LineStart,
        '$' => MotionType::LineEnd,
        '^' => MotionType::FirstNonWhitespace,
        'G' => MotionType::DocumentEnd,
        '%' => MotionType::MatchingBracket,
        '{' => MotionType::ParagraphBackward,
        '}' => MotionType::ParagraphForward,
        ';' => MotionType::RepeatFind,
        ',' => MotionType::RepeatFindReverse,
        'g' => {
            if *pos >= n {
                return ParseResult::Incomplete;
            }
            if chars[*pos] == 'g' {
                *pos += 1;
                MotionType::DocumentStart
            } else {
                return ParseResult::Invalid;
            }
        }
        'f' => {
            if *pos >= n {
                return ParseResult::Incomplete;
            }
            let target = chars[*pos];
            *pos += 1;
            MotionType::FindForward(target)
        }
        'F' => {
            if *pos >= n {
                return ParseResult::Incomplete;
            }
            let target = chars[*pos];
            *pos += 1;
            MotionType::FindBackward(target)
        }
        't' => {
            if *pos >= n {
                return ParseResult::Incomplete;
            }
            let target = chars[*pos];
            *pos += 1;
            MotionType::ToForward(target)
        }
        'T' => {
            if *pos >= n {
                return ParseResult::Incomplete;
            }
            let target = chars[*pos];
            *pos += 1;
            MotionType::ToBackward(target)
        }
        _ => return ParseResult::Invalid,
    };

    if *pos != n {
        return ParseResult::Invalid;
    }

    match op {
        Some(operator) => ParseResult::Complete(ParsedCommand::OperatorMotion {
            operator,
            motion,
            count,
        }),
        None => ParseResult::Complete(ParsedCommand::Motion { motion, count }),
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn complete_mode(m: ModeSwitch) -> ParseResult {
    ParseResult::Complete(ParsedCommand::ModeSwitch(m))
}

fn complete_standalone(cmd: StandaloneCmd, count: Option<usize>) -> ParseResult {
    ParseResult::Complete(ParsedCommand::Standalone { cmd, count })
}
