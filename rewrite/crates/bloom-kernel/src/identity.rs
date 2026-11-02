use std::collections::BTreeMap;

use bloom_text::coordinates::VisibleRange;
use bloom_text::{BlockIdMarker, BlockIdToken, Doc, ProjectionError, VisibleProjection};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PageId(String);

impl PageId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockId(String);

impl BlockId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.is_empty()
            || !value
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
        {
            return None;
        }
        Some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DetachGeneration(u64);

impl DetachGeneration {
    pub const INITIAL: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityState {
    Available,
    Live {
        page: PageId,
        generation: DetachGeneration,
    },
    Detached {
        page: PageId,
        generation: DetachGeneration,
    },
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryChange {
    AssertLive {
        id: BlockId,
        page: PageId,
        generation: DetachGeneration,
    },
    Reserve {
        id: BlockId,
        page: PageId,
    },
    Detach {
        id: BlockId,
        page: PageId,
        previous_generation: DetachGeneration,
    },
    MoveDetached {
        id: BlockId,
        source_page: PageId,
        target_page: PageId,
        generation: DetachGeneration,
    },
    RetireDetached {
        id: BlockId,
        page: PageId,
        generation: DetachGeneration,
    },
    Retire {
        id: BlockId,
        page: PageId,
        generation: DetachGeneration,
    },
}

/// `apply` must atomically validate each change's preconditions and commit the
/// complete change set or leave the registry unchanged. Reservations start at
/// generation zero; detachment increments the expected generation without
/// wrapping, and moves retain it.
pub trait BlockIdRegistry {
    type Error;

    fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error>;
    fn apply(&mut self, changes: &[RegistryChange]) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterOperation {
    Yank,
    Delete { generation: DetachGeneration },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterIdentity {
    pub id: BlockId,
    pub source_page: PageId,
    pub operation: RegisterOperation,
}

/// A captured register cannot be cloned outside its owning register bank.
///
/// ```compile_fail,E0599
/// use bloom_kernel::identity::BlockRegister;
/// fn untracked_alias(register: BlockRegister) {
///     let _other = register.clone();
/// }
/// ```
#[must_use = "move registers require explicit ownership and cleanup"]
#[derive(Debug, Eq, PartialEq)]
pub struct BlockRegister {
    visible_text: String,
    identity: Option<RegisterIdentity>,
}

impl BlockRegister {
    pub fn yank(visible_text: String, id: BlockId, source_page: PageId) -> Self {
        Self {
            visible_text,
            identity: Some(RegisterIdentity {
                id,
                source_page,
                operation: RegisterOperation::Yank,
            }),
        }
    }

    pub fn without_identity(visible_text: String) -> Self {
        Self {
            visible_text,
            identity: None,
        }
    }

    pub fn visible_text(&self) -> &str {
        &self.visible_text
    }

    pub fn identity(&self) -> Option<&RegisterIdentity> {
        self.identity.as_ref()
    }

    pub fn discard<R: BlockIdRegistry>(
        self,
        registry: &mut R,
    ) -> Result<(), RegisterDiscardFailure<R::Error>> {
        if let Err(error) = discard_identity(&self, registry) {
            return Err(RegisterDiscardFailure {
                error,
                register: self,
            });
        }
        Ok(())
    }

    fn tracked_alias(&self) -> Self {
        Self {
            visible_text: self.visible_text.clone(),
            identity: self.identity.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockTextDeletion {
    pub canonical_text: String,
    pub visible_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityDecision {
    Delete {
        detached_id: BlockId,
    },
    Paste {
        assigned_id: Option<BlockId>,
        reused_source_id: bool,
    },
    Join {
        retained_id: BlockId,
        retired_id: BlockId,
    },
    Split {
        left_id: BlockId,
        right_id: BlockId,
    },
    DiscardRegister {
        retired_id: Option<BlockId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityPlan {
    pub decision: IdentityDecision,
    pub changes: Vec<RegistryChange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityPlanError<E> {
    Registry(E),
    InvalidFreshId,
    FreshIdUnavailable(BlockId),
    DeleteSourceNotLive(BlockId),
    DeleteIdentityMissing(BlockId),
    DetachGenerationExhausted(BlockId),
    JoinedIdsMustDiffer,
    JoinedIdNotLive(BlockId),
    SplitSourceNotLive(BlockId),
    DetachedRegisterPageMismatch(BlockId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityTextPlanError {
    Projection(ProjectionError),
    UnsupportedTokenSet,
    IdentityMismatch,
    InvalidJoinEdit,
    InvalidSplitEdit,
    InvalidPastePlan,
    InvalidPasteLocation,
    InvalidRegisterText,
    DuplicateIdentity,
    RequiresStructuralEdit,
    InvalidResult,
    LineOutOfBounds(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityEditPlanError<E> {
    Identity(IdentityPlanError<E>),
    Text(IdentityTextPlanError),
}

#[derive(Debug, Default)]
pub struct RegisterBank {
    slots: BTreeMap<char, BlockRegister>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterSlotError {
    Empty(char),
    Occupied(char),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterCleanupError<E> {
    Identity(IdentityPlanError<E>),
    Registry(E),
}

#[derive(Debug)]
pub struct RegisterDiscardFailure<E> {
    pub error: RegisterCleanupError<E>,
    pub register: BlockRegister,
}

#[must_use = "retry replacement or explicitly discard the returned register"]
#[derive(Debug)]
pub struct RegisterReplaceFailure<E> {
    pub error: RegisterCleanupError<E>,
    pub register: BlockRegister,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterClearError<E> {
    Empty(char),
    Identity(IdentityPlanError<E>),
    Registry(E),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegisterAliasReplaceError<E> {
    EmptySource(char),
    Cleanup(RegisterCleanupError<E>),
}

impl RegisterBank {
    pub fn get(&self, slot: char) -> Option<&BlockRegister> {
        self.slots.get(&slot)
    }

    pub fn insert(&mut self, slot: char, register: BlockRegister) -> Result<(), BlockRegister> {
        match self.slots.entry(slot) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(register);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => Err(register),
        }
    }

    pub fn alias(&mut self, source: char, target: char) -> Result<(), RegisterSlotError> {
        let register = self
            .slots
            .get(&source)
            .ok_or(RegisterSlotError::Empty(source))?;
        if self.slots.contains_key(&target) {
            return Err(RegisterSlotError::Occupied(target));
        }
        let alias = register.tracked_alias();
        self.slots.insert(target, alias);
        Ok(())
    }

    /// Replaces a slot only after final-owner cleanup succeeds. On failure,
    /// the old slot is unchanged and the incoming register is returned.
    pub fn replace<R: BlockIdRegistry>(
        &mut self,
        slot: char,
        register: BlockRegister,
        registry: &mut R,
    ) -> Result<(), RegisterReplaceFailure<R::Error>> {
        if let Some(old) = self.slots.get(&slot) {
            if !self.has_other_owner(slot, Some(&register)) {
                if let Err(error) = discard_identity(old, registry) {
                    return Err(RegisterReplaceFailure { error, register });
                }
            }
        }
        self.slots.insert(slot, register);
        Ok(())
    }

    /// Replaces a slot with a tracked alias without removing its source.
    pub fn replace_with_alias<R: BlockIdRegistry>(
        &mut self,
        source: char,
        target: char,
        registry: &mut R,
    ) -> Result<(), RegisterAliasReplaceError<R::Error>> {
        let register = self
            .slots
            .get(&source)
            .ok_or(RegisterAliasReplaceError::EmptySource(source))?;
        if source == target {
            return Ok(());
        }
        let alias = register.tracked_alias();
        self.replace(target, alias, registry)
            .map_err(|failure| RegisterAliasReplaceError::Cleanup(failure.error))
    }

    pub fn clear<R: BlockIdRegistry>(
        &mut self,
        slot: char,
        registry: &mut R,
    ) -> Result<(), RegisterClearError<R::Error>> {
        let register = self
            .slots
            .get(&slot)
            .ok_or(RegisterClearError::Empty(slot))?;
        if !self.has_other_owner(slot, None) {
            discard_identity(register, registry).map_err(|error| match error {
                RegisterCleanupError::Identity(error) => RegisterClearError::Identity(error),
                RegisterCleanupError::Registry(error) => RegisterClearError::Registry(error),
            })?;
        }
        self.slots.remove(&slot);
        Ok(())
    }

    fn has_other_owner(&self, slot: char, incoming: Option<&BlockRegister>) -> bool {
        let Some(register) = self.slots.get(&slot) else {
            return false;
        };
        incoming.is_some_and(|other| same_detachment(register, other))
            || self
                .slots
                .iter()
                .any(|(other_slot, other)| *other_slot != slot && same_detachment(register, other))
    }
}

fn same_detachment(first: &BlockRegister, second: &BlockRegister) -> bool {
    first.identity.as_ref().is_some_and(|identity| {
        matches!(identity.operation, RegisterOperation::Delete { .. })
            && second.identity.as_ref() == Some(identity)
    })
}

/// A complete in-memory edit bound to the text and registry it was planned for.
///
/// The exclusive borrows prevent intervening edits before the plan is consumed:
///
/// ```compile_fail,E0499
/// use bloom_kernel::identity::{
///     commit_planned_identity_edit, plan_split_edit, BlockId, BlockIdRegistry, PageId,
/// };
/// fn stale_edit<R: BlockIdRegistry>(
///     page: &PageId, text: &mut String, left: &BlockId, right: BlockId, registry: &mut R,
/// ) {
///     let Ok(plan) = plan_split_edit(page, text, 1, left, right, registry) else { return };
///     text.push_str("another edit");
///     let _ = commit_planned_identity_edit(plan);
/// }
/// ```
#[must_use]
pub struct CanonicalIdentityEditPlan<'a, R: BlockIdRegistry> {
    canonical_text: &'a mut String,
    registry: &'a mut R,
    planned_text: String,
    identity: IdentityPlan,
}

#[must_use]
pub struct CanonicalBlockDeletePlan<'a, R: BlockIdRegistry> {
    edit: CanonicalIdentityEditPlan<'a, R>,
    register: BlockRegister,
}

/// A projection and its exclusively borrowed source document.
///
/// ```compile_fail,E0499
/// use bloom_kernel::identity::{begin_visible_edit, commit_planned_visible_edit};
/// fn stale_edit(text: &mut String) {
///     let context = begin_visible_edit(text);
///     let Ok(range) = context.projection().range(0, 0) else { return };
///     let Ok(plan) = context.plan(range, "x") else { return };
///     text.push_str("another edit");
///     commit_planned_visible_edit(plan);
/// }
/// ```
#[must_use]
pub struct VisibleEditContext<'a> {
    canonical_text: &'a mut String,
    projection: VisibleProjection,
}

/// An identity-preserving text edit bound to the exact source document.
#[must_use]
pub struct CanonicalVisibleEditPlan<'a> {
    canonical_text: &'a mut String,
    planned_text: String,
}

fn live_generation_on_page<R: BlockIdRegistry>(
    registry: &R,
    page: &PageId,
    id: &BlockId,
) -> Result<Option<DetachGeneration>, R::Error> {
    Ok(match registry.state(id)? {
        IdentityState::Live {
            page: live_page,
            generation,
        } if live_page == *page => Some(generation),
        _ => None,
    })
}

fn plan_delete<R: BlockIdRegistry>(
    page: &PageId,
    id: &BlockId,
    visible_text: String,
    registry: &R,
) -> Result<(BlockRegister, IdentityPlan), IdentityPlanError<R::Error>> {
    let previous_generation = live_generation_on_page(registry, page, id)
        .map_err(IdentityPlanError::Registry)?
        .ok_or_else(|| IdentityPlanError::DeleteSourceNotLive(id.clone()))?;
    let generation = previous_generation
        .next()
        .ok_or_else(|| IdentityPlanError::DetachGenerationExhausted(id.clone()))?;

    Ok((
        BlockRegister {
            visible_text,
            identity: Some(RegisterIdentity {
                id: id.clone(),
                source_page: page.clone(),
                operation: RegisterOperation::Delete { generation },
            }),
        },
        IdentityPlan {
            decision: IdentityDecision::Delete {
                detached_id: id.clone(),
            },
            changes: vec![RegistryChange::Detach {
                id: id.clone(),
                page: page.clone(),
                previous_generation,
            }],
        },
    ))
}

pub fn plan_paste<R: BlockIdRegistry>(
    register: &BlockRegister,
    target_page: &PageId,
    fresh_id: Option<BlockId>,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    let Some(identity) = &register.identity else {
        return Ok(IdentityPlan {
            decision: IdentityDecision::Paste {
                assigned_id: None,
                reused_source_id: false,
            },
            changes: Vec::new(),
        });
    };

    let source_state = registry
        .state(&identity.id)
        .map_err(IdentityPlanError::Registry)?;
    if source_state == IdentityState::Available
        && matches!(identity.operation, RegisterOperation::Delete { .. })
    {
        return Err(IdentityPlanError::DeleteIdentityMissing(
            identity.id.clone(),
        ));
    }
    let owned_generation = match (source_state, identity.operation) {
        (
            IdentityState::Detached {
                page: source_page,
                generation,
            },
            RegisterOperation::Delete {
                generation: captured_generation,
            },
        ) if generation == captured_generation => {
            if source_page != identity.source_page {
                return Err(IdentityPlanError::DetachedRegisterPageMismatch(
                    identity.id.clone(),
                ));
            }
            Some(generation)
        }
        _ => None,
    };

    let (assigned_id, reused_source_id, change) = if let Some(generation) = owned_generation {
        let assigned_id = identity.id.clone();
        let change = RegistryChange::MoveDetached {
            id: assigned_id.clone(),
            source_page: identity.source_page.clone(),
            target_page: target_page.clone(),
            generation,
        };
        (assigned_id, true, change)
    } else {
        let fresh_id = fresh_id.ok_or(IdentityPlanError::InvalidFreshId)?;
        if fresh_id == identity.id
            || registry
                .state(&fresh_id)
                .map_err(IdentityPlanError::Registry)?
                != IdentityState::Available
        {
            return Err(IdentityPlanError::FreshIdUnavailable(fresh_id));
        }
        let change = RegistryChange::Reserve {
            id: fresh_id.clone(),
            page: target_page.clone(),
        };
        (fresh_id, false, change)
    };

    Ok(IdentityPlan {
        decision: IdentityDecision::Paste {
            assigned_id: Some(assigned_id.clone()),
            reused_source_id,
        },
        changes: vec![change],
    })
}

fn plan_discard_register<R: BlockIdRegistry>(
    register: &BlockRegister,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    let Some(identity) = &register.identity else {
        return Ok(IdentityPlan {
            decision: IdentityDecision::DiscardRegister { retired_id: None },
            changes: Vec::new(),
        });
    };
    let RegisterOperation::Delete {
        generation: captured_generation,
    } = identity.operation
    else {
        return Ok(IdentityPlan {
            decision: IdentityDecision::DiscardRegister { retired_id: None },
            changes: Vec::new(),
        });
    };

    let change = match registry
        .state(&identity.id)
        .map_err(IdentityPlanError::Registry)?
    {
        IdentityState::Detached {
            page: source_page,
            generation,
        } if generation == captured_generation => {
            if source_page != identity.source_page {
                return Err(IdentityPlanError::DetachedRegisterPageMismatch(
                    identity.id.clone(),
                ));
            }
            Some(RegistryChange::RetireDetached {
                id: identity.id.clone(),
                page: source_page,
                generation,
            })
        }
        IdentityState::Available => {
            return Err(IdentityPlanError::DeleteIdentityMissing(
                identity.id.clone(),
            ));
        }
        _ => None,
    };
    Ok(IdentityPlan {
        decision: IdentityDecision::DiscardRegister {
            retired_id: change.as_ref().map(|_| identity.id.clone()),
        },
        changes: change.into_iter().collect(),
    })
}

fn discard_identity<R: BlockIdRegistry>(
    register: &BlockRegister,
    registry: &mut R,
) -> Result<(), RegisterCleanupError<R::Error>> {
    let plan = plan_discard_register(register, registry).map_err(RegisterCleanupError::Identity)?;
    if !plan.changes.is_empty() {
        registry
            .apply(&plan.changes)
            .map_err(RegisterCleanupError::Registry)?;
    }
    Ok(())
}

pub fn plan_join<R: BlockIdRegistry>(
    page: &PageId,
    retained_id: &BlockId,
    retired_id: &BlockId,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    if retained_id == retired_id {
        return Err(IdentityPlanError::JoinedIdsMustDiffer);
    }
    let retained_generation = live_generation_on_page(registry, page, retained_id)
        .map_err(IdentityPlanError::Registry)?
        .ok_or_else(|| IdentityPlanError::JoinedIdNotLive(retained_id.clone()))?;
    let retired_generation = live_generation_on_page(registry, page, retired_id)
        .map_err(IdentityPlanError::Registry)?
        .ok_or_else(|| IdentityPlanError::JoinedIdNotLive(retired_id.clone()))?;

    Ok(IdentityPlan {
        decision: IdentityDecision::Join {
            retained_id: retained_id.clone(),
            retired_id: retired_id.clone(),
        },
        changes: vec![
            RegistryChange::AssertLive {
                id: retained_id.clone(),
                page: page.clone(),
                generation: retained_generation,
            },
            RegistryChange::Retire {
                id: retired_id.clone(),
                page: page.clone(),
                generation: retired_generation,
            },
        ],
    })
}

pub fn plan_split<R: BlockIdRegistry>(
    page: &PageId,
    left_id: &BlockId,
    right_id: BlockId,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    let generation = live_generation_on_page(registry, page, left_id)
        .map_err(IdentityPlanError::Registry)?
        .ok_or_else(|| IdentityPlanError::SplitSourceNotLive(left_id.clone()))?;
    if registry
        .state(&right_id)
        .map_err(IdentityPlanError::Registry)?
        != IdentityState::Available
    {
        return Err(IdentityPlanError::FreshIdUnavailable(right_id));
    }

    Ok(IdentityPlan {
        decision: IdentityDecision::Split {
            left_id: left_id.clone(),
            right_id: right_id.clone(),
        },
        changes: vec![
            RegistryChange::AssertLive {
                id: left_id.clone(),
                page: page.clone(),
                generation,
            },
            RegistryChange::Reserve {
                id: right_id,
                page: page.clone(),
            },
        ],
    })
}

/// Applies a precomputed canonical-text result only after the registry accepts
/// its complete change set. `String` replacement is infallible after that point.
fn commit_identity_edit<R: BlockIdRegistry>(
    registry: &mut R,
    canonical_text: &mut String,
    planned_text: String,
    plan: &IdentityPlan,
) -> Result<(), R::Error> {
    registry.apply(&plan.changes)?;
    *canonical_text = planned_text;
    Ok(())
}

/// Creates a projection while exclusively borrowing its canonical source.
pub fn begin_visible_edit(canonical_text: &mut String) -> VisibleEditContext<'_> {
    let projection = Doc::new(canonical_text.as_str()).visible_projection();
    VisibleEditContext {
        canonical_text,
        projection,
    }
}

impl<'a> VisibleEditContext<'a> {
    pub fn projection(&self) -> &VisibleProjection {
        &self.projection
    }

    /// Plans a single-line visible replacement that preserves every token.
    pub fn plan(
        self,
        visible_range: VisibleRange,
        replacement: &str,
    ) -> Result<CanonicalVisibleEditPlan<'a>, IdentityTextPlanError> {
        let planned_text = plan_visible_text(
            self.canonical_text.as_str(),
            &self.projection,
            visible_range,
            replacement,
        )?;
        Ok(CanonicalVisibleEditPlan {
            canonical_text: self.canonical_text,
            planned_text,
        })
    }
}

/// Commits a previously validated visible edit.
pub fn commit_planned_visible_edit(plan: CanonicalVisibleEditPlan<'_>) {
    *plan.canonical_text = plan.planned_text;
}

fn plan_visible_text(
    canonical_text: &str,
    projection: &VisibleProjection,
    range: VisibleRange,
    replacement: &str,
) -> Result<String, IdentityTextPlanError> {
    let doc = Doc::new(canonical_text);
    let visible_range = range.start().get()..range.end().get();
    let lowered = match projection.lower(range) {
        Ok(lowered) => lowered,
        Err(ProjectionError::InsertionAtLeadingToken { .. })
            if visible_range.is_empty() && replacement.is_empty() =>
        {
            return Ok(canonical_text.to_owned());
        }
        Err(error) => return Err(IdentityTextPlanError::Projection(error)),
    };
    let visible = projection.text();
    let start_byte = char_to_byte(visible, visible_range.start).expect("validated visible start");
    let end_byte = char_to_byte(visible, visible_range.end).expect("validated visible end");
    if visible[start_byte..end_byte]
        .chars()
        .chain(replacement.chars())
        .any(|character| matches!(character, '\r' | '\n'))
    {
        return Err(IdentityTextPlanError::RequiresStructuralEdit);
    }
    if visible_range.is_empty() && replacement.is_empty() {
        return Ok(canonical_text.to_owned());
    }

    let raw_segments = lowered
        .raw_segments
        .iter()
        .map(|segment| {
            let start = doc
                .raw_char_offset(segment.start())
                .expect("lowered segment belongs to canonical text");
            let end = doc
                .raw_char_offset(segment.end())
                .expect("lowered segment belongs to canonical text");
            start..end
        })
        .collect::<Vec<_>>();
    let insertion = if replacement.is_empty() {
        None
    } else {
        let insertion = if visible_range.is_empty() {
            lowered.insertion
        } else {
            let start = projection
                .range(visible_range.start, visible_range.start)
                .map_err(IdentityTextPlanError::Projection)?;
            projection
                .lower(start)
                .map_err(IdentityTextPlanError::Projection)?
                .insertion
        }
        .ok_or(IdentityTextPlanError::InvalidResult)?;
        Some(
            doc.raw_char_offset(insertion)
                .expect("lowered insertion belongs to canonical text"),
        )
    };

    let mut raw_chars = canonical_text.chars().collect::<Vec<_>>();
    for segment in raw_segments.iter().rev() {
        raw_chars.drain(segment.clone());
    }
    if let Some(insertion) = insertion {
        if raw_segments
            .iter()
            .any(|segment| segment.start < insertion && insertion < segment.end)
        {
            return Err(IdentityTextPlanError::InvalidResult);
        }
        let removed_before = raw_segments
            .iter()
            .filter(|segment| segment.end <= insertion)
            .map(|segment| segment.end - segment.start)
            .sum::<usize>();
        let at = insertion - removed_before;
        raw_chars.splice(at..at, replacement.chars());
    }
    let planned_text = raw_chars.into_iter().collect::<String>();
    let expected_visible = replace_chars(visible, visible_range, replacement);
    validate_visible_edit_result(
        &doc,
        projection,
        &planned_text,
        &expected_visible,
        &raw_segments,
        insertion,
        replacement.chars().count(),
    )?;
    Ok(planned_text)
}

fn validate_visible_edit_result(
    doc: &Doc,
    projection: &VisibleProjection,
    planned_text: &str,
    expected_visible: &str,
    raw_segments: &[std::ops::Range<usize>],
    insertion: Option<usize>,
    inserted_len: usize,
) -> Result<(), IdentityTextPlanError> {
    let planned_doc = Doc::new(planned_text);
    let planned_projection = planned_doc.visible_projection();
    if planned_projection.text() != expected_visible
        || planned_projection.tokens().len() != projection.tokens().len()
    {
        return Err(IdentityTextPlanError::InvalidResult);
    }

    for (original, planned) in projection.tokens().iter().zip(planned_projection.tokens()) {
        let start = doc
            .raw_char_offset(original.raw_range().start())
            .expect("token belongs to canonical text");
        let end = doc
            .raw_char_offset(original.raw_range().end())
            .expect("token belongs to canonical text");
        if raw_segments
            .iter()
            .any(|segment| segment.start < end && start < segment.end)
            || insertion.is_some_and(|at| start < at && at < end)
        {
            return Err(IdentityTextPlanError::InvalidResult);
        }
        let removed_before = raw_segments
            .iter()
            .filter(|segment| segment.end <= start)
            .map(|segment| segment.end - segment.start)
            .sum::<usize>();
        let inserted_before = insertion
            .filter(|at| *at <= start)
            .map_or(0, |_| inserted_len);
        let expected_start = start - removed_before + inserted_before;
        let expected_end = end - removed_before + inserted_before;
        if planned.marker() != original.marker()
            || planned.id() != original.id()
            || planned_doc
                .raw_char_offset(planned.raw_range().start())
                .expect("token belongs to planned text")
                != expected_start
            || planned_doc
                .raw_char_offset(planned.raw_range().end())
                .expect("token belongs to planned text")
                != expected_end
        {
            return Err(IdentityTextPlanError::InvalidResult);
        }
    }
    Ok(())
}

pub fn plan_join_text(
    canonical_text: &str,
    visible_range: std::ops::Range<usize>,
    replacement: &str,
    retained_id: &BlockId,
    retired_id: &BlockId,
) -> Result<String, IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    if retained_id == retired_id {
        return Err(IdentityTextPlanError::IdentityMismatch);
    }
    let (_, retained_token) = unique_block_token(&projection, retained_id)?;
    let (retired_index, retired_token) = unique_block_token(&projection, retired_id)?;

    projection
        .lower(
            projection
                .range(visible_range.start, visible_range.end)
                .map_err(IdentityTextPlanError::Projection)?,
        )
        .map_err(IdentityTextPlanError::Projection)?;
    let visible = projection.text();
    let start = char_to_byte(visible, visible_range.start).expect("validated visible start");
    let end = char_to_byte(visible, visible_range.end).expect("validated visible end");
    if !matches!(&visible[start..end], "\n" | "\r\n") || replacement != " " {
        return Err(IdentityTextPlanError::InvalidJoinEdit);
    }
    let joined_line = visible[..start].chars().filter(|ch| *ch == '\n').count();
    if line_at(visible, retained_token.visible_boundary().get()) != joined_line
        || line_at(visible, retired_token.visible_boundary().get()) != joined_line + 1
    {
        return Err(IdentityTextPlanError::InvalidJoinEdit);
    }
    let left = line_content_range(visible, joined_line)?;
    let right = line_content_range(visible, joined_line + 1)?;
    let joined = format!("{}{}{}", &visible[left], replacement, &visible[right]);
    let joined = append_block_id(&joined, 0, retained_id)?;
    let raw_left = line_content_range(canonical_text, joined_line)?;
    let raw_right = line_content_range(canonical_text, joined_line + 1)?;
    let mut planned = canonical_text.to_owned();
    planned.replace_range(raw_left.start..raw_right.end, &joined);

    let edited = replace_chars(visible, visible_range, replacement);
    let mut expected_tokens = projection
        .tokens()
        .iter()
        .map(|token| (token.marker(), token.id()))
        .collect::<Vec<_>>();
    expected_tokens.remove(retired_index);
    validate_planned_text(&planned, &edited, &expected_tokens)?;
    Ok(planned)
}

pub fn plan_split_text(
    canonical_text: &str,
    split_at: usize,
    left_id: &BlockId,
    right_id: &BlockId,
) -> Result<String, IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    if left_id == right_id {
        return Err(IdentityTextPlanError::IdentityMismatch);
    }
    let (left_index, left_token) = unique_block_token(&projection, left_id)?;
    if projection
        .tokens()
        .iter()
        .any(|token| token.id() == right_id.as_str())
    {
        return Err(IdentityTextPlanError::DuplicateIdentity);
    }

    projection
        .lower(
            projection
                .range(split_at, split_at)
                .map_err(IdentityTextPlanError::Projection)?,
        )
        .map_err(IdentityTextPlanError::Projection)?;
    let visible = projection.text();
    let split_byte = char_to_byte(visible, split_at).expect("validated visible split");
    let left_line = visible[..split_byte]
        .chars()
        .filter(|ch| *ch == '\n')
        .count();
    if line_at(visible, left_token.visible_boundary().get()) != left_line {
        return Err(IdentityTextPlanError::InvalidSplitEdit);
    }
    let line_ending = line_ending_at(visible, split_at);
    let line = line_content_range(visible, left_line)?;
    let split = format!(
        "{}{}{}",
        &visible[line.start..split_byte],
        line_ending,
        &visible[split_byte..line.end]
    );
    let split = append_block_id(&split, 0, left_id)?;
    let split = append_block_id(&split, 1, right_id)?;
    let raw_line = line_content_range(canonical_text, left_line)?;
    let mut planned = canonical_text.to_owned();
    planned.replace_range(raw_line, &split);

    let edited = replace_chars(visible, split_at..split_at, line_ending);
    let mut expected_tokens = projection
        .tokens()
        .iter()
        .map(|token| (token.marker(), token.id()))
        .collect::<Vec<_>>();
    expected_tokens.insert(left_index + 1, (BlockIdMarker::Block, right_id.as_str()));
    validate_planned_text(&planned, &edited, &expected_tokens)?;
    Ok(planned)
}

/// Plans join text and identity changes together; neither is applied on error.
pub fn plan_join_edit<'a, R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &'a mut String,
    visible_range: std::ops::Range<usize>,
    replacement: &str,
    retained_id: &BlockId,
    retired_id: &BlockId,
    registry: &'a mut R,
) -> Result<CanonicalIdentityEditPlan<'a, R>, IdentityEditPlanError<R::Error>> {
    let identity = plan_join(page, retained_id, retired_id, registry)
        .map_err(IdentityEditPlanError::Identity)?;
    let planned_text = plan_join_text(
        canonical_text,
        visible_range,
        replacement,
        retained_id,
        retired_id,
    )
    .map_err(IdentityEditPlanError::Text)?;
    Ok(CanonicalIdentityEditPlan {
        canonical_text,
        registry,
        planned_text,
        identity,
    })
}

/// Plans split text and identity changes together; neither is applied on error.
pub fn plan_split_edit<'a, R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &'a mut String,
    split_at: usize,
    left_id: &BlockId,
    right_id: BlockId,
    registry: &'a mut R,
) -> Result<CanonicalIdentityEditPlan<'a, R>, IdentityEditPlanError<R::Error>> {
    let identity = plan_split(page, left_id, right_id.clone(), registry)
        .map_err(IdentityEditPlanError::Identity)?;
    let planned_text = plan_split_text(canonical_text, split_at, left_id, &right_id)
        .map_err(IdentityEditPlanError::Text)?;
    Ok(CanonicalIdentityEditPlan {
        canonical_text,
        registry,
        planned_text,
        identity,
    })
}

/// Applies a fully validated structural edit to registry and canonical text.
pub fn commit_planned_identity_edit<R: BlockIdRegistry>(
    plan: CanonicalIdentityEditPlan<'_, R>,
) -> Result<(), R::Error> {
    commit_identity_edit(
        plan.registry,
        plan.canonical_text,
        plan.planned_text,
        &plan.identity,
    )
}

pub fn plan_delete_block_text(
    canonical_text: &str,
    id: &BlockId,
) -> Result<BlockTextDeletion, IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    let matching_tokens = projection
        .tokens()
        .iter()
        .filter(|token| token.id() == id.as_str())
        .collect::<Vec<_>>();
    if matching_tokens.len() != 1 || matching_tokens[0].marker() != BlockIdMarker::Block {
        return Err(IdentityTextPlanError::IdentityMismatch);
    }

    let token = matching_tokens[0];
    let token_line = line_at(projection.text(), token.visible_boundary().get());
    if projection.tokens().iter().any(|other| {
        !std::ptr::eq(other, token)
            && line_at(projection.text(), other.visible_boundary().get()) == token_line
    }) {
        return Err(IdentityTextPlanError::UnsupportedTokenSet);
    }

    let visible_chars = projection.text().chars().collect::<Vec<_>>();
    let visible_line_start = visible_chars[..token.visible_boundary().get()]
        .iter()
        .rposition(|ch| *ch == '\n')
        .map_or(0, |offset| offset + 1);
    let visible_line_end = visible_chars[token.visible_boundary().get()..]
        .iter()
        .position(|ch| *ch == '\n')
        .map_or(visible_chars.len(), |offset| {
            token.visible_boundary().get() + offset
        });
    let raw_chars = canonical_text.chars().collect::<Vec<_>>();
    let doc = Doc::new(canonical_text);
    let raw_token_start = doc
        .raw_char_offset(token.raw_range().start())
        .expect("token span belongs to canonical text");
    let raw_line_start = raw_chars[..raw_token_start]
        .iter()
        .rposition(|ch| *ch == '\n')
        .map_or(0, |offset| offset + 1);
    let raw_line_end = raw_chars[raw_token_start..]
        .iter()
        .position(|ch| *ch == '\n')
        .map_or(raw_chars.len(), |offset| raw_token_start + offset);
    let raw_has_crlf = raw_line_end < raw_chars.len()
        && raw_line_end > raw_line_start
        && raw_chars[raw_line_end - 1] == '\r';
    let visible_content_end = if raw_has_crlf
        && visible_line_end > visible_line_start
        && visible_chars[visible_line_end - 1] == '\r'
    {
        visible_line_end - 1
    } else {
        visible_line_end
    };
    let visible_text = visible_chars[visible_line_start..visible_content_end]
        .iter()
        .collect::<String>();
    validate_register_text(&visible_text)?;

    let delete_start = if raw_line_end == raw_chars.len() && raw_line_start > 0 {
        previous_line_separator_start(&raw_chars, raw_line_start)
    } else {
        raw_line_start
    };
    let delete_end = if raw_line_end < raw_chars.len() {
        raw_line_end + 1
    } else {
        raw_line_end
    };
    let mut remaining = raw_chars;
    remaining.drain(delete_start..delete_end);
    let canonical_text = remaining.into_iter().collect::<String>();
    let remaining_projection = Doc::new(&canonical_text).visible_projection();
    let mut expected_visible = visible_chars;
    let visible_delete_start =
        if visible_line_end == expected_visible.len() && visible_line_start > 0 {
            previous_line_separator_start(&expected_visible, visible_line_start)
        } else {
            visible_line_start
        };
    let visible_delete_end = if visible_line_end < expected_visible.len() {
        visible_line_end + 1
    } else {
        visible_line_end
    };
    expected_visible.drain(visible_delete_start..visible_delete_end);
    let expected_ids = projection
        .tokens()
        .iter()
        .filter(|other| !std::ptr::eq(*other, token))
        .map(|other| (other.marker(), other.id().to_owned()))
        .collect::<Vec<_>>();
    let remaining_ids = remaining_projection
        .tokens()
        .iter()
        .map(|other| (other.marker(), other.id().to_owned()))
        .collect::<Vec<_>>();
    if remaining_projection.text() != expected_visible.iter().collect::<String>()
        || remaining_ids != expected_ids
    {
        return Err(IdentityTextPlanError::InvalidResult);
    }

    Ok(BlockTextDeletion {
        canonical_text,
        visible_text,
    })
}

pub fn plan_paste_block_text(
    canonical_text: &str,
    insertion_at: usize,
    register: &BlockRegister,
    identity_plan: &IdentityPlan,
) -> Result<String, IdentityTextPlanError> {
    let IdentityDecision::Paste {
        assigned_id: Some(assigned_id),
        ..
    } = &identity_plan.decision
    else {
        return Err(IdentityTextPlanError::InvalidPastePlan);
    };
    let Some(identity) = &register.identity else {
        return Err(IdentityTextPlanError::InvalidPastePlan);
    };
    let valid_change = match (&identity_plan.decision, identity_plan.changes.as_slice()) {
        (
            IdentityDecision::Paste {
                reused_source_id: true,
                ..
            },
            [RegistryChange::MoveDetached {
                id,
                source_page,
                generation,
                ..
            }],
        ) => {
            id == assigned_id
                && source_page == &identity.source_page
                && id == &identity.id
                && identity.operation
                    == (RegisterOperation::Delete {
                        generation: *generation,
                    })
        }
        (
            IdentityDecision::Paste {
                reused_source_id: false,
                ..
            },
            [RegistryChange::Reserve { id, .. }],
        ) => id == assigned_id && id != &identity.id,
        _ => false,
    };
    if !valid_change {
        return Err(IdentityTextPlanError::InvalidPastePlan);
    }
    validate_register_text(&register.visible_text)?;

    let projection = Doc::new(canonical_text).visible_projection();
    if projection
        .tokens()
        .iter()
        .any(|token| token.id() == assigned_id.as_str())
    {
        return Err(IdentityTextPlanError::DuplicateIdentity);
    }
    let range = projection
        .range(insertion_at, insertion_at)
        .map_err(IdentityTextPlanError::Projection)?;
    let lowered = projection
        .lower(range)
        .map_err(IdentityTextPlanError::Projection)?;
    let previous_is_line_end = insertion_at == 0
        || projection
            .text()
            .chars()
            .nth(insertion_at.saturating_sub(1))
            == Some('\n');
    if !previous_is_line_end {
        return Err(IdentityTextPlanError::InvalidPasteLocation);
    }
    let insertion = lowered
        .insertion
        .ok_or(IdentityTextPlanError::InvalidPasteLocation)?;
    let insertion = Doc::new(canonical_text)
        .raw_char_offset(insertion)
        .expect("projection insertion belongs to canonical text");
    let line_ending = line_ending_at(projection.text(), insertion_at);
    let separator = if register.visible_text.is_empty() {
        ""
    } else {
        " "
    };
    let inserted = format!(
        "{}{}^{}{}",
        register.visible_text,
        separator,
        assigned_id.as_str(),
        line_ending
    );
    let mut raw_chars = canonical_text.chars().collect::<Vec<_>>();
    raw_chars.splice(insertion..insertion, inserted.chars());
    let planned = raw_chars.into_iter().collect::<String>();

    let expected_visible = format!(
        "{}{}{}{}",
        &projection.text()
            [..char_to_byte(projection.text(), insertion_at).expect("validated visible insertion")],
        register.visible_text,
        line_ending,
        &projection.text()
            [char_to_byte(projection.text(), insertion_at).expect("validated visible insertion")..]
    );
    let planned_projection = Doc::new(&planned).visible_projection();
    let expected_ids = projection
        .tokens()
        .iter()
        .map(|token| (token.marker(), token.id().to_owned()))
        .collect::<Vec<_>>();
    let planned_existing_ids = planned_projection
        .tokens()
        .iter()
        .filter(|token| token.id() != assigned_id.as_str())
        .map(|token| (token.marker(), token.id().to_owned()))
        .collect::<Vec<_>>();
    let added = planned_projection
        .tokens()
        .iter()
        .filter(|token| token.id() == assigned_id.as_str())
        .collect::<Vec<_>>();
    if planned_projection.text() != expected_visible
        || planned_existing_ids != expected_ids
        || added.len() != 1
        || added[0].marker() != BlockIdMarker::Block
    {
        return Err(IdentityTextPlanError::InvalidResult);
    }
    Ok(planned)
}

/// Plans deletion and keeps the register private until the edit commits.
pub fn plan_delete_edit<'a, R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &'a mut String,
    id: &BlockId,
    registry: &'a mut R,
) -> Result<CanonicalBlockDeletePlan<'a, R>, IdentityEditPlanError<R::Error>> {
    let deletion =
        plan_delete_block_text(canonical_text, id).map_err(IdentityEditPlanError::Text)?;
    let (register, identity) = plan_delete(page, id, deletion.visible_text, registry)
        .map_err(IdentityEditPlanError::Identity)?;
    Ok(CanonicalBlockDeletePlan {
        edit: CanonicalIdentityEditPlan {
            canonical_text,
            registry,
            planned_text: deletion.canonical_text,
            identity,
        },
        register,
    })
}

/// Commits a block deletion and returns its register only after success.
pub fn commit_planned_block_delete<R: BlockIdRegistry>(
    plan: CanonicalBlockDeletePlan<'_, R>,
) -> Result<BlockRegister, R::Error> {
    let CanonicalBlockDeletePlan { edit, register } = plan;
    commit_planned_identity_edit(edit)?;
    Ok(register)
}

/// Plans a linewise paste against its exact destination and registry.
pub fn plan_paste_edit<'a, R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &'a mut String,
    insertion_at: usize,
    register: &BlockRegister,
    fresh_id: Option<BlockId>,
    registry: &'a mut R,
) -> Result<CanonicalIdentityEditPlan<'a, R>, IdentityEditPlanError<R::Error>> {
    let identity =
        plan_paste(register, page, fresh_id, registry).map_err(IdentityEditPlanError::Identity)?;
    let planned_text = plan_paste_block_text(canonical_text, insertion_at, register, &identity)
        .map_err(IdentityEditPlanError::Text)?;
    Ok(CanonicalIdentityEditPlan {
        canonical_text,
        registry,
        planned_text,
        identity,
    })
}

fn validate_planned_text(
    canonical_text: &str,
    visible_text: &str,
    expected_tokens: &[(BlockIdMarker, &str)],
) -> Result<(), IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    if projection.text() != visible_text
        || projection.tokens().len() != expected_tokens.len()
        || projection
            .tokens()
            .iter()
            .zip(expected_tokens)
            .any(|(token, (marker, id))| token.marker() != *marker || token.id() != *id)
    {
        return Err(IdentityTextPlanError::InvalidResult);
    }
    Ok(())
}

