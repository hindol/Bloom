use std::{sync::Arc, time::Duration};

use bloom_text::{coordinates::RawCharIdx, Doc, DocError, DocSnapshot, EditDelta};

use crate::{
    geometry::{FontMetrics, Layout, LayoutCache, LayoutError, MonospaceFontMetrics, Size},
    invariants::{RegistryError, Violation},
    timer::{TimerId, TimerWheel},
};

#[cfg(any(debug_assertions, feature = "simulation"))]
use crate::{
    invariants::{CheckerRegistry, InvariantId},
    timer::check_no_due_armed_timers,
};

const IDLE_COMMIT_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusOwner {
    Editor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ui {
    focus_stack: Vec<FocusOwner>,
}

impl Ui {
    fn editor_only() -> Self {
        Self {
            focus_stack: vec![FocusOwner::Editor],
        }
    }

    pub fn focus_owner(&self) -> Option<FocusOwner> {
        self.focus_stack.last().copied()
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    document: Doc,
    cursor: RawCharIdx,
    ui: Ui,
    timers: TimerWheel,
    viewport: Size,
    font_metrics: Arc<dyn FontMetrics>,
    layout_cache: LayoutCache,
    layout: Arc<Layout>,
}

impl Model {
    pub fn new(text: &str) -> Self {
        Self::with_font_metrics(
            text,
            Size::default(),
            Arc::new(MonospaceFontMetrics::default()),
        )
    }

    pub fn with_font_metrics(
        text: &str,
        viewport: Size,
        font_metrics: Arc<dyn FontMetrics>,
    ) -> Self {
        let document = Doc::new(text);
        let cursor = document.raw_start();
        let snapshot = document.snapshot();
        let mut layout_cache = LayoutCache::new(&snapshot, viewport, font_metrics.as_ref())
            .expect("document start must always have valid geometry");
        let layout = Arc::new(
            layout_cache
                .frame(&snapshot, cursor, viewport)
                .expect("document start must always have valid geometry"),
        );
        Self {
            document,
            cursor,
            ui: Ui::editor_only(),
            timers: TimerWheel::default(),
            viewport,
            font_metrics,
            layout_cache,
            layout,
        }
    }

    pub fn text(&self) -> String {
        self.document.text()
    }

    pub fn cursor(&self) -> RawCharIdx {
        self.cursor
    }

    pub fn focus_owner(&self) -> Option<FocusOwner> {
        self.ui.focus_owner()
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.timers.next_deadline()
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    InsertText(String),
    Backspace,
    Resize(Size),
    TimerWake,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    ArmTimer { id: TimerId, deadline: Duration },
    TimerFired(TimerId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateError {
    Document(DocError),
    Layout(LayoutError),
    DeadlineOverflow,
    CheckerRegistry(RegistryError),
    InvariantViolations(Vec<Violation>),
}

/// Applies an event at externally supplied monotonic time, draining all due
/// timers before handling the event.
pub fn update(model: &mut Model, event: Event, now: Duration) -> Result<Vec<Effect>, UpdateError> {
    let document_changed = match &event {
        Event::InsertText(text) => !text.is_empty(),
        Event::Backspace => {
            model
                .document
                .raw_char_offset(model.cursor)
                .map_err(UpdateError::Document)?
                > 0
        }
        Event::Resize(_) => false,
        Event::TimerWake => false,
    };
    let idle_deadline = if document_changed {
        Some(
            now.checked_add(IDLE_COMMIT_DEBOUNCE)
                .ok_or(UpdateError::DeadlineOverflow)?,
        )
    } else {
        None
    };
    let mut effects = model
        .timers
        .drain_due(now)
        .into_iter()
        .map(Effect::TimerFired)
        .collect::<Vec<_>>();
    let mut edit_delta = None;

    match event {
        Event::InsertText(text) => {
            if !text.is_empty() {
                let (cursor, delta) = model
                    .document
                    .insert_with_delta(model.cursor, &text)
                    .map_err(UpdateError::Document)?;
                model.cursor = cursor;
                edit_delta = delta;
            }
        }
        Event::Backspace => {
            let (cursor, delta) = model
                .document
                .delete_previous_char_with_delta(model.cursor)
                .map_err(UpdateError::Document)?;
            model.cursor = cursor;
            edit_delta = delta;
        }
        Event::Resize(size) => {
            model.viewport = size;
            model
                .layout_cache
                .resize(
                    &model.document.snapshot(),
                    size,
                    model.font_metrics.as_ref(),
                )
                .map_err(UpdateError::Layout)?;
        }
        Event::TimerWake => {}
    }

    if let Some(deadline) = idle_deadline {
        model.timers.schedule(TimerId::IDLE_COMMIT, deadline);
        effects.push(Effect::ArmTimer {
            id: TimerId::IDLE_COMMIT,
            deadline,
        });
    }

    settle(model, edit_delta)?;
    #[cfg(any(debug_assertions, feature = "simulation"))]
    check_update_invariants(model, now)?;
    Ok(effects)
}

pub fn view(model: &Model) -> Frame {
    Frame {
        document: model.document.snapshot(),
        cursor: model.cursor(),
        focus: model.focus_owner(),
        layout: Arc::clone(&model.layout),
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub document: DocSnapshot,
    pub cursor: RawCharIdx,
    pub focus: Option<FocusOwner>,
    pub layout: Arc<Layout>,
}

pub fn check_exactly_one_focus_owner(ui: &Ui) -> Result<(), String> {
    if ui.focus_stack.len() == 1 && ui.focus_stack[0] == FocusOwner::Editor {
        Ok(())
    } else {
        Err("M1 UI must have exactly one editor focus owner".to_owned())
    }
}

pub fn check_view_purity(model: &Model) -> Result<(), String> {
    let first = view(model);
    let second = view(model);
    if first.document.text() == second.document.text()
        && first.cursor == second.cursor
        && first.focus == second.focus
        && first.layout == second.layout
    {
        Ok(())
    } else {
        Err("view returned different frames for the same model".to_owned())
    }
}

fn settle(model: &mut Model, edit_delta: Option<EditDelta>) -> Result<(), UpdateError> {
    model
        .document
        .raw_char_offset(model.cursor)
        .map_err(UpdateError::Document)?;
    let snapshot = model.document.snapshot();
    if let Some(delta) = edit_delta {
        model
            .layout_cache
            .apply_edit(&snapshot, delta, model.font_metrics.as_ref())
            .map_err(UpdateError::Layout)?;
    }
    model.layout = Arc::new(
        model
            .layout_cache
            .frame(&snapshot, model.cursor, model.viewport)
            .map_err(UpdateError::Layout)?,
    );
    Ok(())
}

#[cfg(any(debug_assertions, feature = "simulation"))]
fn check_update_invariants(model: &Model, now: Duration) -> Result<(), UpdateError> {
    let mut registry = CheckerRegistry::new();
    registry
        .register(InvariantId::I7, || check_exactly_one_focus_owner(&model.ui))
        .map_err(UpdateError::CheckerRegistry)?;
    registry
        .register(InvariantId::I10, || {
            check_no_due_armed_timers(&model.timers, now)
        })
        .map_err(UpdateError::CheckerRegistry)?;

    let violations = registry
        .run_applicable(&[InvariantId::I7, InvariantId::I10])
        .map_err(UpdateError::CheckerRegistry)?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(UpdateError::InvariantViolations(violations))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        check_exactly_one_focus_owner, check_view_purity, update, view, Effect, Event, FocusOwner,
        Model, UpdateError,
    };
    use crate::timer::TimerId;

    #[test]
    fn update_edits_plain_text_and_settles_cursor() {
        let mut model = Model::new("note");
        update(
            &mut model,
            Event::InsertText("🙂".to_owned()),
            Duration::ZERO,
        )
        .unwrap();
        update(&mut model, Event::Backspace, Duration::ZERO).unwrap();

        assert_eq!(view(&model).document.text(), "note");
        assert_eq!(model.cursor, model.document.raw_start());
    }

    #[test]
    fn backspace_between_cr_and_lf_settles_without_panicking() {
        let mut model = Model::new("\n");
        update(
            &mut model,
            Event::InsertText("\rX".to_owned()),
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(model.text(), "\rX\n");
        update(&mut model, Event::Backspace, Duration::ZERO).unwrap();
        assert_eq!(model.text(), "\r\n");
    }

    #[test]
    fn due_timer_is_consumed_before_each_event_path() {
        let event_paths = [
            Event::InsertText("x".to_owned()),
            Event::Backspace,
            Event::TimerWake,
        ];

        for event in event_paths {
            let mut model = Model::new("a");
            let timer = TimerId::new(7);
            model.timers.schedule(timer, Duration::from_millis(5));
            let arms_idle_timer = matches!(&event, Event::InsertText(text) if !text.is_empty());

            let effects = update(&mut model, event, Duration::from_millis(5)).unwrap();

            assert!(effects.contains(&Effect::TimerFired(timer)));
            if arms_idle_timer {
                assert!(effects.contains(&Effect::ArmTimer {
                    id: TimerId::IDLE_COMMIT,
                    deadline: Duration::from_millis(305),
                }));
                assert_eq!(model.next_deadline(), Some(Duration::from_millis(305)));
            } else {
                assert_eq!(model.next_deadline(), None);
            }
        }
    }

    #[test]
    fn future_timer_remains_armed_without_blocking_update() {
        let mut model = Model::new("");
        let timer = TimerId::new(9);
        model.timers.schedule(timer, Duration::from_millis(10));

        assert!(
            update(&mut model, Event::TimerWake, Duration::from_millis(9))
                .unwrap()
                .is_empty()
        );
        assert_eq!(model.next_deadline(), Some(Duration::from_millis(10)));
    }

    #[test]
    fn text_edits_reset_the_idle_commit_deadline() {
        let mut model = Model::new("");

        let first = update(
            &mut model,
            Event::InsertText("a".to_owned()),
            Duration::ZERO,
        )
        .unwrap();
        assert!(first.contains(&Effect::ArmTimer {
            id: TimerId::IDLE_COMMIT,
            deadline: Duration::from_millis(300),
        }));

        let second = update(
            &mut model,
            Event::InsertText("b".to_owned()),
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(second.contains(&Effect::ArmTimer {
            id: TimerId::IDLE_COMMIT,
            deadline: Duration::from_millis(400),
        }));

        assert!(
            update(&mut model, Event::TimerWake, Duration::from_millis(300))
                .unwrap()
                .is_empty()
        );
        assert_eq!(model.next_deadline(), Some(Duration::from_millis(400)));
    }

    #[test]
    fn no_op_edits_do_not_arm_idle_commit_timer() {
        let mut model = Model::new("");

        assert!(
            update(&mut model, Event::InsertText(String::new()), Duration::ZERO)
                .unwrap()
                .is_empty()
        );
        assert!(update(&mut model, Event::Backspace, Duration::ZERO)
            .unwrap()
            .is_empty());
        assert_eq!(model.next_deadline(), None);
    }

    #[test]
    fn deadline_overflow_does_not_partially_apply_an_edit_or_consume_timers() {
        let mut model = Model::new("a");
        let timer = TimerId::new(9);
        model.timers.schedule(timer, Duration::MAX);

        assert_eq!(
            update(&mut model, Event::InsertText("b".to_owned()), Duration::MAX),
            Err(UpdateError::DeadlineOverflow)
        );
        assert_eq!(model.text(), "a");
        assert_eq!(model.next_deadline(), Some(Duration::MAX));
    }

    #[test]
    fn focus_stack_ownership_is_enforced_by_the_model() {
        let mut model = Model::new("");
        assert_eq!(model.focus_owner(), Some(FocusOwner::Editor));
        assert_eq!(check_exactly_one_focus_owner(&model.ui), Ok(()));

        model.ui.focus_stack.clear();
        assert!(check_exactly_one_focus_owner(&model.ui).is_err());
    }

    #[test]
    fn repeated_view_is_identical() {
        let model = Model::new("synthetic note");

        assert_eq!(check_view_purity(&model), Ok(()));
    }
}
