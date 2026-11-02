use std::{convert::Infallible, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimInputEvent {
    Keys(String),
    Text(String),
    AdvanceTime(u64),
    FlushBackground,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableReason {
    EmptyKernelHasNoProductModel,
    LayoutNotImplemented,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreenData {
    pub lines: Vec<String>,
    pub title: String,
    pub mode: String,
    pub cursor: (usize, usize),
    pub dirty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScreenSnapshot {
    Available(ScreenData),
    Unavailable(UnavailableReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BufferSnapshot {
    Available(String),
    Unavailable(UnavailableReason),
}

pub trait SimInputBackend {
    type Error;

    fn apply(&mut self, event: SimInputEvent) -> Result<(), Self::Error>;
    fn screen(&mut self, width: u16, height: u16) -> Result<ScreenSnapshot, Self::Error>;
    fn buffer_text(&self) -> Result<BufferSnapshot, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimInputError<E> {
    Backend(E),
    ScreenUnavailable(UnavailableReason),
    BufferUnavailable(UnavailableReason),
}

impl<E: fmt::Display> fmt::Display for SimInputError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "simulation backend failed: {error}"),
            Self::ScreenUnavailable(reason) => {
                write!(formatter, "screen is unavailable: {reason:?}")
            }
            Self::BufferUnavailable(reason) => {
                write!(formatter, "buffer text is unavailable: {reason:?}")
            }
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for SimInputError<E> {}

pub struct SimInput<B> {
    backend: B,
}

impl<B: SimInputBackend> SimInput<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn keys(&mut self, sequence: &str) -> Result<&mut Self, SimInputError<B::Error>> {
        self.dispatch(SimInputEvent::Keys(sequence.to_owned()))
    }

    pub fn type_text(&mut self, text: &str) -> Result<&mut Self, SimInputError<B::Error>> {
        self.dispatch(SimInputEvent::Text(text.to_owned()))
    }

    pub fn tick(&mut self, millis: u64) -> Result<&mut Self, SimInputError<B::Error>> {
        self.dispatch(SimInputEvent::AdvanceTime(millis))
    }

    pub fn flush_background(&mut self) -> Result<&mut Self, SimInputError<B::Error>> {
        self.dispatch(SimInputEvent::FlushBackground)
    }

    pub fn screen(
        &mut self,
        width: u16,
        height: u16,
    ) -> Result<TestScreen, SimInputError<B::Error>> {
        self.backend
            .screen(width, height)
            .map(|snapshot| TestScreen {
                snapshot,
                width,
                height,
            })
            .map_err(SimInputError::Backend)
    }

    pub fn buffer_text(&self) -> Result<String, SimInputError<B::Error>> {
        match self.backend.buffer_text() {
            Ok(BufferSnapshot::Available(text)) => Ok(text),
            Ok(BufferSnapshot::Unavailable(reason)) => {
                Err(SimInputError::BufferUnavailable(reason))
            }
            Err(error) => Err(SimInputError::Backend(error)),
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    fn dispatch(&mut self, event: SimInputEvent) -> Result<&mut Self, SimInputError<B::Error>> {
        self.backend
            .apply(event)
            .map(|()| self)
            .map_err(SimInputError::Backend)
    }
}

pub struct TestScreen {
    snapshot: ScreenSnapshot,
    pub width: u16,
    pub height: u16,
}

impl TestScreen {
    pub fn snapshot(&self) -> &ScreenSnapshot {
        &self.snapshot
    }

    pub fn line_text(&self, row: usize) -> Result<Option<&str>, SimInputError<Infallible>> {
        self.data()
            .map(|data| data.lines.get(row).map(String::as_str))
    }

    pub fn all_lines(&self) -> Result<String, SimInputError<Infallible>> {
        self.data().map(|data| data.lines.join("\n"))
    }

    pub fn title(&self) -> Result<&str, SimInputError<Infallible>> {
        self.data().map(|data| data.title.as_str())
    }

    pub fn mode(&self) -> Result<&str, SimInputError<Infallible>> {
        self.data().map(|data| data.mode.as_str())
    }

    pub fn cursor(&self) -> Result<(usize, usize), SimInputError<Infallible>> {
        self.data().map(|data| data.cursor)
    }

    pub fn is_dirty(&self) -> Result<bool, SimInputError<Infallible>> {
        self.data().map(|data| data.dirty)
    }

    fn data(&self) -> Result<&ScreenData, SimInputError<Infallible>> {
        match &self.snapshot {
            ScreenSnapshot::Available(data) => Ok(data),
            ScreenSnapshot::Unavailable(reason) => Err(SimInputError::ScreenUnavailable(*reason)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioSource {
    Oracle,
    InfrastructureOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioStatus {
    Pending,
    Implemented,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioOutcome {
    Passed,
    Failed,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScenarioEvidence {
    pub source: ScenarioSource,
    pub status: ScenarioStatus,
    pub outcome: ScenarioOutcome,
}

impl ScenarioEvidence {
    pub fn validate(self) -> Result<Self, ScenarioGateError> {
        if self.source == ScenarioSource::Oracle
            && self.status == ScenarioStatus::Pending
            && self.outcome == ScenarioOutcome::Passed
        {
            return Err(ScenarioGateError::PendingOracleCannotPass);
        }
        Ok(self)
    }

    pub const fn counts_as_parity(self) -> bool {
        matches!(self.source, ScenarioSource::Oracle)
            && matches!(self.status, ScenarioStatus::Implemented)
            && matches!(self.outcome, ScenarioOutcome::Passed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioGateError {
    PendingOracleCannotPass,
}

impl fmt::Display for ScenarioGateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PendingOracleCannotPass => {
                formatter.write_str("a pending oracle scenario cannot be marked passed")
            }
        }
    }
}

impl std::error::Error for ScenarioGateError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmptyKernelError;

impl fmt::Display for EmptyKernelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("injected empty-kernel adapter failure")
    }
}

impl std::error::Error for EmptyKernelError {}

#[derive(Default)]
pub struct EmptyKernelBackend {
    events: Vec<SimInputEvent>,
    fail_next_input: bool,
}

impl EmptyKernelBackend {
    pub fn events(&self) -> &[SimInputEvent] {
        &self.events
    }

    pub fn fail_next_input(&mut self) {
        self.fail_next_input = true;
    }
}

impl SimInputBackend for EmptyKernelBackend {
    type Error = EmptyKernelError;

    fn apply(&mut self, event: SimInputEvent) -> Result<(), Self::Error> {
        if self.fail_next_input {
            self.fail_next_input = false;
            return Err(EmptyKernelError);
        }
        self.events.push(event);
        Ok(())
    }

    fn screen(&mut self, _width: u16, _height: u16) -> Result<ScreenSnapshot, Self::Error> {
        Ok(ScreenSnapshot::Unavailable(
            UnavailableReason::EmptyKernelHasNoProductModel,
        ))
    }

    fn buffer_text(&self) -> Result<BufferSnapshot, Self::Error> {
        Ok(BufferSnapshot::Unavailable(
            UnavailableReason::EmptyKernelHasNoProductModel,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::update::{update, view, Event, Model};

    use super::*;

    struct PlainTextBackend {
        model: Model,
    }

    fn cursor_offset(backend: &PlainTextBackend) -> usize {
        let frame = view(&backend.model);
        frame.document.raw_char_offset(frame.cursor).unwrap()
    }

    impl SimInputBackend for PlainTextBackend {
        type Error = String;

        fn apply(&mut self, event: SimInputEvent) -> Result<(), Self::Error> {
            let event = match event {
                SimInputEvent::Text(text) => Event::InsertText(text),
                SimInputEvent::Keys(keys) if keys == "<BS>" => Event::Backspace,
                event => return Err(format!("unsupported plain-text event: {event:?}")),
            };
            update(&mut self.model, event, Duration::ZERO)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        }

        fn screen(&mut self, _width: u16, _height: u16) -> Result<ScreenSnapshot, Self::Error> {
            Ok(ScreenSnapshot::Unavailable(
                UnavailableReason::LayoutNotImplemented,
            ))
        }

        fn buffer_text(&self) -> Result<BufferSnapshot, Self::Error> {
            Ok(BufferSnapshot::Available(self.model.text()))
        }
    }

    #[test]
    fn plain_text_unicode_input_preserves_order_and_character_offsets() {
        for typed in ["Ωλ", "aé中🙂", "e\u{301}👩\u{200d}💻"] {
            let mut sim = SimInput::new(PlainTextBackend {
                model: Model::new("ab\n"),
            });
            let mut prefix = String::new();
            for character in typed.chars() {
                sim.type_text(&character.to_string()).unwrap();
                prefix.push(character);
                assert_eq!(sim.buffer_text().unwrap(), format!("{prefix}ab\n"));
                assert_eq!(cursor_offset(sim.backend()), prefix.chars().count());
            }
            assert_eq!(
                sim.screen(80, 24).unwrap().all_lines(),
                Err(SimInputError::ScreenUnavailable(
                    UnavailableReason::LayoutNotImplemented
                ))
            );

            let mut whole = SimInput::new(PlainTextBackend {
                model: Model::new("ab\n"),
            });
            whole.type_text(typed).unwrap();
            assert_eq!(whole.buffer_text().unwrap(), sim.buffer_text().unwrap());
            assert_eq!(cursor_offset(whole.backend()), typed.chars().count());
        }
    }

    #[test]
    fn plain_text_backspace_merges_and_resplits_crlf_without_losing_text() {
        let mut sim = SimInput::new(PlainTextBackend {
            model: Model::new("\nlast"),
        });
        sim.type_text("\rX").unwrap();
        assert_eq!(sim.buffer_text().unwrap(), "\rX\nlast");

        for _ in 0..2 {
            sim.keys("<BS>").unwrap();
            assert_eq!(sim.buffer_text().unwrap(), "\r\nlast");
            assert_eq!(cursor_offset(sim.backend()), 1);
            assert_eq!(
                view(&sim.backend().model)
                    .layout
                    .lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["", "last"]
            );
            sim.type_text("X").unwrap();
            assert_eq!(sim.buffer_text().unwrap(), "\rX\nlast");
            assert_eq!(cursor_offset(sim.backend()), 2);
            assert_eq!(
                view(&sim.backend().model)
                    .layout
                    .lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["\r", "X", "last"]
            );
        }
    }

    #[test]
    fn plain_text_newline_after_cr_updates_the_previous_layout_line() {
        let mut sim = SimInput::new(PlainTextBackend {
            model: Model::new(""),
        });
        sim.type_text("\r").unwrap().type_text("\n").unwrap();
        assert_eq!(sim.buffer_text().unwrap(), "\r\n");
        assert_eq!(cursor_offset(sim.backend()), 2);
        assert_eq!(
            view(&sim.backend().model)
                .layout
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["", ""]
        );
    }

    #[test]
    #[ignore = "M2 I6 blocker: insertion can merge with the existing grapheme suffix"]
    fn insertion_before_a_combining_suffix_keeps_the_cursor_on_a_grapheme_boundary() {
        let mut failures = Vec::new();
        for (initial, typed, expected) in [
            ("\u{301}ab", "e", "e\u{301}ab"),
            ("\u{200d}💻ab", "👩", "👩\u{200d}💻ab"),
        ] {
            let mut sim = SimInput::new(PlainTextBackend {
                model: Model::new(initial),
            });
            sim.type_text(typed).unwrap();
            assert_eq!(sim.buffer_text().unwrap(), expected);
            let frame = view(&sim.backend().model);
            let offset = frame.document.raw_char_offset(frame.cursor).unwrap();
            let projection = frame.document.visible_projection();
            assert!(projection.tokens().is_empty());
            if let Err(error) = projection.range(offset, offset) {
                failures.push(format!("{expected:?} at {offset}: {error:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn empty_kernel_smoke_dispatches_compatible_input_without_product_parity() {
        let mut sim = SimInput::new(EmptyKernelBackend::default());
        sim.keys("i")
            .unwrap()
            .type_text("synthetic note")
            .unwrap()
            .tick(5)
            .unwrap();
        sim.flush_background().unwrap();

        assert_eq!(
            sim.backend().events(),
            &[
                SimInputEvent::Keys("i".to_owned()),
                SimInputEvent::Text("synthetic note".to_owned()),
                SimInputEvent::AdvanceTime(5),
                SimInputEvent::FlushBackground,
            ]
        );
        assert_eq!(
            sim.screen(80, 24).unwrap().all_lines(),
            Err(SimInputError::ScreenUnavailable(
                UnavailableReason::EmptyKernelHasNoProductModel
            ))
        );
        assert_eq!(
            sim.buffer_text(),
            Err(SimInputError::BufferUnavailable(
                UnavailableReason::EmptyKernelHasNoProductModel
            ))
        );
        let evidence = ScenarioEvidence {
            source: ScenarioSource::InfrastructureOnly,
            status: ScenarioStatus::Implemented,
            outcome: ScenarioOutcome::Passed,
        };
        assert!(!evidence.validate().unwrap().counts_as_parity());
    }

    #[test]
    fn backend_failures_propagate_and_pending_oracle_cannot_pass() {
        let mut backend = EmptyKernelBackend::default();
        backend.fail_next_input();
        let mut sim = SimInput::new(backend);
        assert!(matches!(
            sim.keys("x"),
            Err(SimInputError::Backend(EmptyKernelError))
        ));

        assert_eq!(
            ScenarioEvidence {
                source: ScenarioSource::Oracle,
                status: ScenarioStatus::Pending,
                outcome: ScenarioOutcome::Passed,
            }
            .validate(),
            Err(ScenarioGateError::PendingOracleCannotPass)
        );
        assert!(!ScenarioEvidence {
            source: ScenarioSource::Oracle,
            status: ScenarioStatus::Pending,
            outcome: ScenarioOutcome::Blocked,
        }
        .validate()
        .unwrap()
        .counts_as_parity());
    }

    #[test]
    fn available_screen_distinguishes_missing_rows_from_empty_content() {
        let screen = TestScreen {
            snapshot: ScreenSnapshot::Available(ScreenData {
                lines: vec![String::new(), "note".to_owned()],
                title: "Scratch".to_owned(),
                mode: "INSERT".to_owned(),
                cursor: (0, 0),
                dirty: true,
            }),
            width: 80,
            height: 24,
        };
        assert_eq!(screen.line_text(0), Ok(Some("")));
        assert_eq!(screen.line_text(1), Ok(Some("note")));
        assert_eq!(screen.line_text(2), Ok(None));
        assert_eq!(screen.title(), Ok("Scratch"));
    }
}