fn unique_block_token<'a>(
    projection: &'a VisibleProjection,
    id: &BlockId,
) -> Result<(usize, &'a BlockIdToken), IdentityTextPlanError> {
    let mut matches =
        projection.tokens().iter().enumerate().filter(|(_, token)| {
            token.marker() == BlockIdMarker::Block && token.id() == id.as_str()
        });
    let found = matches
        .next()
        .ok_or(IdentityTextPlanError::IdentityMismatch)?;
    if matches.next().is_some() {
        return Err(IdentityTextPlanError::IdentityMismatch);
    }
    Ok(found)
}

fn line_at(text: &str, char_offset: usize) -> usize {
    text.chars()
        .take(char_offset)
        .filter(|ch| *ch == '\n')
        .count()
}

fn validate_register_text(text: &str) -> Result<(), IdentityTextPlanError> {
    if text
        .chars()
        .any(|character| matches!(character, '\n' | '\r'))
    {
        return Err(IdentityTextPlanError::InvalidRegisterText);
    }
    Ok(())
}

fn previous_line_separator_start(chars: &[char], line_start: usize) -> usize {
    if line_start >= 2 && chars[line_start - 1] == '\n' && chars[line_start - 2] == '\r' {
        line_start - 2
    } else {
        line_start - 1
    }
}

fn replace_chars(text: &str, range: std::ops::Range<usize>, replacement: &str) -> String {
    let start = char_to_byte(text, range.start).expect("validated visible start");
    let end = char_to_byte(text, range.end).expect("validated visible end");
    let mut result = text.to_owned();
    result.replace_range(start..end, replacement);
    result
}

fn append_block_id(
    text: &str,
    line_index: usize,
    id: &BlockId,
) -> Result<String, IdentityTextPlanError> {
    let line = line_content_range(text, line_index)?;
    let content_end = if line.end == text.len()
        && line.end > line.start
        && text.as_bytes()[line.end - 1] == b'\r'
    {
        line.end - 1
    } else {
        line.end
    };
    let prefix = if content_end == line.start { "" } else { " " };
    let insertion = format!("{prefix}^{}", id.as_str());
    let mut result = text.to_owned();
    result.insert_str(content_end, &insertion);
    Ok(result)
}

fn line_content_range(
    text: &str,
    line_index: usize,
) -> Result<std::ops::Range<usize>, IdentityTextPlanError> {
    let start = if line_index == 0 {
        0
    } else {
        text.match_indices('\n')
            .nth(line_index - 1)
            .map(|(byte, _)| byte + 1)
            .ok_or(IdentityTextPlanError::LineOutOfBounds(line_index))?
    };
    let end = match text[start..].find('\n') {
        Some(offset) => {
            let end = start + offset;
            if end > start && text.as_bytes()[end - 1] == b'\r' {
                end - 1
            } else {
                end
            }
        }
        None => text.len(),
    };
    Ok(start..end)
}

fn char_to_byte(text: &str, char_offset: usize) -> Option<usize> {
    if char_offset == text.chars().count() {
        Some(text.len())
    } else {
        text.char_indices().nth(char_offset).map(|(byte, _)| byte)
    }
}

fn line_ending_at(text: &str, char_offset: usize) -> &'static str {
    let chars = text.chars().collect::<Vec<_>>();
    if let Some(offset) = chars[char_offset..].iter().position(|ch| *ch == '\n') {
        return if offset > 0 && chars[char_offset + offset - 1] == '\r' {
            "\r\n"
        } else {
            "\n"
        };
    }

    if let Some(offset) = chars[..char_offset].iter().rposition(|ch| *ch == '\n') {
        return if offset > 0 && chars[offset - 1] == '\r' {
            "\r\n"
        } else {
            "\n"
        };
    }

    "\n"
}

/// Returns only ordinary block IDs; legacy mirror tokens are preserved by the
/// projection but do not participate in the new identity registry.
pub fn block_ids_in_text(text: &str) -> Vec<BlockId> {
    Doc::new(text)
        .visible_projection()
        .tokens()
        .iter()
        .filter(|token| token.marker() == BlockIdMarker::Block)
        .filter_map(|token| BlockId::new(token.id()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;

    use super::{
        append_block_id, begin_visible_edit, block_ids_in_text, commit_planned_block_delete,
        commit_planned_identity_edit, commit_planned_visible_edit, plan_delete,
        plan_delete_block_text, plan_delete_edit, plan_discard_register, plan_join, plan_join_edit,
        plan_join_text, plan_paste, plan_paste_edit, plan_split, plan_split_edit, plan_split_text,
        validate_visible_edit_result, BlockId, BlockIdRegistry, BlockRegister,
        CanonicalVisibleEditPlan, DetachGeneration, IdentityDecision, IdentityEditPlanError,
        IdentityPlanError, IdentityState, IdentityTextPlanError, PageId, RegisterAliasReplaceError,
        RegisterBank, RegisterCleanupError, RegisterClearError, RegisterIdentity,
        RegisterOperation, RegisterSlotError, RegistryChange,
    };
    use bloom_text::{BlockIdMarker, Doc, ProjectionError};

    const ADR3_IDENTITY_ORACLE: &str =
        include_str!("../../../../testdata/adr3_vim_identity_oracle.v1");
    const ADR3_EDITOR_REPEAT_ORACLE: &str =
        include_str!("../../../../testdata/adr3_editor_repeat.v1");

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct FakeRegistry {
        live: BTreeMap<BlockId, (PageId, DetachGeneration)>,
        detached: BTreeMap<BlockId, (PageId, DetachGeneration)>,
        retired: BTreeSet<BlockId>,
        fail_next_apply: bool,
        fail_state_reads: bool,
    }

    impl FakeRegistry {
        fn seed(&mut self, id: BlockId, page: PageId) {
            self.live.insert(id, (page, DetachGeneration::INITIAL));
        }
    }

    impl BlockIdRegistry for FakeRegistry {
        type Error = &'static str;

        fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error> {
            if self.fail_state_reads {
                return Err("registry state read failed");
            }
            Ok(if let Some((page, generation)) = self.live.get(id) {
                IdentityState::Live {
                    page: page.clone(),
                    generation: *generation,
                }
            } else if let Some((page, generation)) = self.detached.get(id) {
                IdentityState::Detached {
                    page: page.clone(),
                    generation: *generation,
                }
            } else if self.retired.contains(id) {
                IdentityState::Retired
            } else {
                IdentityState::Available
            })
        }

        fn apply(&mut self, changes: &[RegistryChange]) -> Result<(), Self::Error> {
            if self.fail_next_apply {
                return Err("registry transaction failed");
            }

            let mut staged = self.clone();
            for change in changes {
                match change {
                    RegistryChange::AssertLive {
                        id,
                        page,
                        generation,
                    } => {
                        if staged.state(id)?
                            != (IdentityState::Live {
                                page: page.clone(),
                                generation: *generation,
                            })
                        {
                            return Err("identity is not live at the expected page and generation");
                        }
                    }
                    RegistryChange::Reserve { id, page } => {
                        if staged.state(id)? != IdentityState::Available {
                            return Err("identity is not available");
                        }
                        staged
                            .live
                            .insert(id.clone(), (page.clone(), DetachGeneration::INITIAL));
                    }
                    RegistryChange::Detach {
                        id,
                        page,
                        previous_generation,
                    } => {
                        if staged.state(id)?
                            != (IdentityState::Live {
                                page: page.clone(),
                                generation: *previous_generation,
                            })
                        {
                            return Err("identity is not live at the expected page and generation");
                        }
                        let generation = previous_generation
                            .next()
                            .ok_or("detach generation exhausted")?;
                        staged.live.remove(id);
                        staged
                            .detached
                            .insert(id.clone(), (page.clone(), generation));
                    }
                    RegistryChange::MoveDetached {
                        id,
                        source_page,
                        target_page,
                        generation,
                    } => {
                        if staged.state(id)?
                            != (IdentityState::Detached {
                                page: source_page.clone(),
                                generation: *generation,
                            })
                        {
                            return Err(
                                "identity is not detached at the expected page and generation",
                            );
                        }
                        staged.detached.remove(id);
                        staged
                            .live
                            .insert(id.clone(), (target_page.clone(), *generation));
                    }
                    RegistryChange::RetireDetached {
                        id,
                        page,
                        generation,
                    } => {
                        if staged.state(id)?
                            != (IdentityState::Detached {
                                page: page.clone(),
                                generation: *generation,
                            })
                        {
                            return Err(
                                "identity is not detached at the expected page and generation",
                            );
                        }
                        staged.detached.remove(id);
                        staged.retired.insert(id.clone());
                    }
                    RegistryChange::Retire {
                        id,
                        page,
                        generation,
                    } => {
                        if staged.state(id)?
                            != (IdentityState::Live {
                                page: page.clone(),
                                generation: *generation,
                            })
                        {
                            return Err("identity is not live at the expected page and generation");
                        }
                        staged.live.remove(id);
                        staged.retired.insert(id.clone());
                    }
                }
            }
            *self = staged;
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct SharedRegistry(Rc<RefCell<FakeRegistry>>);

    impl BlockIdRegistry for SharedRegistry {
        type Error = &'static str;

        fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error> {
            self.0.borrow().state(id)
        }

        fn apply(&mut self, changes: &[RegistryChange]) -> Result<(), Self::Error> {
            self.0.borrow_mut().apply(changes)
        }
    }

    fn page(value: &str) -> PageId {
        PageId::new(value).unwrap()
    }

    fn id(value: &str) -> BlockId {
        BlockId::new(value).unwrap()
    }

    fn live(page: PageId, generation: u64) -> IdentityState {
        IdentityState::Live {
            page,
            generation: DetachGeneration::new(generation),
        }
    }

    fn detached(page: PageId, generation: u64) -> IdentityState {
        IdentityState::Detached {
            page,
            generation: DetachGeneration::new(generation),
        }
    }

    fn delete_operation(generation: u64) -> RegisterOperation {
        RegisterOperation::Delete {
            generation: DetachGeneration::new(generation),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum OwnershipModelState {
        Live(u64),
        Detached(u64),
        Retired,
    }

    fn model_registry_state(state: OwnershipModelState, source_page: &PageId) -> IdentityState {
        match state {
            OwnershipModelState::Live(generation) => live(source_page.clone(), generation),
            OwnershipModelState::Detached(generation) => detached(source_page.clone(), generation),
            OwnershipModelState::Retired => IdentityState::Retired,
        }
    }

    fn model_paste_decision(
        state: OwnershipModelState,
        captured_generation: u64,
        moved_id: &BlockId,
    ) -> IdentityDecision {
        let can_move = state == OwnershipModelState::Detached(captured_generation);
        IdentityDecision::Paste {
            assigned_id: Some(if can_move {
                moved_id.clone()
            } else {
                id("fresh1")
            }),
            reused_source_id: can_move,
        }
    }

    fn register(value: &str, operation: RegisterOperation) -> BlockRegister {
        BlockRegister {
            visible_text: "block text".to_owned(),
            identity: Some(RegisterIdentity {
                id: id(value),
                source_page: page("source"),
                operation,
            }),
        }
    }

    fn detach_and_return<R: BlockIdRegistry<Error = &'static str>>(
        registry: &mut R,
        source_page: &PageId,
        block_id: &BlockId,
    ) {
        let (register, delete) =
            plan_delete(source_page, block_id, "one".to_owned(), registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let paste = plan_paste(&register, source_page, None, registry).unwrap();
        registry.apply(&paste.changes).unwrap();
    }

    fn oracle_case(name: &str) -> Vec<&str> {
        ADR3_IDENTITY_ORACLE
            .lines()
            .find(|line| line.starts_with(&format!("{name}|")))
            .unwrap_or_else(|| panic!("missing identity oracle case {name}"))
            .split('|')
            .collect()
    }

    fn decode_oracle_text(text: &str) -> String {
        text.replace("\\n", "\n").replace("\\r", "\r")
    }

    fn oracle_edit(encoded: &str) -> (usize, usize, String) {
        let fields = encoded.splitn(4, ',').collect::<Vec<_>>();
        assert_eq!(fields.len(), 4);
        (
            fields[0].parse().unwrap(),
            fields[1].parse().unwrap(),
            decode_oracle_text(fields[2]),
        )
    }

    fn plan_test_visible_edit<'a>(
        canonical: &'a mut String,
        range: std::ops::Range<usize>,
        replacement: &str,
    ) -> Result<CanonicalVisibleEditPlan<'a>, IdentityTextPlanError> {
        let context = begin_visible_edit(canonical);
        let range = context
            .projection()
            .range(range.start, range.end)
            .map_err(IdentityTextPlanError::Projection)?;
        context.plan(range, replacement)
    }

    #[test]
    fn recorded_vim_boundary_edits_commit_without_changing_token_bytes() {
        for (name, expected_raw) in [
            ("x", "hell ^edge01"),
            ("X", "hell ^edge01"),
            ("D", "h ^edge01"),
        ] {
            let fields = oracle_case(name);
            let mut canonical = decode_oracle_text(fields[1]);
            let (start, end, replacement) = oracle_edit(fields[6]);
            let plan = plan_test_visible_edit(&mut canonical, start..end, &replacement).unwrap();
            commit_planned_visible_edit(plan);

            assert_eq!(canonical, expected_raw, "{name}");
            assert_eq!(
                Doc::new(&canonical).visible_projection().text(),
                decode_oracle_text(fields[5]),
                "{name}"
            );
        }
    }

    #[test]
    fn editor_observed_repeat_deletions_preserve_canonical_token_bytes() {
        fn tokens(canonical: &str) -> Vec<(BlockIdMarker, String, String)> {
            let doc = Doc::new(canonical);
            let projection = doc.visible_projection();
            let chars = canonical.chars().collect::<Vec<_>>();
            projection
                .tokens()
                .iter()
                .map(|token| {
                    let start = doc.raw_char_offset(token.raw_range().start()).unwrap();
                    let end = doc.raw_char_offset(token.raw_range().end()).unwrap();
                    (
                        token.marker(),
                        token.id().to_owned(),
                        chars[start..end].iter().collect(),
                    )
                })
                .collect()
        }

        let mut lines = ADR3_EDITOR_REPEAT_ORACLE.lines();
        assert_eq!(lines.next(), Some("BLOOM_EDITOR_REPEAT_ORACLE_V1"));
        let mut cases = 0;
        let mut edits = 0;
        let mut no_ops = 0;
        for line in lines.filter(|line| !line.is_empty()) {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 5);
            let name = fields[0];
            let mut canonical = decode_oracle_text(fields[1]);
            assert_eq!(
                Doc::new(&canonical).visible_projection().text(),
                decode_oracle_text(fields[2]),
                "{name}: initial projection"
            );
            let initial_tokens = tokens(&canonical);
            assert_eq!(initial_tokens.len(), 2, "{name}");
            for step in fields[4].split(';') {
                let fields = step.splitn(4, ',').collect::<Vec<_>>();
                assert_eq!(fields.len(), 4, "{name}: invalid step");
                let expected_after = decode_oracle_text(fields[3]);
                if fields[1] == "-" {
                    assert_eq!(fields[2], "-", "{name}: invalid no-op");
                    no_ops += 1;
                } else {
                    let start = fields[1].parse::<usize>().unwrap();
                    let end = fields[2].parse::<usize>().unwrap();
                    let plan = plan_test_visible_edit(&mut canonical, start..end, "").unwrap();
                    commit_planned_visible_edit(plan);
                    edits += 1;
                }
                assert_eq!(
                    Doc::new(&canonical).visible_projection().text(),
                    expected_after,
                    "{name}: {}",
                    fields[0]
                );
                assert_eq!(tokens(&canonical), initial_tokens, "{name}: {}", fields[0]);
            }
            cases += 1;
        }
        assert_eq!((cases, edits, no_ops), (3, 6, 2));
    }

    #[test]
    fn visible_edits_match_a_line_local_reference_model() {
        let payload = "aβe\u{301}👩\u{200d}💻z";
        let chars = payload.chars().collect::<Vec<_>>();
        let local = Doc::new(payload).visible_projection();
        let mut cases = 0;
        for ending in ["\n", "\r\n"] {
            for tail in ["", ending] {
                let raw_prefix = format!("other ^anchor{ending}");
                let visible_prefix_len = format!("other{ending}").chars().count();
                for start in 0..=chars.len() {
                    for end in start..=chars.len() {
                        if local.range(start, end).is_err() {
                            continue;
                        }
                        for replacement in ["", "X", "λ", "e\u{301}"] {
                            let edited_line = chars[..start]
                                .iter()
                                .copied()
                                .chain(replacement.chars())
                                .chain(chars[end..].iter().copied())
                                .collect::<String>();
                            let suffix = format!(
                                " ^current \t{ending}mirror ^=current{ending}after ^other2{tail}"
                            );
                            let mut canonical = format!("{raw_prefix}{payload}{suffix}");
                            let expected = format!("{raw_prefix}{edited_line}{suffix}");
                            let plan = plan_test_visible_edit(
                                &mut canonical,
                                visible_prefix_len + start..visible_prefix_len + end,
                                replacement,
                            )
                            .unwrap();
                            commit_planned_visible_edit(plan);
                            assert_eq!(
                                canonical, expected,
                                "ending={ending:?}, tail={tail:?}, range={start}..{end}, replacement={replacement:?}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 336);
    }

    #[test]
    fn visible_replacement_can_span_multiple_raw_segments_around_a_token() {
        let mut canonical = "abc ^id \t\r\nnext ^=id\r\n".to_owned();
        let projection = Doc::new(&canonical).visible_projection();
        let lowered = projection.lower(projection.range(2, 5).unwrap()).unwrap();
        assert_eq!(lowered.raw_segments.len(), 2);

        let plan = plan_test_visible_edit(&mut canonical, 2..5, "X").unwrap();
        commit_planned_visible_edit(plan);
        assert_eq!(canonical, "abX ^id\r\nnext ^=id\r\n");
    }

    #[test]
    fn successive_visible_edits_reproject_each_document_version() {
        let mut canonical = "one ^first\nβe\u{301} ^second\r\n".to_owned();
        let steps = [
            (0..3, "ONE", "ONE ^first\nβe\u{301} ^second\r\n"),
            (7..7, "!", "ONE ^first\nβe\u{301}! ^second\r\n"),
            (4..5, "", "ONE ^first\ne\u{301}! ^second\r\n"),
            (0..3, "hi", "hi ^first\ne\u{301}! ^second\r\n"),
        ];
        for (range, replacement, expected) in steps {
            let plan = plan_test_visible_edit(&mut canonical, range, replacement).unwrap();
            commit_planned_visible_edit(plan);
            assert_eq!(canonical, expected);
        }
    }

    #[test]
    fn visible_edits_preserve_literal_markers_inside_longer_code_fences() {
        let mut canonical = "````\n```\ninside ^literal\n````\noutside ^real\n".to_owned();
        let context = begin_visible_edit(&mut canonical);
        let visible = context.projection().text();
        let start = visible[..visible.find("^literal").unwrap()].chars().count();
        let range = context
            .projection()
            .range(start, start + "^literal".chars().count())
            .unwrap();
        let plan = context.plan(range, "^note").unwrap();
        commit_planned_visible_edit(plan);

        assert_eq!(canonical, "````\n```\ninside ^note\n````\noutside ^real\n");
        let projection = Doc::new(&canonical).visible_projection();
        assert!(projection.text().contains("inside ^note"));
        assert_eq!(projection.tokens().len(), 1);
        assert_eq!(projection.tokens()[0].id(), "real");
    }

    #[test]
    fn visible_edits_reject_unsafe_results_without_mutating_the_document() {
        let cases = [
            (
                "hello ^keep",
                0..0,
                "`",
                IdentityTextPlanError::InvalidResult,
            ),
            (
                "hello",
                5..5,
                " ^literal",
                IdentityTextPlanError::InvalidResult,
            ),
            (
                "safe\nline ^id\n",
                0..4,
                "```",
                IdentityTextPlanError::InvalidResult,
            ),
            (
                "```rust\ntext ^fake\n```\nlast ^keep\n",
                0..1,
                "",
                IdentityTextPlanError::InvalidResult,
            ),
            (
                "````\n```\ninside ^literal\n````\noutside ^real\n",
                0..1,
                "",
                IdentityTextPlanError::InvalidResult,
            ),
            (
                "left ^one\nright ^two",
                4..5,
                "",
                IdentityTextPlanError::RequiresStructuralEdit,
            ),
            (
                "a ^one\r\n",
                1..3,
                "",
                IdentityTextPlanError::RequiresStructuralEdit,
            ),
            (
                "a ^one",
                0..0,
                "\r",
                IdentityTextPlanError::RequiresStructuralEdit,
            ),
            (
                "e\u{301} ^id",
                1..1,
                "x",
                IdentityTextPlanError::Projection(ProjectionError::SplitsGrapheme { offset: 1 }),
            ),
            (
                "^id\n",
                0..0,
                "x",
                IdentityTextPlanError::Projection(ProjectionError::InsertionAtLeadingToken {
                    token_index: 0,
                }),
            ),
            (
                "x",
                0..2,
                "",
                IdentityTextPlanError::Projection(ProjectionError::InvalidVisibleRange {
                    start: 0,
                    end: 2,
                    length: 1,
                }),
            ),
        ];
        for (source, range, replacement, expected_error) in cases {
            let mut canonical = source.to_owned();
            let actual = plan_test_visible_edit(&mut canonical, range, replacement)
                .err()
                .expect("unsafe edit must fail");
            assert_eq!(actual, expected_error, "{source:?}");
            assert_eq!(canonical, source);
        }
    }

    #[test]
    fn discarded_and_empty_visible_plans_leave_canonical_bytes_unchanged() {
        let mut canonical = "hello ^id\n".to_owned();
        let plan = plan_test_visible_edit(&mut canonical, 0..5, "world").unwrap();
        drop(plan);
        assert_eq!(canonical, "hello ^id\n");

        let mut id_only = "^id\n".to_owned();
        let plan = plan_test_visible_edit(&mut id_only, 0..0, "").unwrap();
        commit_planned_visible_edit(plan);
        assert_eq!(id_only, "^id\n");
    }

    #[test]
    fn visible_edits_reject_ranges_from_another_or_stale_projection() {
        for change_text in [false, true] {
            let mut canonical = "hello ^id\n".to_owned();
            let old_projection = Doc::new(&canonical).visible_projection();
            let old_range = old_projection.range(1, 2).unwrap();
            if change_text {
                canonical.insert(0, 'X');
            }
            let before = canonical.clone();
            let context = begin_visible_edit(&mut canonical);
            let actual = context
                .plan(old_range, "y")
                .err()
                .expect("foreign projection range must fail");

            assert_eq!(
                actual,
                IdentityTextPlanError::Projection(ProjectionError::ForeignVisibleRange)
            );
            assert_eq!(canonical, before);
        }

        let mut id_only = "^id\n".to_owned();
        let foreign = Doc::new(&id_only).visible_projection();
        let range = foreign.range(0, 0).unwrap();
        let context = begin_visible_edit(&mut id_only);
        assert_eq!(
            context.plan(range, "").err(),
            Some(IdentityTextPlanError::Projection(
                ProjectionError::ForeignVisibleRange
            ))
        );
        assert_eq!(id_only, "^id\n");
    }

    #[test]
    fn visible_edit_validation_detects_token_reattachment_and_marker_changes() {
        let original = Doc::new("a ^same\n\n");
        let projection = original.visible_projection();
        let reattached = "a\n^same\n";
        let altered_marker = "a ^=same\n\n";
        for candidate in [reattached, altered_marker] {
            let candidate_projection = Doc::new(candidate).visible_projection();
            assert_eq!(candidate_projection.text(), projection.text());
            assert_eq!(candidate_projection.tokens()[0].id(), "same");
            assert_eq!(
                validate_visible_edit_result(
                    &original,
                    &projection,
                    candidate,
                    projection.text(),
                    &[],
                    None,
                    0,
                ),
                Err(IdentityTextPlanError::InvalidResult)
            );
        }
    }

    #[test]
    fn delete_then_paste_moves_the_detached_identity_to_the_target_page() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("move01"), page("source"));
        let (deleted, delete_plan) = plan_delete(
            &page("source"),
            &id("move01"),
            "block text".to_owned(),
            &registry,
        )
        .unwrap();
        assert_eq!(
            delete_plan.decision,
            IdentityDecision::Delete {
                detached_id: id("move01"),
            }
        );
        registry.apply(&delete_plan.changes).unwrap();
        assert_eq!(
            registry.state(&id("move01")),
            Ok(detached(page("source"), 1))
        );

        let plan = plan_paste(&deleted, &page("target"), None, &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("move01")),
                reused_source_id: true,
            }
        );
        registry.apply(&plan.changes).unwrap();
        assert_eq!(registry.state(&id("move01")), Ok(live(page("target"), 1)));
    }

    #[test]
    fn discarding_an_unpasted_delete_register_retires_its_detached_identity() {
        let source_page = page("source");
        let deleted_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(deleted_id.clone(), source_page.clone());
        let (register, delete_plan) = plan_delete(
            &source_page,
            &deleted_id,
            "block text".to_owned(),
            &registry,
        )
        .unwrap();
        registry.apply(&delete_plan.changes).unwrap();

        let discard_plan = plan_discard_register(&register, &registry).unwrap();
        assert_eq!(
            discard_plan.decision,
            IdentityDecision::DiscardRegister {
                retired_id: Some(deleted_id.clone())
            }
        );
        registry.apply(&discard_plan.changes).unwrap();

        assert_eq!(registry.state(&deleted_id), Ok(IdentityState::Retired));
        assert_eq!(
            plan_paste(&register, &page("target"), Some(id("fresh1")), &registry)
                .unwrap()
                .decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("fresh1")),
                reused_source_id: false,
            }
        );
    }

    #[test]
    fn discarding_a_consumed_move_register_keeps_its_live_identity() {
        let source_page = page("source");
        let target_page = page("target");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete_plan) =
            plan_delete(&source_page, &moved_id, "block text".to_owned(), &registry).unwrap();
        registry.apply(&delete_plan.changes).unwrap();
        let paste_plan = plan_paste(&register, &target_page, None, &registry).unwrap();
        registry.apply(&paste_plan.changes).unwrap();

        let discard_plan = plan_discard_register(&register, &registry).unwrap();

        assert_eq!(
            discard_plan.decision,
            IdentityDecision::DiscardRegister { retired_id: None }
        );
        assert!(discard_plan.changes.is_empty());
        assert_eq!(registry.state(&moved_id), Ok(live(target_page, 1)));
    }

    #[test]
    fn discarding_yank_register_does_not_change_live_identity_state() {
        let original_id = id("copy01");
        let mut registry = FakeRegistry::default();
        registry.seed(original_id.clone(), page("source"));

        let plan =
            plan_discard_register(&register("copy01", RegisterOperation::Yank), &registry).unwrap();

        assert_eq!(
            plan.decision,
            IdentityDecision::DiscardRegister { retired_id: None }
        );
        assert!(plan.changes.is_empty());
        assert_eq!(registry.state(&original_id), Ok(live(page("source"), 0)));
    }

    #[test]
    fn discarding_a_stale_move_register_rejects_another_pages_detached_id() {
        let move_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry
            .detached
            .insert(move_id.clone(), (page("other"), DetachGeneration::new(1)));

        assert_eq!(
            plan_discard_register(&register("move01", delete_operation(1)), &registry),
            Err(IdentityPlanError::DetachedRegisterPageMismatch(move_id))
        );
    }

    #[test]
    fn old_register_cannot_claim_or_retire_a_later_detachment_on_the_same_page() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (old_register, first_delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&first_delete.changes).unwrap();
        let stale_move = plan_paste(&old_register, &page("target"), None, &registry).unwrap();
        let stale_retire = plan_discard_register(&old_register, &registry).unwrap();
        let first_paste = plan_paste(&old_register, &source_page, None, &registry).unwrap();
        registry.apply(&first_paste.changes).unwrap();
        let (_new_register, second_delete) =
            plan_delete(&source_page, &moved_id, "two".to_owned(), &registry).unwrap();
        registry.apply(&second_delete.changes).unwrap();
        let before = registry.clone();

        assert_eq!(
            plan_paste(
                &old_register,
                &page("target"),
                Some(id("fresh1")),
                &registry
            )
            .unwrap()
            .decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("fresh1")),
                reused_source_id: false,
            }
        );
        assert_eq!(
            plan_discard_register(&old_register, &registry)
                .unwrap()
                .decision,
            IdentityDecision::DiscardRegister { retired_id: None }
        );
        assert!(registry.apply(&stale_move.changes).is_err());
        assert!(registry.apply(&stale_retire.changes).is_err());
        assert_eq!(registry, before);
    }

    #[test]
    fn old_register_can_be_cleared_after_a_later_detachment_on_another_page() {
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), page("source"));
        let (old_register, first_delete) =
            plan_delete(&page("source"), &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&first_delete.changes).unwrap();
        let paste = plan_paste(&old_register, &page("target"), None, &registry).unwrap();
        registry.apply(&paste.changes).unwrap();
        let (_new_register, second_delete) =
            plan_delete(&page("target"), &moved_id, "two".to_owned(), &registry).unwrap();
        registry.apply(&second_delete.changes).unwrap();
        let before = registry.clone();
        let mut bank = RegisterBank::default();
        bank.insert('a', old_register).unwrap();

        bank.clear('a', &mut registry).unwrap();
        assert!(bank.get('a').is_none());
        assert_eq!(registry, before);
        assert_eq!(registry.state(&moved_id), Ok(detached(page("target"), 2)));
    }

    #[test]
    fn register_bank_retains_a_detached_identity_until_the_last_alias_is_cleared() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let mut bank = RegisterBank::default();
        bank.insert('"', register).unwrap();
        bank.alias('"', 'a').unwrap();
        registry.fail_next_apply = true;

        bank.clear('"', &mut registry).unwrap();
        assert!(bank.get('"').is_none());
        assert!(bank.get('a').is_some());
        assert_eq!(
            registry.state(&moved_id),
            Ok(detached(source_page.clone(), 1))
        );
        let before = registry.clone();
        assert_eq!(
            bank.clear('a', &mut registry),
            Err(RegisterClearError::Registry("registry transaction failed"))
        );
        assert!(bank.get('a').is_some());
        assert_eq!(registry, before);

        registry.fail_next_apply = false;
        bank.clear('a', &mut registry).unwrap();
        assert!(bank.get('a').is_none());
        assert_eq!(registry.state(&moved_id), Ok(IdentityState::Retired));
    }

    #[test]
    fn a_surviving_register_alias_can_move_the_original_identity() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let mut bank = RegisterBank::default();
        bank.insert('"', register).unwrap();
        bank.alias('"', 'a').unwrap();
        bank.clear('"', &mut registry).unwrap();

        let paste = plan_paste(bank.get('a').unwrap(), &page("target"), None, &registry).unwrap();
        assert_eq!(
            paste.decision,
            IdentityDecision::Paste {
                assigned_id: Some(moved_id.clone()),
                reused_source_id: true,
            }
        );
        registry.apply(&paste.changes).unwrap();
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(registry.state(&moved_id), Ok(live(page("target"), 1)));
    }

    #[test]
    fn clearing_old_aliases_does_not_retire_a_newer_detachment() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (old_register, first_delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&first_delete.changes).unwrap();
        let mut bank = RegisterBank::default();
        bank.insert('"', old_register).unwrap();
        bank.alias('"', 'a').unwrap();
        let paste = plan_paste(bank.get('"').unwrap(), &source_page, None, &registry).unwrap();
        registry.apply(&paste.changes).unwrap();
        let (new_register, second_delete) =
            plan_delete(&source_page, &moved_id, "two".to_owned(), &registry).unwrap();
        registry.apply(&second_delete.changes).unwrap();
        bank.insert('b', new_register).unwrap();

        bank.clear('"', &mut registry).unwrap();
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(
            registry.state(&moved_id),
            Ok(detached(source_page.clone(), 2))
        );
        bank.clear('b', &mut registry).unwrap();
        assert_eq!(registry.state(&moved_id), Ok(IdentityState::Retired));
    }

    #[test]
    fn register_cleanup_revalidates_ownership_when_the_registry_changes_during_apply() {
        struct RacingRegistry {
            inner: FakeRegistry,
            moved_id: BlockId,
            source_page: PageId,
            interfere: bool,
        }

        impl BlockIdRegistry for RacingRegistry {
            type Error = &'static str;

            fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error> {
                self.inner.state(id)
            }

            fn apply(&mut self, changes: &[RegistryChange]) -> Result<(), Self::Error> {
                if self.interfere {
                    self.interfere = false;
                    let generation = DetachGeneration::new(1);
                    self.inner.apply(&[
                        RegistryChange::MoveDetached {
                            id: self.moved_id.clone(),
                            source_page: self.source_page.clone(),
                            target_page: self.source_page.clone(),
                            generation,
                        },
                        RegistryChange::Detach {
                            id: self.moved_id.clone(),
                            page: self.source_page.clone(),
                            previous_generation: generation,
                        },
                    ])?;
                }
                self.inner.apply(changes)
            }
        }

        let source_page = page("source");
        let moved_id = id("move01");
        let mut inner = FakeRegistry::default();
        inner.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &inner).unwrap();
        inner.apply(&delete.changes).unwrap();
        let mut registry = RacingRegistry {
            inner,
            moved_id: moved_id.clone(),
            source_page: source_page.clone(),
            interfere: true,
        };
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();

        assert_eq!(
            bank.clear('a', &mut registry),
            Err(RegisterClearError::Registry(
                "identity is not detached at the expected page and generation"
            ))
        );
        assert!(bank.get('a').is_some());
        assert_eq!(
            registry.state(&moved_id),
            Ok(detached(source_page.clone(), 2))
        );
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(registry.state(&moved_id), Ok(detached(source_page, 2)));

        let source_page = page("source");
        let mut inner = FakeRegistry::default();
        let register = detached_register(&mut inner, &moved_id, &source_page, "old");
        let mut registry = RacingRegistry {
            inner,
            moved_id: moved_id.clone(),
            source_page: source_page.clone(),
            interfere: true,
        };
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();
        let incoming = BlockRegister::without_identity("new".to_owned());

        let failure = bank.replace('a', incoming, &mut registry).unwrap_err();
        assert_eq!(
            failure.error,
            RegisterCleanupError::Registry(
                "identity is not detached at the expected page and generation"
            )
        );
        assert_eq!(bank.get('a').unwrap().visible_text(), "old");
        assert_eq!(failure.register.visible_text(), "new");
        assert_eq!(
            registry.state(&moved_id),
            Ok(detached(source_page.clone(), 2))
        );
        bank.replace('a', failure.register, &mut registry).unwrap();
        assert_eq!(bank.get('a').unwrap().visible_text(), "new");
        assert_eq!(registry.state(&moved_id), Ok(detached(source_page, 2)));
    }

    #[test]
    fn missing_delete_identity_is_reported_without_clearing_its_register() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        registry.detached.remove(&moved_id);
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();

        assert_eq!(
            plan_paste(
                bank.get('a').unwrap(),
                &page("target"),
                Some(id("fresh1")),
                &registry
            ),
            Err(IdentityPlanError::DeleteIdentityMissing(moved_id.clone()))
        );
        assert_eq!(
            bank.clear('a', &mut registry),
            Err(RegisterClearError::Identity(
                IdentityPlanError::DeleteIdentityMissing(moved_id.clone())
            ))
        );
        assert!(bank.get('a').is_some());

        let failure = bank
            .replace(
                'a',
                BlockRegister::without_identity("new".to_owned()),
                &mut registry,
            )
            .unwrap_err();
        assert_eq!(
            failure.error,
            RegisterCleanupError::Identity(IdentityPlanError::DeleteIdentityMissing(moved_id))
        );
        assert_eq!(failure.register.visible_text(), "new");
        assert_eq!(bank.get('a').unwrap().visible_text(), "one");
    }

    #[test]
    fn register_bank_rejects_missing_sources_and_occupied_slots() {
        let mut bank = RegisterBank::default();
        assert_eq!(bank.alias('a', 'b'), Err(RegisterSlotError::Empty('a')));
        bank.insert('a', BlockRegister::without_identity("one".to_owned()))
            .unwrap();
        let rejected = bank
            .insert('a', BlockRegister::without_identity("two".to_owned()))
            .unwrap_err();
        assert_eq!(rejected.visible_text(), "two");
        assert_eq!(bank.alias('a', 'a'), Err(RegisterSlotError::Occupied('a')));
        assert_eq!(bank.get('a').unwrap().visible_text(), "one");
        assert_eq!(
            bank.clear('b', &mut FakeRegistry::default()),
            Err(RegisterClearError::Empty('b'))
        );
    }

    #[test]
    fn occupied_slot_returns_the_new_move_register_without_losing_either_identity() {
        let source_page = page("source");
        let first_id = id("first1");
        let second_id = id("second");
        let mut registry = FakeRegistry::default();
        registry.seed(first_id.clone(), source_page.clone());
        registry.seed(second_id.clone(), source_page.clone());
        let (first, first_delete) =
            plan_delete(&source_page, &first_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&first_delete.changes).unwrap();
        let (second, second_delete) =
            plan_delete(&source_page, &second_id, "two".to_owned(), &registry).unwrap();
        registry.apply(&second_delete.changes).unwrap();
        let mut bank = RegisterBank::default();
        bank.insert('a', first).unwrap();
        let second = bank.insert('a', second).unwrap_err();

        assert_eq!(bank.get('a').unwrap().visible_text(), "one");
        assert_eq!(second.visible_text(), "two");
        assert_eq!(
            registry.state(&first_id),
            Ok(detached(source_page.clone(), 1))
        );
        assert_eq!(
            registry.state(&second_id),
            Ok(detached(source_page.clone(), 1))
        );
        bank.clear('a', &mut registry).unwrap();
        bank.insert('a', second).unwrap();
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(registry.state(&first_id), Ok(IdentityState::Retired));
        assert_eq!(registry.state(&second_id), Ok(IdentityState::Retired));
    }

    fn detached_register(
        registry: &mut FakeRegistry,
        block_id: &BlockId,
        source_page: &PageId,
        text: &str,
    ) -> BlockRegister {
        registry.seed(block_id.clone(), source_page.clone());
        let (register, plan) =
            plan_delete(source_page, block_id, text.to_owned(), registry).unwrap();
        registry.apply(&plan.changes).unwrap();
        register
    }

    #[test]
    fn replacing_a_register_retires_only_the_last_old_owner() {
        for keep_alias in [false, true] {
            let source = page("source");
            let old_id = id("old001");
            let new_id = id("new001");
            let mut registry = FakeRegistry::default();
            let old = detached_register(&mut registry, &old_id, &source, "old");
            let new = detached_register(&mut registry, &new_id, &source, "new");
            let mut bank = RegisterBank::default();
            bank.insert('a', old).unwrap();
            if keep_alias {
                bank.alias('a', 'b').unwrap();
            }

            bank.replace('a', new, &mut registry).unwrap();

            assert_eq!(bank.get('a').unwrap().visible_text(), "new");
            assert_eq!(registry.state(&new_id), Ok(detached(source.clone(), 1)));
            if keep_alias {
                assert_eq!(registry.state(&old_id), Ok(detached(source.clone(), 1)));
                bank.clear('b', &mut registry).unwrap();
            }
            assert_eq!(registry.state(&old_id), Ok(IdentityState::Retired));
            let paste =
                plan_paste(bank.get('a').unwrap(), &page("target"), None, &registry).unwrap();
            assert_eq!(
                paste.decision,
                IdentityDecision::Paste {
                    assigned_id: Some(new_id.clone()),
                    reused_source_id: true,
                }
            );
            registry.apply(&paste.changes).unwrap();
            bank.clear('a', &mut registry).unwrap();
            assert_eq!(registry.state(&new_id), Ok(live(page("target"), 1)));
        }
    }

    #[test]
    fn failed_replacement_preserves_old_slot_and_returns_new_register() {
        for fail_read in [false, true] {
            let source = page("source");
            let old_id = id("old001");
            let new_id = id("new001");
            let mut registry = FakeRegistry::default();
            let old = detached_register(&mut registry, &old_id, &source, "old");
            let new = detached_register(&mut registry, &new_id, &source, "new");
            let mut bank = RegisterBank::default();
            bank.insert('a', old).unwrap();
            registry.fail_state_reads = fail_read;
            registry.fail_next_apply = !fail_read;
            let before = registry.clone();

            let failure = bank.replace('a', new, &mut registry).unwrap_err();

            let expected = if fail_read {
                RegisterCleanupError::Identity(IdentityPlanError::Registry(
                    "registry state read failed",
                ))
            } else {
                RegisterCleanupError::Registry("registry transaction failed")
            };
            assert_eq!(failure.error, expected);
            assert_eq!(bank.get('a').unwrap().visible_text(), "old");
            assert_eq!(failure.register.visible_text(), "new");
            assert_eq!(failure.register.identity().unwrap().id, new_id);
            assert_eq!(registry, before);

            registry.fail_state_reads = false;
            registry.fail_next_apply = false;
            bank.replace('a', failure.register, &mut registry).unwrap();
            assert_eq!(registry.state(&old_id), Ok(IdentityState::Retired));
            assert_eq!(registry.state(&new_id), Ok(detached(source, 1)));
        }
    }

    #[test]
    fn replacement_reports_wrong_source_page_without_losing_either_register() {
        let source = page("source");
        let block_id = id("move01");
        let mut registry = FakeRegistry::default();
        let register = detached_register(&mut registry, &block_id, &source, "old");
        registry
            .detached
            .insert(block_id.clone(), (page("other"), DetachGeneration::new(1)));
        let before = registry.clone();
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();
        let incoming = BlockRegister::without_identity("new".to_owned());

        let failure = bank.replace('a', incoming, &mut registry).unwrap_err();

        assert_eq!(
            failure.error,
            RegisterCleanupError::Identity(IdentityPlanError::DetachedRegisterPageMismatch(
                block_id
            ))
        );
        assert_eq!(bank.get('a').unwrap().visible_text(), "old");
        assert_eq!(failure.register.visible_text(), "new");
        assert_eq!(registry, before);
    }

    #[test]
    fn incoming_alias_prevents_retiring_the_detachment_it_replaces() {
        let source = page("source");
        let block_id = id("move01");
        let mut registry = FakeRegistry::default();
        let register = detached_register(&mut registry, &block_id, &source, "old");
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();
        let incoming = bank.get('a').unwrap().tracked_alias();

        bank.replace('a', incoming, &mut registry).unwrap();

        assert_eq!(registry.state(&block_id), Ok(detached(source, 1)));
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(registry.state(&block_id), Ok(IdentityState::Retired));
    }

    #[test]
    fn replacing_with_an_alias_is_retryable_and_keeps_source_ownership() {
        let source = page("source");
        let source_id = id("source1");
        let target_id = id("target1");
        let mut registry = FakeRegistry::default();
        let source_register = detached_register(&mut registry, &source_id, &source, "source");
        let target_register = detached_register(&mut registry, &target_id, &source, "target");
        let mut bank = RegisterBank::default();
        bank.insert('a', source_register).unwrap();
        bank.insert('b', target_register).unwrap();
        registry.fail_next_apply = true;
        let before = registry.clone();

        assert_eq!(
            bank.replace_with_alias('a', 'b', &mut registry),
            Err(RegisterAliasReplaceError::Cleanup(
                RegisterCleanupError::Registry("registry transaction failed")
            ))
        );
        assert_eq!(bank.get('a').unwrap().visible_text(), "source");
        assert_eq!(bank.get('b').unwrap().visible_text(), "target");
        assert_eq!(registry, before);

        registry.fail_next_apply = false;
        bank.replace_with_alias('a', 'b', &mut registry).unwrap();
        assert_eq!(bank.get('a').unwrap().visible_text(), "source");
        assert_eq!(bank.get('b').unwrap().visible_text(), "source");
        assert_eq!(registry.state(&target_id), Ok(IdentityState::Retired));
        bank.clear('a', &mut registry).unwrap();
        assert_eq!(registry.state(&source_id), Ok(detached(source, 1)));
        bank.clear('b', &mut registry).unwrap();
        assert_eq!(registry.state(&source_id), Ok(IdentityState::Retired));
    }

    #[test]
    fn alias_replacement_handles_missing_source_and_same_slot_without_cleanup() {
        let mut bank = RegisterBank::default();
        let mut registry = FakeRegistry::default();
        assert_eq!(
            bank.replace_with_alias('a', 'b', &mut registry),
            Err(RegisterAliasReplaceError::EmptySource('a'))
        );
        let source = page("source");
        let block_id = id("move01");
        let register = detached_register(&mut registry, &block_id, &source, "one");
        bank.insert('a', register).unwrap();
        registry.fail_state_reads = true;
        registry.fail_next_apply = true;
        let before = registry.clone();

        bank.replace_with_alias('a', 'a', &mut registry).unwrap();

        assert_eq!(bank.get('a').unwrap().visible_text(), "one");
        assert_eq!(registry, before);
    }

    #[test]
    fn alias_replacement_distinguishes_stale_and_current_generations() {
        for replace_current in [false, true] {
            let source = page("source");
            let block_id = id("move01");
            let mut registry = FakeRegistry::default();
            let register = detached_register(&mut registry, &block_id, &source, "first");
            let mut bank = RegisterBank::default();
            bank.insert('a', register).unwrap();
            bank.alias('a', 'b').unwrap();
            let paste = plan_paste(bank.get('a').unwrap(), &source, None, &registry).unwrap();
            registry.apply(&paste.changes).unwrap();
            bank.clear('a', &mut registry).unwrap();
            let (current, delete) =
                plan_delete(&source, &block_id, "second".to_owned(), &registry).unwrap();
            registry.apply(&delete.changes).unwrap();
            bank.insert('a', current).unwrap();

            if replace_current {
                bank.replace_with_alias('b', 'a', &mut registry).unwrap();
                assert_eq!(registry.state(&block_id), Ok(IdentityState::Retired));
            } else {
                bank.replace_with_alias('a', 'b', &mut registry).unwrap();
                assert_eq!(registry.state(&block_id), Ok(detached(source.clone(), 2)));
                bank.clear('a', &mut registry).unwrap();
                assert_eq!(registry.state(&block_id), Ok(detached(source, 2)));
                bank.clear('b', &mut registry).unwrap();
                assert_eq!(registry.state(&block_id), Ok(IdentityState::Retired));
            }
        }
    }

    #[test]
    fn replacement_supports_empty_slots_and_registers_without_identity() {
        let source = page("source");
        let block_id = id("move01");
        let mut registry = FakeRegistry::default();
        let register = detached_register(&mut registry, &block_id, &source, "move");
        let mut bank = RegisterBank::default();
        bank.replace('a', register, &mut registry).unwrap();
        bank.replace(
            'a',
            BlockRegister::without_identity("plain".to_owned()),
            &mut registry,
        )
        .unwrap();

        assert_eq!(bank.get('a').unwrap().visible_text(), "plain");
        assert_eq!(registry.state(&block_id), Ok(IdentityState::Retired));
        bank.clear('a', &mut registry).unwrap();

        let yanked_id = id("yank01");
        registry.seed(yanked_id.clone(), source.clone());
        bank.insert(
            'b',
            BlockRegister::yank("yanked".to_owned(), yanked_id.clone(), source.clone()),
        )
        .unwrap();
        registry.fail_state_reads = true;
        bank.replace(
            'b',
            BlockRegister::without_identity("plain".to_owned()),
            &mut registry,
        )
        .unwrap();
        registry.fail_state_reads = false;
        assert_eq!(registry.state(&yanked_id), Ok(live(source, 0)));
    }

    #[test]
    fn discarding_an_uninstalled_register_returns_it_if_retirement_fails() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        registry.fail_next_apply = true;
        let before = registry.clone();

        let failure = register.discard(&mut registry).unwrap_err();
        assert_eq!(
            failure.error,
            RegisterCleanupError::Registry("registry transaction failed")
        );
        assert_eq!(failure.register.visible_text(), "one");
        assert_eq!(registry, before);
        registry.fail_next_apply = false;
        failure.register.discard(&mut registry).unwrap();
        assert_eq!(registry.state(&moved_id), Ok(IdentityState::Retired));
    }

    #[test]
    fn bounded_register_histories_match_an_independent_ownership_model() {
        const SLOTS: [char; 2] = ['a', 'b'];
        const ACTIONS: usize = 10;
        const DEPTH: u32 = 5;
        for encoded in 0..ACTIONS.pow(DEPTH) {
            let mut registry = FakeRegistry::default();
            let source_page = page("source");
            let moved_id = id("move01");
            registry.seed(moved_id.clone(), source_page.clone());
            let mut bank = RegisterBank::default();
            let mut state = OwnershipModelState::Live(0);
            let mut slots = [None; 2];
            let mut remaining = encoded;
            let mut trace = Vec::new();

            for _ in 0..DEPTH {
                let action = remaining % ACTIONS;
                remaining /= ACTIONS;
                trace.push(action);
                match action {
                    0 | 1 => {
                        let slot = action;
                        if let (OwnershipModelState::Live(generation), None) = (state, slots[slot])
                        {
                            let (register, plan) =
                                plan_delete(&source_page, &moved_id, "one".to_owned(), &registry)
                                    .unwrap();
                            registry.apply(&plan.changes).unwrap();
                            bank.insert(SLOTS[slot], register).unwrap();
                            state = OwnershipModelState::Detached(generation + 1);
                            slots[slot] = Some(generation + 1);
                        }
                    }
                    2 | 3 => {
                        let slot = action - 2;
                        if let (OwnershipModelState::Detached(generation), Some(captured)) =
                            (state, slots[slot])
                        {
                            if generation == captured {
                                let plan = plan_paste(
                                    bank.get(SLOTS[slot]).unwrap(),
                                    &source_page,
                                    None,
                                    &registry,
                                )
                                .unwrap();
                                registry.apply(&plan.changes).unwrap();
                                state = OwnershipModelState::Live(generation);
                            }
                        }
                    }
                    4 | 5 => {
                        let source = action - 4;
                        let target = 1 - source;
                        if slots[source].is_some() && slots[target].is_none() {
                            bank.alias(SLOTS[source], SLOTS[target]).unwrap();
                            slots[target] = slots[source];
                        }
                    }
                    6 | 7 => {
                        let slot = action - 6;
                        if let Some(captured) = slots[slot] {
                            bank.clear(SLOTS[slot], &mut registry).unwrap();
                            slots[slot] = None;
                            if state == OwnershipModelState::Detached(captured)
                                && !slots.contains(&Some(captured))
                            {
                                state = OwnershipModelState::Retired;
                            }
                        }
                    }
                    8 | 9 => {
                        let source = action - 8;
                        let target = 1 - source;
                        if let Some(captured) = slots[source] {
                            let replaced = slots[target];
                            bank.replace_with_alias(SLOTS[source], SLOTS[target], &mut registry)
                                .unwrap();
                            slots[target] = Some(captured);
                            if let Some(replaced) = replaced {
                                if state == OwnershipModelState::Detached(replaced)
                                    && !slots.contains(&Some(replaced))
                                {
                                    state = OwnershipModelState::Retired;
                                }
                            }
                        }
                    }
                    _ => unreachable!(),
                }

                let expected = model_registry_state(state, &source_page);
                assert_eq!(registry.state(&moved_id), Ok(expected), "trace: {trace:?}");
                for (slot, captured) in SLOTS.into_iter().zip(slots) {
                    let register = bank.get(slot);
                    assert_eq!(register.is_some(), captured.is_some(), "trace: {trace:?}");
                    if let (Some(register), Some(captured)) = (register, captured) {
                        let plan =
                            plan_paste(register, &source_page, Some(id("fresh1")), &registry)
                                .unwrap();
                        assert_eq!(
                            plan.decision,
                            model_paste_decision(state, captured, &moved_id),
                            "trace: {trace:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ownership_model_detects_injected_early_retirement_and_stale_reuse() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let mut bank = RegisterBank::default();
        bank.insert('a', register).unwrap();
        bank.alias('a', 'b').unwrap();
        bank.clear('a', &mut registry).unwrap();

        let expected = model_registry_state(OwnershipModelState::Detached(1), &source_page);
        assert_eq!(registry.state(&moved_id), Ok(expected.clone()));
        let mut faulted_registry = registry.clone();
        faulted_registry.detached.remove(&moved_id);
        faulted_registry.retired.insert(moved_id.clone());
        assert_ne!(faulted_registry.state(&moved_id), Ok(expected));

        let old_register = bank.get('b').unwrap();
        let paste = plan_paste(old_register, &source_page, None, &registry).unwrap();
        registry.apply(&paste.changes).unwrap();
        let (_new_register, second_delete) =
            plan_delete(&source_page, &moved_id, "two".to_owned(), &registry).unwrap();
        registry.apply(&second_delete.changes).unwrap();
        let mut plan =
            plan_paste(old_register, &source_page, Some(id("fresh1")), &registry).unwrap();
        let expected = model_paste_decision(OwnershipModelState::Detached(2), 1, &moved_id);
        assert_eq!(plan.decision, expected);
        plan.decision = IdentityDecision::Paste {
            assigned_id: Some(moved_id),
            reused_source_id: true,
        };
        assert_ne!(plan.decision, expected);
    }

    #[test]
    fn exhausted_detach_generation_fails_without_changing_text_or_registry() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.live.insert(
            moved_id.clone(),
            (source_page.clone(), DetachGeneration::new(u64::MAX)),
        );
        let before = registry.clone();
        let mut text = "one ^move01\n".to_owned();

        assert!(matches!(
            plan_delete_edit(&source_page, &mut text, &moved_id, &mut registry),
            Err(IdentityEditPlanError::Identity(
                IdentityPlanError::DetachGenerationExhausted(id)
            )) if id == moved_id
        ));
        assert_eq!(text, "one ^move01\n");
        assert_eq!(registry, before);
        assert_eq!(
            registry.apply(&[RegistryChange::Detach {
                id: moved_id,
                page: source_page,
                previous_generation: DetachGeneration::new(u64::MAX),
            }]),
            Err("detach generation exhausted")
        );
        assert_eq!(registry, before);
    }

    #[test]
    fn yank_then_paste_allocates_a_fresh_identity_while_source_is_live() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("copy01"), page("source"));
        let copied = register("copy01", RegisterOperation::Yank);
        let plan = plan_paste(&copied, &page("target"), Some(id("copy02")), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("copy02")),
                reused_source_id: false,
            }
        );
        registry.apply(&plan.changes).unwrap();
        assert_eq!(registry.state(&id("copy01")), Ok(live(page("source"), 0)));
        assert_eq!(registry.state(&id("copy02")), Ok(live(page("target"), 0)));
    }

    #[test]
    fn stale_yank_cannot_claim_an_identity_detached_for_a_move_register() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("copy01"), page("source"));
        let copied = register("copy01", RegisterOperation::Yank);
        let (_, delete_plan) = plan_delete(
            &page("source"),
            &id("copy01"),
            "block text".to_owned(),
            &registry,
        )
        .unwrap();
        registry.apply(&delete_plan.changes).unwrap();

        let plan = plan_paste(&copied, &page("target"), Some(id("copy02")), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("copy02")),
                reused_source_id: false,
            }
        );
        assert_eq!(
            registry.state(&id("copy01")),
            Ok(detached(page("source"), 1))
        );
    }

    #[test]
    fn retired_or_colliding_ids_are_never_reused_by_paste() {
        let mut registry = FakeRegistry::default();
        registry.retired.insert(id("old001"));
        registry.seed(id("taken1"), page("other"));
        let copied = register("old001", delete_operation(1));

        assert_eq!(
            plan_paste(&copied, &page("target"), Some(id("taken1")), &registry),
            Err(IdentityPlanError::FreshIdUnavailable(id("taken1")))
        );
        let plan = plan_paste(&copied, &page("target"), Some(id("fresh1")), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("fresh1")),
                reused_source_id: false,
            }
        );
    }

    #[test]
    fn a_fresh_candidate_cannot_be_the_unavailable_register_identity() {
        let registry = FakeRegistry::default();
        let copied = register("source1", RegisterOperation::Yank);

        assert_eq!(
            plan_paste(&copied, &page("target"), Some(id("source1")), &registry),
            Err(IdentityPlanError::FreshIdUnavailable(id("source1")))
        );
    }

    #[test]
    fn join_keeps_the_first_identity_and_retires_the_second() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("left01"), page("page"));
        registry.seed(id("right1"), page("page"));
        let plan = plan_join(&page("page"), &id("left01"), &id("right1"), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Join {
                retained_id: id("left01"),
                retired_id: id("right1"),
            }
        );
        registry.apply(&plan.changes).unwrap();
        assert_eq!(registry.state(&id("left01")), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&id("right1")), Ok(IdentityState::Retired));
    }

    #[test]
    fn split_keeps_left_identity_and_reserves_a_fresh_right_identity() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("split1"), page("page"));
        let plan = plan_split(&page("page"), &id("split1"), id("split2"), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Split {
                left_id: id("split1"),
                right_id: id("split2"),
            }
        );
        registry.apply(&plan.changes).unwrap();
        assert_eq!(registry.state(&id("split1")), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&id("split2")), Ok(live(page("page"), 0)));
    }

    #[test]
    fn join_revalidates_both_identities_on_the_expected_page() {
        for stale_id in ["left01", "right1"] {
            let mut registry = FakeRegistry::default();
            registry.seed(id("left01"), page("page"));
            registry.seed(id("right1"), page("page"));
            let plan = plan_join(&page("page"), &id("left01"), &id("right1"), &registry).unwrap();
            registry
                .live
                .insert(id(stale_id), (page("other"), DetachGeneration::INITIAL));
            let before = registry.clone();

            assert_eq!(
                registry.apply(&plan.changes),
                Err("identity is not live at the expected page and generation")
            );
            assert_eq!(registry, before);
        }
    }

    #[test]
    fn join_rejects_a_live_identity_detached_and_returned_after_planning() {
        let source_page = page("source");
        let mut registry = FakeRegistry::default();
        registry.seed(id("left01"), source_page.clone());
        registry.seed(id("right1"), source_page.clone());
        let join = plan_join(&source_page, &id("left01"), &id("right1"), &registry).unwrap();
        let (register, delete) =
            plan_delete(&source_page, &id("right1"), "right".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let paste = plan_paste(&register, &source_page, None, &registry).unwrap();
        registry.apply(&paste.changes).unwrap();
        let before = registry.clone();

        assert!(registry.apply(&join.changes).is_err());
        assert_eq!(registry, before);
    }

    #[test]
    fn shared_registry_change_after_planning_rejects_join_without_changing_text() {
        let mut registry = SharedRegistry::default();
        registry.0.borrow_mut().seed(id("left01"), page("page"));
        registry.0.borrow_mut().seed(id("right1"), page("page"));
        let external = registry.clone();
        let mut text = "left ^left01\nright ^right1\n".to_owned();
        let plan = plan_join_edit(
            &page("page"),
            &mut text,
            4..5,
            " ",
            &id("left01"),
            &id("right1"),
            &mut registry,
        )
        .unwrap();
        external
            .0
            .borrow_mut()
            .live
            .insert(id("right1"), (page("other"), DetachGeneration::INITIAL));
        let before_commit = external.0.borrow().clone();

        assert_eq!(
            commit_planned_identity_edit(plan),
            Err("identity is not live at the expected page and generation")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
        assert_eq!(*external.0.borrow(), before_commit);
    }

    #[test]
    fn shared_registry_aba_rejects_stale_join_and_split_edits() {
        for stale_id in ["left01", "right1"] {
            let source_page = page("source");
            let mut registry = SharedRegistry::default();
            registry
                .0
                .borrow_mut()
                .seed(id("left01"), source_page.clone());
            registry
                .0
                .borrow_mut()
                .seed(id("right1"), source_page.clone());
            let mut external = registry.clone();
            let mut text = "left ^left01\nright ^right1\n".to_owned();
            let plan = plan_join_edit(
                &source_page,
                &mut text,
                4..5,
                " ",
                &id("left01"),
                &id("right1"),
                &mut registry,
            )
            .unwrap();
            detach_and_return(&mut external, &source_page, &id(stale_id));
            let before_commit = external.0.borrow().clone();

            assert_eq!(
                commit_planned_identity_edit(plan),
                Err("identity is not live at the expected page and generation")
            );
            assert_eq!(text, "left ^left01\nright ^right1\n");
            assert_eq!(*external.0.borrow(), before_commit);
        }

        let source_page = page("source");
        let mut registry = SharedRegistry::default();
        registry
            .0
            .borrow_mut()
            .seed(id("left01"), source_page.clone());
        let mut external = registry.clone();
        let mut text = "left ^left01\n".to_owned();
        let plan = plan_split_edit(
            &source_page,
            &mut text,
            2,
            &id("left01"),
            id("right1"),
            &mut registry,
        )
        .unwrap();
        detach_and_return(&mut external, &source_page, &id("left01"));
        let before_commit = external.0.borrow().clone();

        assert_eq!(
            commit_planned_identity_edit(plan),
            Err("identity is not live at the expected page and generation")
        );
        assert_eq!(text, "left ^left01\n");
        assert_eq!(*external.0.borrow(), before_commit);
    }

    #[test]
    fn shared_registry_aba_rejects_stale_delete_and_paste_edits() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = SharedRegistry::default();
        registry
            .0
            .borrow_mut()
            .seed(moved_id.clone(), source_page.clone());
        let mut external = registry.clone();
        let mut source_text = "one ^move01\n".to_owned();
        let delete =
            plan_delete_edit(&source_page, &mut source_text, &moved_id, &mut registry).unwrap();
        detach_and_return(&mut external, &source_page, &moved_id);
        let before_commit = external.0.borrow().clone();

        assert_eq!(
            commit_planned_block_delete(delete),
            Err("identity is not live at the expected page and generation")
        );
        assert_eq!(source_text, "one ^move01\n");
        assert_eq!(*external.0.borrow(), before_commit);

        let (old_register, first_delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &external).unwrap();
        external.apply(&first_delete.changes).unwrap();
        let mut target_text = "target ^dest01\n".to_owned();
        external.0.borrow_mut().seed(id("dest01"), page("target"));
        let insertion_at = Doc::new(&target_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        let paste = plan_paste_edit(
            &page("target"),
            &mut target_text,
            insertion_at,
            &old_register,
            None,
            &mut registry,
        )
        .unwrap();
        let first_paste = plan_paste(&old_register, &source_page, None, &external).unwrap();
        external.apply(&first_paste.changes).unwrap();
        let (_new_register, second_delete) =
            plan_delete(&source_page, &moved_id, "two".to_owned(), &external).unwrap();
        external.apply(&second_delete.changes).unwrap();
        let before_commit = external.0.borrow().clone();

        assert_eq!(
            commit_planned_identity_edit(paste),
            Err("identity is not detached at the expected page and generation")
        );
        assert_eq!(target_text, "target ^dest01\n");
        assert_eq!(*external.0.borrow(), before_commit);
    }

    #[test]
    fn split_revalidates_source_liveness_and_fresh_id_availability() {
        for stale_source in [true, false] {
            let mut registry = FakeRegistry::default();
            registry.seed(id("split1"), page("page"));
            let plan = plan_split(&page("page"), &id("split1"), id("split2"), &registry).unwrap();
            if stale_source {
                registry
                    .live
                    .insert(id("split1"), (page("other"), DetachGeneration::INITIAL));
            } else {
                registry.seed(id("split2"), page("other"));
            }
            let before = registry.clone();

            assert_eq!(
                registry.apply(&plan.changes),
                Err(if stale_source {
                    "identity is not live at the expected page and generation"
                } else {
                    "identity is not available"
                })
            );
            assert_eq!(registry, before);
        }
    }

    #[test]
    fn dropping_a_planned_delete_leaves_text_and_registry_unchanged() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("block1"), page("page"));
        let before_registry = registry.clone();
        let mut text = "one ^block1\n".to_owned();

        {
            let _plan =
                plan_delete_edit(&page("page"), &mut text, &id("block1"), &mut registry).unwrap();
        }

        assert_eq!(text, "one ^block1\n");
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn failed_block_delete_does_not_return_a_register_or_mutate_state() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("block1"), page("page"));
        registry.fail_next_apply = true;
        let before_registry = registry.clone();
        let mut text = "one ^block1\n".to_owned();
        let plan =
            plan_delete_edit(&page("page"), &mut text, &id("block1"), &mut registry).unwrap();

        assert_eq!(
            commit_planned_block_delete(plan),
            Err("registry transaction failed")
        );
        assert_eq!(text, "one ^block1\n");
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn actual_join_stream_retires_the_second_id_and_moves_the_first_to_line_end() {
        let fields = oracle_case("join");
        assert_eq!(fields.len(), 7);
        let canonical = decode_oracle_text(fields[1]);
        let projection = Doc::new(&canonical).visible_projection();
        assert_eq!(projection.text(), decode_oracle_text(fields[2]));
        let edits = fields[6].split(';').collect::<Vec<_>>();
        assert_eq!(edits.len(), 1);
        let (start, end, replacement) = oracle_edit(edits[0]);
        let retained = id("left01");
        let retired = id("right1");
        let mut registry = FakeRegistry::default();
        registry.seed(retained.clone(), page("page"));
        registry.seed(retired.clone(), page("page"));
        let mut actual_text = canonical;
        let plan = plan_join_edit(
            &page("page"),
            &mut actual_text,
            start..end,
            &replacement,
            &retained,
            &retired,
            &mut registry,
        )
        .unwrap();

        commit_planned_identity_edit(plan).unwrap();

        assert_eq!(actual_text, "left right ^left01\n");
        assert_eq!(
            Doc::new(&actual_text).visible_projection().text(),
            decode_oracle_text(fields[5])
        );
        assert_eq!(
            block_ids_in_text(&actual_text).as_slice(),
            std::slice::from_ref(&retained)
        );
        assert_eq!(registry.state(&retained), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&retired), Ok(IdentityState::Retired));
    }

    #[test]
    fn actual_split_stream_keeps_the_left_id_and_allocates_the_right_id() {
        let fields = oracle_case("split");
        assert_eq!(fields.len(), 7);
        let canonical = decode_oracle_text(fields[1]);
        let projection = Doc::new(&canonical).visible_projection();
        assert_eq!(projection.text(), decode_oracle_text(fields[2]));
        let edits = fields[6].split(';').collect::<Vec<_>>();
        assert_eq!(edits.len(), 1);
        let (start, end, replacement) = oracle_edit(edits[0]);
        assert_eq!(replacement, "\n");
        assert_eq!(start, end);
        let original = id("split1");
        let allocated = id("split2");
        let mut registry = FakeRegistry::default();
        registry.seed(original.clone(), page("page"));
        let mut actual_text = canonical;
        let plan = plan_split_edit(
            &page("page"),
            &mut actual_text,
            start,
            &original,
            allocated.clone(),
            &mut registry,
        )
        .unwrap();

        commit_planned_identity_edit(plan).unwrap();

        assert_eq!(actual_text, "alpha ^split1\n beta ^split2\n");
        assert_eq!(
            Doc::new(&actual_text).visible_projection().text(),
            decode_oracle_text(fields[5])
        );
        assert_eq!(
            block_ids_in_text(&actual_text),
            [original.clone(), allocated.clone()]
        );
        assert_eq!(registry.state(&original), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&allocated), Ok(live(page("page"), 0)));
    }

    #[test]
    fn recorded_vim_structural_edits_preserve_surrounding_token_blocks() {
        for case in ["join", "split"] {
            let fields = oracle_case(case);
            let source_id = if case == "join" { "left01" } else { "split1" };
            let prefix = format!("before ^other1\nref ^={source_id}\n");
            let suffix = "after ^other2\n";
            let mut canonical = format!("{prefix}{}{suffix}", decode_oracle_text(fields[1]));
            let prefix_projection = Doc::new(&prefix).visible_projection();
            let suffix_projection = Doc::new(suffix).visible_projection();
            let offset = prefix_projection.text().chars().count();
            let (start, end, replacement) = oracle_edit(fields[6]);
            let mut registry = FakeRegistry::default();
            registry.seed(id(source_id), page("page"));

            let expected_changed = if case == "join" {
                registry.seed(id("right1"), page("page"));
                let plan = plan_join_edit(
                    &page("page"),
                    &mut canonical,
                    start + offset..end + offset,
                    &replacement,
                    &id("left01"),
                    &id("right1"),
                    &mut registry,
                )
                .unwrap();
                commit_planned_identity_edit(plan).unwrap();
                assert_eq!(registry.state(&id("right1")), Ok(IdentityState::Retired));
                "left right ^left01\n"
            } else {
                assert_eq!(start, end);
                assert_eq!(replacement, "\n");
                let plan = plan_split_edit(
                    &page("page"),
                    &mut canonical,
                    start + offset,
                    &id("split1"),
                    id("split2"),
                    &mut registry,
                )
                .unwrap();
                commit_planned_identity_edit(plan).unwrap();
                assert_eq!(registry.state(&id("split2")), Ok(live(page("page"), 0)));
                "alpha ^split1\n beta ^split2\n"
            };

            assert_eq!(canonical, format!("{prefix}{expected_changed}{suffix}"));
            assert_eq!(
                Doc::new(&canonical).visible_projection().text(),
                format!(
                    "{}{}{}",
                    prefix_projection.text(),
                    decode_oracle_text(fields[5]),
                    suffix_projection.text(),
                )
            );
            assert_eq!(registry.state(&id(source_id)), Ok(live(page("page"), 0)));
        }
    }

    #[test]
    fn structural_text_plans_preserve_crlf_and_validate_line_bounds() {
        let retained = id("left01");
        let retired = id("right1");
        let join_input = "left ^left01\r\nright ^right1\r\n";
        assert_eq!(
            plan_join_text(join_input, 4..6, " ", &retained, &retired).unwrap(),
            "left right ^left01\r\n"
        );
        assert_eq!(
            plan_join_text(join_input, 3..6, " ", &retained, &retired),
            Err(super::IdentityTextPlanError::InvalidJoinEdit)
        );

        let original = id("split1");
        let allocated = id("split2");
        let split_input = "alpha beta ^split1\r\n";
        assert_eq!(
            plan_split_text(split_input, 5, &original, &allocated).unwrap(),
            "alpha ^split1\r\n beta ^split2\r\n"
        );
        assert_eq!(
            append_block_id("", 1, &original),
            Err(super::IdentityTextPlanError::LineOutOfBounds(1))
        );
    }

    #[test]
    fn structural_text_plans_reject_edits_on_unrelated_lines() {
        let join_input = "intro\nleft ^left01\nright ^right1\noutro\n";
        assert_eq!(
            plan_join_text(join_input, 5..6, " ", &id("left01"), &id("right1")),
            Err(super::IdentityTextPlanError::InvalidJoinEdit)
        );
        assert_eq!(
            plan_join_text(join_input, 10..11, " ", &id("left01"), &id("right1")).unwrap(),
            "intro\nleft right ^left01\noutro\n"
        );
        assert_eq!(
            plan_join_text(
                "left ^left01\nmiddle\nright ^right1\n",
                4..5,
                " ",
                &id("left01"),
                &id("right1")
            ),
            Err(super::IdentityTextPlanError::InvalidJoinEdit)
        );

        let split_input = "before\nalpha beta ^split1\nafter\n";
        assert_eq!(
            plan_split_text(split_input, 3, &id("split1"), &id("split2")),
            Err(super::IdentityTextPlanError::InvalidSplitEdit)
        );
        assert_eq!(
            plan_split_text(split_input, 20, &id("split1"), &id("split2")),
            Err(super::IdentityTextPlanError::InvalidSplitEdit)
        );
        assert_eq!(
            plan_split_text(split_input, 12, &id("split1"), &id("split2")).unwrap(),
            "before\nalpha ^split1\n beta ^split2\nafter\n"
        );
    }

    #[test]
    fn join_preserves_unrelated_tokens_and_exact_canonical_bytes() {
        let prefix = concat!(
            "---\r\n",
            "title: ^frontmatter\r\n",
            "---\r\n",
            "π ^head01 \r\n",
            "ref ^=left01\t\r\n",
        );
        let suffix = concat!(
            "extra ^third_1\t\r\n",
            "ref ^=right1 \r\n",
            "```rust\r\n",
            "literal ^fenced\r\n",
            "```\r\n",
            "tail ^last01",
        );
        let mut canonical = format!("{prefix}left ^left01\r\nright ^right1  \r\n{suffix}");
        let projection = Doc::new(&canonical).visible_projection();
        let boundary = projection.text().find("left\r\nright").unwrap() + "left".len();
        let start = projection.text()[..boundary].chars().count();
        let mut registry = FakeRegistry::default();
        registry.seed(id("left01"), page("page"));
        registry.seed(id("right1"), page("page"));

        let plan = plan_join_edit(
            &page("page"),
            &mut canonical,
            start..start + 2,
            " ",
            &id("left01"),
            &id("right1"),
            &mut registry,
        )
        .unwrap();
        commit_planned_identity_edit(plan).unwrap();

        assert_eq!(
            canonical,
            format!("{prefix}left right   ^left01\r\n{suffix}")
        );
        assert_eq!(registry.state(&id("left01")), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&id("right1")), Ok(IdentityState::Retired));
    }

    #[test]
    fn split_preserves_unrelated_tokens_and_exact_canonical_bytes() {
        let prefix = "π ^head01 \nref ^=split1\t\n";
        let suffix = "extra ^third_1\t\r\nref ^=split1 \r\ntail ^last01";
        let mut canonical = format!("{prefix}alpha βeta ^split1  \r\n{suffix}");
        let projection = Doc::new(&canonical).visible_projection();
        let boundary = projection.text().find("alpha βeta").unwrap() + "alpha".len();
        let split_at = projection.text()[..boundary].chars().count();
        let mut registry = FakeRegistry::default();
        registry.seed(id("split1"), page("page"));

        let plan = plan_split_edit(
            &page("page"),
            &mut canonical,
            split_at,
            &id("split1"),
            id("split2"),
            &mut registry,
        )
        .unwrap();
        commit_planned_identity_edit(plan).unwrap();

        assert_eq!(
            canonical,
            format!("{prefix}alpha ^split1\r\n βeta   ^split2\r\n{suffix}")
        );
        assert_eq!(registry.state(&id("split1")), Ok(live(page("page"), 0)));
        assert_eq!(registry.state(&id("split2")), Ok(live(page("page"), 0)));
    }

    fn block_line(content: &str, id: &str) -> String {
        let separator = if content.is_empty() { "" } else { " " };
        format!("{content}{separator}^{id}")
    }

    #[test]
    fn multi_block_join_matches_line_local_reference_model() {
        let mut cases = 0;
        for ending in ["\n", "\r\n"] {
            let prefix = format!("before ^other1 \t{ending}mirror ^=left01\t{ending}");
            let prefix_len = Doc::new(&prefix)
                .visible_projection()
                .text()
                .chars()
                .count();
            for suffix in [
                String::new(),
                format!("{ending}mirror ^=right1 \t{ending}after ^other2"),
            ] {
                for left in ["", "α", "🙂 x", "e\u{301}"] {
                    for right in ["", "β", "q  "] {
                        for tail in ["", " \t"] {
                            let canonical = format!(
                                "{prefix}{}{tail}{ending}{}{tail}{suffix}",
                                block_line(left, "left01"),
                                block_line(right, "right1"),
                            );
                            let start = prefix_len + left.chars().count() + tail.chars().count();
                            let expected_visible = format!("{left}{tail} {right}{tail}");
                            let expected = format!(
                                "{prefix}{}{suffix}",
                                block_line(&expected_visible, "left01")
                            );

                            assert_eq!(
                                plan_join_text(
                                    &canonical,
                                    start..start + ending.chars().count(),
                                    " ",
                                    &id("left01"),
                                    &id("right1"),
                                ),
                                Ok(expected),
                                "input: {canonical:?}",
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 96);
    }

    #[test]
    fn multi_block_split_matches_line_local_reference_model() {
        let mut cases = 0;
        for ending in ["\n", "\r\n"] {
            let prefix = format!("before ^other1 \t{ending}mirror ^=split1\t{ending}");
            let prefix_len = Doc::new(&prefix)
                .visible_projection()
                .text()
                .chars()
                .count();
            for suffix in [
                String::new(),
                format!("{ending}mirror ^=split1 \t{ending}after ^other2"),
            ] {
                for content in ["", "alpha beta", "é e\u{301}🙂", "  ", "🧑‍💻"] {
                    for tail in ["", " \t"] {
                        let canonical =
                            format!("{prefix}{}{tail}{suffix}", block_line(content, "split1"));
                        let visible = format!("{content}{tail}").chars().collect::<Vec<_>>();
                        let projection = Doc::new(&canonical).visible_projection();
                        for offset in 0..=visible.len() {
                            let split_at = prefix_len + offset;
                            if projection
                                .range(split_at, split_at)
                                .and_then(|range| projection.lower(range))
                                .is_err()
                            {
                                continue;
                            }
                            let left = visible[..offset].iter().collect::<String>();
                            let right = visible[offset..].iter().collect::<String>();
                            let expected = format!(
                                "{prefix}{}{ending}{}{suffix}",
                                block_line(&left, "split1"),
                                block_line(&right, "split2"),
                            );

                            assert_eq!(
                                plan_split_text(&canonical, split_at, &id("split1"), &id("split2")),
                                Ok(expected),
                                "input: {canonical:?}, split offset: {offset}",
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert!(cases >= 200, "only exercised {cases} valid split positions");
    }

    #[test]
    fn structural_plans_reject_ambiguous_or_colliding_ids_without_mutation() {
        assert_eq!(
            plan_join_text(
                "one ^left01\ntwo ^right1\nagain ^left01\n",
                3..4,
                " ",
                &id("left01"),
                &id("right1"),
            ),
            Err(super::IdentityTextPlanError::IdentityMismatch)
        );
        assert_eq!(
            plan_join_text(
                "one ^left01\ntwo ^right1\nagain ^right1\n",
                3..4,
                " ",
                &id("left01"),
                &id("right1"),
            ),
            Err(super::IdentityTextPlanError::IdentityMismatch)
        );
        assert_eq!(
            plan_split_text(
                "one ^split1\ntwo ^split1\n",
                1,
                &id("split1"),
                &id("split2"),
            ),
            Err(super::IdentityTextPlanError::IdentityMismatch)
        );
        for other in ["other ^split2", "mirror ^=split2"] {
            let mut canonical = format!("alpha ^split1\n{other}\n");
            let before = canonical.clone();
            let mut registry = FakeRegistry::default();
            registry.seed(id("split1"), page("page"));
            let before_registry = registry.clone();

            assert!(matches!(
                plan_split_edit(
                    &page("page"),
                    &mut canonical,
                    2,
                    &id("split1"),
                    id("split2"),
                    &mut registry,
                ),
                Err(IdentityEditPlanError::Text(
                    super::IdentityTextPlanError::DuplicateIdentity
                ))
            ));
            assert_eq!(canonical, before);
            assert_eq!(registry, before_registry);
        }
    }

    #[test]
    fn structural_plans_preserve_unrelated_duplicate_legacy_ids() {
        let join_input = "before ^other1\nleft ^left01\nright ^right1\nafter ^other1";
        assert_eq!(
            plan_join_text(join_input, 11..12, " ", &id("left01"), &id("right1")),
            Ok("before ^other1\nleft right ^left01\nafter ^other1".to_owned())
        );

        let split_input = "before ^other1\nalpha ^split1\nafter ^other1";
        assert_eq!(
            plan_split_text(split_input, 9, &id("split1"), &id("split2")),
            Ok("before ^other1\nal ^split1\npha ^split2\nafter ^other1".to_owned())
        );
    }

    #[test]
    fn multi_block_registry_failures_leave_canonical_bytes_unchanged() {
        let mut join_text =
            "other ^other1\nleft ^left01\nright ^right1\nmirror ^=right1\n".to_owned();
        let join_before = join_text.clone();
        let projection = Doc::new(&join_text).visible_projection();
        let boundary = projection.text().find("left\nright").unwrap() + "left".len();
        let start = projection.text()[..boundary].chars().count();
        let mut join_registry = FakeRegistry::default();
        join_registry.seed(id("left01"), page("page"));
        join_registry.seed(id("right1"), page("page"));
        join_registry.fail_next_apply = true;
        let join_registry_before = join_registry.clone();
        let join = plan_join_edit(
            &page("page"),
            &mut join_text,
            start..start + 1,
            " ",
            &id("left01"),
            &id("right1"),
            &mut join_registry,
        )
        .unwrap();

        assert_eq!(
            commit_planned_identity_edit(join),
            Err("registry transaction failed")
        );
        assert_eq!(join_text, join_before);
        assert_eq!(join_registry, join_registry_before);

        let mut split_text = "other ^other1\nalpha ^split1\nmirror ^=split1\n".to_owned();
        let split_before = split_text.clone();
        let projection = Doc::new(&split_text).visible_projection();
        let boundary = projection.text().find("alpha").unwrap() + "alp".len();
        let split_at = projection.text()[..boundary].chars().count();
        let mut split_registry = FakeRegistry::default();
        split_registry.seed(id("split1"), page("page"));
        split_registry.fail_next_apply = true;
        let split_registry_before = split_registry.clone();
        let split = plan_split_edit(
            &page("page"),
            &mut split_text,
            split_at,
            &id("split1"),
            id("split2"),
            &mut split_registry,
        )
        .unwrap();

        assert_eq!(
            commit_planned_identity_edit(split),
            Err("registry transaction failed")
        );
        assert_eq!(split_text, split_before);
        assert_eq!(split_registry, split_registry_before);
    }

    #[test]
    fn structural_result_validation_detects_injected_token_faults() {
        let visible = "left right\nmirror\nother\n";
        let expected = [
            (BlockIdMarker::Block, "left01"),
            (BlockIdMarker::Mirror, "right1"),
            (BlockIdMarker::Block, "other1"),
        ];
        let correct = "left right ^left01\nmirror ^=right1\nother ^other1\n";
        assert_eq!(
            super::validate_planned_text(correct, visible, &expected),
            Ok(())
        );

        for faulty in [
            "left right ^left01\nmirror\nother ^other1\n",
            "left right ^left01\nmirror ^right1\nother ^other1\n",
            "left right ^left01\nmirror ^=other1\nother ^right1\n",
        ] {
            assert_eq!(Doc::new(faulty).visible_projection().text(), visible);
            assert_eq!(
                super::validate_planned_text(faulty, visible, &expected),
                Err(super::IdentityTextPlanError::InvalidResult),
                "fault was not detected: {faulty:?}",
            );
        }
    }

    #[test]
    fn multi_block_split_rejects_non_grapheme_and_leading_token_positions() {
        let canonical = "other ^other1\na e\u{301} ^split1\n";
        let projection = Doc::new(canonical).visible_projection();
        let boundary = projection.text().find("e\u{301}").unwrap() + "e".len();
        let offset = projection.text()[..boundary].chars().count();
        assert_eq!(
            plan_split_text(canonical, offset, &id("split1"), &id("split2")),
            Err(super::IdentityTextPlanError::Projection(
                ProjectionError::SplitsGrapheme { offset }
            ))
        );

        let canonical = "other ^other1\n^split1\n";
        let projection = Doc::new(canonical).visible_projection();
        let offset = "other\n".chars().count();
        assert_eq!(projection.tokens().len(), 2);
        assert_eq!(
            plan_split_text(canonical, offset, &id("split1"), &id("split2")),
            Err(super::IdentityTextPlanError::Projection(
                ProjectionError::InsertionAtLeadingToken { token_index: 1 }
            ))
        );
    }

    #[test]
    fn split_rejects_parser_context_changes_to_unrelated_tokens() {
        let mut canonical = "mirror ^=split1\nprefix ```a``` ^split1\noutside ^other1\n".to_owned();
        let before = canonical.clone();
        let projection = Doc::new(&canonical).visible_projection();
        let boundary = projection.text().find("```a```").unwrap();
        let split_at = projection.text()[..boundary].chars().count();
        let mut registry = FakeRegistry::default();
        registry.seed(id("split1"), page("page"));
        let before_registry = registry.clone();

        assert!(matches!(
            plan_split_edit(
                &page("page"),
                &mut canonical,
                split_at,
                &id("split1"),
                id("split2"),
                &mut registry,
            ),
            Err(IdentityEditPlanError::Text(
                super::IdentityTextPlanError::InvalidResult
            ))
        ));
        assert_eq!(canonical, before);
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn split_preserves_a_terminal_standalone_carriage_return() {
        let canonical = "other ^other1\nhi ^split1\r";
        let projection = Doc::new(canonical).visible_projection();
        let end = projection.text().chars().count();

        assert_eq!(
            plan_split_text(canonical, end, &id("split1"), &id("split2")),
            Ok("other ^other1\nhi ^split1\r\n^split2".to_owned()),
        );
    }

    #[test]
    fn split_preserves_a_carriage_return_before_a_crlf_separator() {
        let canonical = "before ^other1\na\r ^split1\r\n";
        assert_eq!(
            append_block_id("a\r\r\n", 0, &id("split1")),
            Ok("a\r ^split1\r\n".to_owned())
        );
        assert_eq!(
            plan_split_text(canonical, 9, &id("split1"), &id("split2")),
            Ok("before ^other1\na\r ^split1\r\n^split2\r\n".to_owned())
        );
    }

    #[test]
    fn split_rejects_a_result_whose_tokens_are_no_longer_recognized() {
        let canonical = "a `bc` ^split1\n";
        assert_eq!(
            plan_split_text(canonical, 4, &id("split1"), &id("split2")),
            Err(super::IdentityTextPlanError::InvalidResult)
        );
    }

    #[test]
    fn rejected_inline_code_split_leaves_text_and_registry_unchanged() {
        let mut canonical = "a `bc` ^split1\n".to_owned();
        let original = id("split1");
        let mut registry = FakeRegistry::default();
        registry.seed(original.clone(), page("page"));
        let before_registry = registry.clone();

        assert!(matches!(
            plan_split_edit(
                &page("page"),
                &mut canonical,
                4,
                &original,
                id("split2"),
                &mut registry
            ),
            Err(IdentityEditPlanError::Text(
                super::IdentityTextPlanError::InvalidResult
            ))
        ));
        assert_eq!(canonical, "a `bc` ^split1\n");
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn canonical_block_delete_and_paste_move_the_detached_identity() {
        let source_page = page("source");
        let target_page = page("target");
        let moved_id = id("blk01");
        let destination_id = id("dest01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        registry.seed(id("blk02"), source_page.clone());
        registry.seed(destination_id.clone(), target_page.clone());
        let mut source_text = "one ^blk01\nnext ^blk02\n".to_owned();

        let deletion =
            plan_delete_edit(&source_page, &mut source_text, &moved_id, &mut registry).unwrap();
        let register = commit_planned_block_delete(deletion).unwrap();
        assert_eq!(register.visible_text, "one");
        assert_eq!(source_text, "next ^blk02\n");
        let mut registers = RegisterBank::default();
        registers.insert('"', register).unwrap();
        registers.alias('"', 'a').unwrap();
        registers.clear('"', &mut registry).unwrap();

        let mut target_text = "target ^dest01\n".to_owned();
        let target_projection = Doc::new(&target_text).visible_projection();
        let paste_plan = plan_paste_edit(
            &target_page,
            &mut target_text,
            target_projection.text().chars().count(),
            registers.get('a').unwrap(),
            None,
            &mut registry,
        )
        .unwrap();
        commit_planned_identity_edit(paste_plan).unwrap();
        registers.clear('a', &mut registry).unwrap();

        assert_eq!(target_text, "target ^dest01\none ^blk01\n");
        assert_eq!(registry.state(&moved_id), Ok(live(target_page, 1)));
        assert_eq!(Doc::new(&source_text).visible_projection().text(), "next\n");
        assert_eq!(
            Doc::new(&target_text).visible_projection().text(),
            "target\none\n"
        );
    }

    #[test]
    fn canonical_crlf_block_can_be_deleted_and_pasted() {
        let source_page = page("source");
        let target_page = page("target");
        let moved_id = id("blk01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        registry.seed(id("blk02"), source_page.clone());
        registry.seed(id("dest01"), target_page.clone());
        let mut source_text = "one ^blk01\r\nnext ^blk02\r\n".to_owned();

        let deletion =
            plan_delete_edit(&source_page, &mut source_text, &moved_id, &mut registry).unwrap();
        let register = commit_planned_block_delete(deletion).unwrap();
        assert_eq!(register.visible_text, "one");
        assert_eq!(source_text, "next ^blk02\r\n");

        let mut target_text = "target ^dest01\r\n".to_owned();
        let insertion_at = Doc::new(&target_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        let paste = plan_paste_edit(
            &target_page,
            &mut target_text,
            insertion_at,
            &register,
            None,
            &mut registry,
        )
        .unwrap();
        commit_planned_identity_edit(paste).unwrap();

        assert_eq!(target_text, "target ^dest01\r\none ^blk01\r\n");
        assert_eq!(registry.state(&moved_id), Ok(live(target_page, 1)));
    }

    #[test]
    fn canonical_delete_rejects_unpasteable_carriage_returns_without_detaching() {
        for original in [
            "one ^block1\r",
            "a\rb ^block1\n",
            "a\r ^block1\n",
            "a\r ^block1\r\n",
        ] {
            let source = page("source");
            let block_id = id("block1");
            let mut registry = FakeRegistry::default();
            registry.seed(block_id.clone(), source.clone());
            let before_registry = registry.clone();
            let mut canonical = original.to_owned();

            assert!(matches!(
                plan_delete_edit(&source, &mut canonical, &block_id, &mut registry),
                Err(IdentityEditPlanError::Text(
                    super::IdentityTextPlanError::InvalidRegisterText
                ))
            ));
            assert_eq!(canonical, original);
            assert_eq!(registry, before_registry);
            assert_eq!(registry.state(&block_id), Ok(live(source, 0)));
        }
    }

    #[test]
    fn captured_single_line_registers_can_move_to_an_empty_page() {
        let mut cases = 0;
        for ending in ["\n", "\r\n"] {
            for content in ["", "α", "a ^literal", "🙂", "  "] {
                for tail in ["", " \t"] {
                    let source = page("source");
                    let target = page("target");
                    let block_id = id("block1");
                    let mut registry = FakeRegistry::default();
                    registry.seed(block_id.clone(), source.clone());
                    let mut original = format!("{}{tail}{ending}", block_line(content, "block1"));
                    let deletion =
                        plan_delete_edit(&source, &mut original, &block_id, &mut registry).unwrap();
                    let register = commit_planned_block_delete(deletion).unwrap();
                    assert_eq!(register.visible_text(), format!("{content}{tail}"));
                    let mut destination = String::new();
                    let paste = plan_paste_edit(
                        &target,
                        &mut destination,
                        0,
                        &register,
                        None,
                        &mut registry,
                    )
                    .unwrap();
                    commit_planned_identity_edit(paste).unwrap();

                    assert_eq!(
                        Doc::new(&destination).visible_projection().text(),
                        format!("{content}{tail}\n")
                    );
                    assert_eq!(registry.state(&block_id), Ok(live(target, 1)));
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 20);
    }

    #[test]
    fn paste_rejects_multiline_registers_without_mutating_the_destination() {
        for text in ["first\nsecond", "first\rsecond"] {
            let source = page("source");
            let target = page("target");
            let original_id = id("block1");
            let mut registry = FakeRegistry::default();
            registry.seed(original_id.clone(), source.clone());
            let before_registry = registry.clone();
            let register = BlockRegister::yank(text.to_owned(), original_id, source);
            let mut destination = "target ^dest01\n".to_owned();
            let before = destination.clone();
            let insertion_at = Doc::new(&destination)
                .visible_projection()
                .text()
                .chars()
                .count();

            assert!(matches!(
                plan_paste_edit(
                    &target,
                    &mut destination,
                    insertion_at,
                    &register,
                    Some(id("fresh1")),
                    &mut registry,
                ),
                Err(IdentityEditPlanError::Text(
                    super::IdentityTextPlanError::InvalidRegisterText
                ))
            ));
            assert_eq!(destination, before);
            assert_eq!(registry, before_registry);
        }
    }

    #[test]
    fn canonical_yank_paste_adds_a_fresh_identity_without_changing_the_source() {
        let source_page = page("page");
        let original_id = id("blk01");
        let fresh_id = id("blk02");
        let mut registry = FakeRegistry::default();
        registry.seed(original_id.clone(), source_page.clone());
        let mut canonical_text = "one ^blk01\r\nnext\r\n".to_owned();
        let register =
            BlockRegister::yank("one".to_owned(), original_id.clone(), source_page.clone());
        let insertion_at = Doc::new(&canonical_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        let plan = plan_paste_edit(
            &source_page,
            &mut canonical_text,
            insertion_at,
            &register,
            Some(fresh_id.clone()),
            &mut registry,
        )
        .unwrap();
        commit_planned_identity_edit(plan).unwrap();

        assert_eq!(canonical_text, "one ^blk01\r\nnext\r\none ^blk02\r\n");
        assert_eq!(
            block_ids_in_text(&canonical_text),
            [original_id.clone(), fresh_id.clone()]
        );
        assert_eq!(
            registry.state(&original_id),
            Ok(live(source_page.clone(), 0))
        );
        assert_eq!(registry.state(&fresh_id), Ok(live(source_page, 0)));
    }

    #[test]
    fn failed_canonical_paste_keeps_destination_text_and_registry_unchanged() {
        let target_page = page("target");
        let original_id = id("blk01");
        let fresh_id = id("blk02");
        let register = BlockRegister::yank("one".to_owned(), original_id.clone(), page("source"));
        let mut registry = FakeRegistry::default();
        registry.seed(original_id, page("source"));
        let expected_text = "target ^dest01\n".to_owned();
        let mut target_text = expected_text.clone();
        let insertion_at = Doc::new(&target_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        registry.fail_next_apply = true;
        let before_registry = registry.clone();
        let plan = plan_paste_edit(
            &target_page,
            &mut target_text,
            insertion_at,
            &register,
            Some(fresh_id),
            &mut registry,
        )
        .unwrap();

        assert_eq!(
            commit_planned_identity_edit(plan),
            Err("registry transaction failed")
        );
        assert_eq!(target_text, expected_text);
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn canonical_paste_rejects_a_plan_for_a_different_detachment_generation() {
        let source_page = page("source");
        let moved_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.seed(moved_id.clone(), source_page.clone());
        let (register, delete) =
            plan_delete(&source_page, &moved_id, "one".to_owned(), &registry).unwrap();
        registry.apply(&delete.changes).unwrap();
        let mut plan = plan_paste(&register, &page("target"), None, &registry).unwrap();
        let [RegistryChange::MoveDetached { generation, .. }] = plan.changes.as_mut_slice() else {
            panic!("expected a move plan");
        };
        *generation = DetachGeneration::new(2);
        let target_text = "target ^dest01\n";
        let insertion_at = Doc::new(target_text)
            .visible_projection()
            .text()
            .chars()
            .count();

        assert_eq!(
            super::plan_paste_block_text(target_text, insertion_at, &register, &plan),
            Err(super::IdentityTextPlanError::InvalidPastePlan)
        );
    }

    #[test]
    fn canonical_delete_rejects_missing_or_ambiguous_block_ids() {
        assert_eq!(
            plan_delete_block_text("plain text\n", &id("missing")),
            Err(super::IdentityTextPlanError::IdentityMismatch)
        );
        assert_eq!(
            plan_delete_block_text("one ^same1\ntwo ^same1\n", &id("same1")),
            Err(super::IdentityTextPlanError::IdentityMismatch)
        );
    }

    #[test]
    fn canonical_block_delete_preserves_crlf_and_adjacent_line_bytes() {
        let deletion =
            plan_delete_block_text("first ^first1\r\nlast ^last01", &id("last01")).unwrap();
        assert_eq!(deletion.visible_text, "last");
        assert_eq!(deletion.canonical_text, "first ^first1");
    }

    #[test]
    fn reservation_and_retirement_failures_leave_canonical_text_unchanged() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("left01"), page("page"));
        registry.seed(id("right1"), page("page"));
        let mut text = "left ^left01\nright ^right1\n".to_owned();
        registry.fail_next_apply = true;
        let before_registry = registry.clone();
        let join = plan_join_edit(
            &page("page"),
            &mut text,
            4..5,
            " ",
            &id("left01"),
            &id("right1"),
            &mut registry,
        )
        .unwrap();

        assert_eq!(
            commit_planned_identity_edit(join),
            Err("registry transaction failed")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
        assert_eq!(registry, before_registry);

        let split_before = "left ^left01\n".to_owned();
        let mut split_text = split_before.clone();
        let split = plan_split_edit(
            &page("page"),
            &mut split_text,
            4,
            &id("left01"),
            id("new001"),
            &mut registry,
        )
        .unwrap();
        assert_eq!(
            commit_planned_identity_edit(split),
            Err("registry transaction failed")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
        assert_eq!(split_text, split_before);
        assert_eq!(registry, before_registry);
    }

    #[test]
    fn a_late_registry_change_failure_rolls_back_earlier_changes_in_the_batch() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("already"), page("page"));
        let before = registry.clone();

        assert_eq!(
            registry.apply(&[
                RegistryChange::Reserve {
                    id: id("new001"),
                    page: page("page"),
                },
                RegistryChange::Retire {
                    id: id("missing"),
                    page: page("page"),
                    generation: DetachGeneration::INITIAL,
                },
            ]),
            Err("identity is not live at the expected page and generation")
        );
        assert_eq!(registry, before);
    }

    #[test]
    fn id_projection_ignores_legacy_mirror_tokens() {
        assert_eq!(
            block_ids_in_text("one ^block1\ntwo ^=mirror1\n"),
            [id("block1")]
        );
    }
}
