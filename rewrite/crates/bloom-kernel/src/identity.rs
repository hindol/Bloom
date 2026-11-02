use bloom_text::{BlockIdMarker, Doc, ProjectionError};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityState {
    Available,
    Live(PageId),
    Detached(PageId),
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryChange {
    Reserve {
        id: BlockId,
        page: PageId,
    },
    Detach {
        id: BlockId,
        page: PageId,
    },
    MoveDetached {
        id: BlockId,
        source_page: PageId,
        target_page: PageId,
    },
    RetireDetached {
        id: BlockId,
        page: PageId,
    },
    Retire {
        id: BlockId,
    },
}

/// `apply` must commit the complete change set or leave the registry unchanged.
pub trait BlockIdRegistry {
    type Error;

    fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error>;
    fn apply(&mut self, changes: &[RegistryChange]) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterOperation {
    Yank,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterIdentity {
    pub id: BlockId,
    pub source_page: PageId,
    pub operation: RegisterOperation,
    pub live_at_capture: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockRegister {
    pub visible_text: String,
    pub identity: Option<RegisterIdentity>,
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
    InvalidResult,
    LineOutOfBounds(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityEditPlanError<E> {
    Identity(IdentityPlanError<E>),
    Text(IdentityTextPlanError),
}

/// Complete structural text and registry changes, ready for one atomic commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalIdentityEditPlan {
    canonical_text: String,
    identity: IdentityPlan,
}

pub fn plan_delete<R: BlockIdRegistry>(
    page: &PageId,
    id: &BlockId,
    visible_text: String,
    registry: &R,
) -> Result<(BlockRegister, IdentityPlan), IdentityPlanError<R::Error>> {
    if registry.state(id).map_err(IdentityPlanError::Registry)? != IdentityState::Live(page.clone())
    {
        return Err(IdentityPlanError::DeleteSourceNotLive(id.clone()));
    }

    Ok((
        BlockRegister {
            visible_text,
            identity: Some(RegisterIdentity {
                id: id.clone(),
                source_page: page.clone(),
                operation: RegisterOperation::Delete,
                live_at_capture: true,
            }),
        },
        IdentityPlan {
            decision: IdentityDecision::Delete {
                detached_id: id.clone(),
            },
            changes: vec![RegistryChange::Detach {
                id: id.clone(),
                page: page.clone(),
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
    let (assigned_id, reused_source_id) = match source_state {
        IdentityState::Detached(source_page)
            if identity.operation == RegisterOperation::Delete
                && identity.source_page == source_page =>
        {
            (identity.id.clone(), true)
        }
        IdentityState::Available
        | IdentityState::Live(_)
        | IdentityState::Detached(_)
        | IdentityState::Retired => {
            let fresh_id = fresh_id.ok_or(IdentityPlanError::InvalidFreshId)?;
            if fresh_id == identity.id
                || registry
                    .state(&fresh_id)
                    .map_err(IdentityPlanError::Registry)?
                    != IdentityState::Available
            {
                return Err(IdentityPlanError::FreshIdUnavailable(fresh_id));
            }
            (fresh_id, false)
        }
    };

    let change = if reused_source_id {
        RegistryChange::MoveDetached {
            id: assigned_id.clone(),
            source_page: identity.source_page.clone(),
            target_page: target_page.clone(),
        }
    } else {
        RegistryChange::Reserve {
            id: assigned_id.clone(),
            page: target_page.clone(),
        }
    };

    Ok(IdentityPlan {
        decision: IdentityDecision::Paste {
            assigned_id: Some(assigned_id.clone()),
            reused_source_id,
        },
        changes: vec![change],
    })
}

pub fn plan_discard_register<R: BlockIdRegistry>(
    register: &BlockRegister,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    let Some(identity) = &register.identity else {
        return Ok(IdentityPlan {
            decision: IdentityDecision::DiscardRegister { retired_id: None },
            changes: Vec::new(),
        });
    };
    if identity.operation != RegisterOperation::Delete {
        return Ok(IdentityPlan {
            decision: IdentityDecision::DiscardRegister { retired_id: None },
            changes: Vec::new(),
        });
    }

    match registry
        .state(&identity.id)
        .map_err(IdentityPlanError::Registry)?
    {
        IdentityState::Detached(source_page) if source_page == identity.source_page => {
            Ok(IdentityPlan {
                decision: IdentityDecision::DiscardRegister {
                    retired_id: Some(identity.id.clone()),
                },
                changes: vec![RegistryChange::RetireDetached {
                    id: identity.id.clone(),
                    page: identity.source_page.clone(),
                }],
            })
        }
        IdentityState::Detached(_) => Err(IdentityPlanError::DetachedRegisterPageMismatch(
            identity.id.clone(),
        )),
        IdentityState::Available | IdentityState::Live(_) | IdentityState::Retired => {
            Ok(IdentityPlan {
                decision: IdentityDecision::DiscardRegister { retired_id: None },
                changes: Vec::new(),
            })
        }
    }
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
    for id in [retained_id, retired_id] {
        if registry.state(id).map_err(IdentityPlanError::Registry)?
            != IdentityState::Live(page.clone())
        {
            return Err(IdentityPlanError::JoinedIdNotLive(id.clone()));
        }
    }

    Ok(IdentityPlan {
        decision: IdentityDecision::Join {
            retained_id: retained_id.clone(),
            retired_id: retired_id.clone(),
        },
        changes: vec![RegistryChange::Retire {
            id: retired_id.clone(),
        }],
    })
}

pub fn plan_split<R: BlockIdRegistry>(
    page: &PageId,
    left_id: &BlockId,
    right_id: BlockId,
    registry: &R,
) -> Result<IdentityPlan, IdentityPlanError<R::Error>> {
    if registry
        .state(left_id)
        .map_err(IdentityPlanError::Registry)?
        != IdentityState::Live(page.clone())
    {
        return Err(IdentityPlanError::SplitSourceNotLive(left_id.clone()));
    }
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
        changes: vec![RegistryChange::Reserve {
            id: right_id,
            page: page.clone(),
        }],
    })
}

/// Applies a precomputed canonical-text result only after the registry accepts
/// its complete change set. `String` replacement is infallible after that point.
pub fn commit_identity_edit<R: BlockIdRegistry>(
    registry: &mut R,
    canonical_text: &mut String,
    planned_text: String,
    plan: &IdentityPlan,
) -> Result<(), R::Error> {
    registry.apply(&plan.changes)?;
    *canonical_text = planned_text;
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
    if projection.tokens().len() != 2
        || projection.tokens()[0].marker() != BlockIdMarker::Block
        || projection.tokens()[1].marker() != BlockIdMarker::Block
    {
        return Err(IdentityTextPlanError::UnsupportedTokenSet);
    }
    if projection.tokens()[0].id() != retained_id.as_str()
        || projection.tokens()[1].id() != retired_id.as_str()
        || retained_id == retired_id
    {
        return Err(IdentityTextPlanError::IdentityMismatch);
    }

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
    if line_at(visible, projection.tokens()[0].visible_boundary().get()) != joined_line
        || line_at(visible, projection.tokens()[1].visible_boundary().get()) != joined_line + 1
    {
        return Err(IdentityTextPlanError::InvalidJoinEdit);
    }
    let edited = replace_chars(visible, visible_range, replacement);
    let planned = append_block_id(&edited, joined_line, retained_id)?;
    validate_planned_text(&planned, &edited, &[retained_id])?;
    Ok(planned)
}

pub fn plan_split_text(
    canonical_text: &str,
    split_at: usize,
    left_id: &BlockId,
    right_id: &BlockId,
) -> Result<String, IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    if projection.tokens().len() != 1 || projection.tokens()[0].marker() != BlockIdMarker::Block {
        return Err(IdentityTextPlanError::UnsupportedTokenSet);
    }
    if projection.tokens()[0].id() != left_id.as_str() || left_id == right_id {
        return Err(IdentityTextPlanError::IdentityMismatch);
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
    if line_at(visible, projection.tokens()[0].visible_boundary().get()) != left_line {
        return Err(IdentityTextPlanError::InvalidSplitEdit);
    }
    let line_ending = line_ending_at(visible, split_at);
    let edited = replace_chars(visible, split_at..split_at, line_ending);
    let with_left_id = append_block_id(&edited, left_line, left_id)?;
    let planned = append_block_id(&with_left_id, left_line + 1, right_id)?;
    validate_planned_text(&planned, &edited, &[left_id, right_id])?;
    Ok(planned)
}

/// Plans join text and identity changes together; neither is applied on error.
pub fn plan_join_edit<R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &str,
    visible_range: std::ops::Range<usize>,
    replacement: &str,
    retained_id: &BlockId,
    retired_id: &BlockId,
    registry: &R,
) -> Result<CanonicalIdentityEditPlan, IdentityEditPlanError<R::Error>> {
    let identity = plan_join(page, retained_id, retired_id, registry)
        .map_err(IdentityEditPlanError::Identity)?;
    let canonical_text = plan_join_text(
        canonical_text,
        visible_range,
        replacement,
        retained_id,
        retired_id,
    )
    .map_err(IdentityEditPlanError::Text)?;
    Ok(CanonicalIdentityEditPlan {
        canonical_text,
        identity,
    })
}

/// Plans split text and identity changes together; neither is applied on error.
pub fn plan_split_edit<R: BlockIdRegistry>(
    page: &PageId,
    canonical_text: &str,
    split_at: usize,
    left_id: &BlockId,
    right_id: BlockId,
    registry: &R,
) -> Result<CanonicalIdentityEditPlan, IdentityEditPlanError<R::Error>> {
    let identity = plan_split(page, left_id, right_id.clone(), registry)
        .map_err(IdentityEditPlanError::Identity)?;
    let canonical_text = plan_split_text(canonical_text, split_at, left_id, &right_id)
        .map_err(IdentityEditPlanError::Text)?;
    Ok(CanonicalIdentityEditPlan {
        canonical_text,
        identity,
    })
}

/// Applies a fully validated structural edit to registry and canonical text.
pub fn commit_planned_identity_edit<R: BlockIdRegistry>(
    registry: &mut R,
    canonical_text: &mut String,
    plan: CanonicalIdentityEditPlan,
) -> Result<(), R::Error> {
    commit_identity_edit(
        registry,
        canonical_text,
        plan.canonical_text,
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
    let visible_text = visible_chars[visible_line_start..visible_line_end]
        .iter()
        .collect::<String>();

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
                id, source_page, ..
            }],
        ) => id == assigned_id && source_page == &identity.source_page && id == &identity.id,
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
    if register
        .visible_text
        .chars()
        .any(|character| character == '\n' || character == '\r')
    {
        return Err(IdentityTextPlanError::InvalidRegisterText);
    }

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

fn validate_planned_text(
    canonical_text: &str,
    visible_text: &str,
    ids: &[&BlockId],
) -> Result<(), IdentityTextPlanError> {
    let projection = Doc::new(canonical_text).visible_projection();
    if projection.text() != visible_text
        || projection.tokens().len() != ids.len()
        || projection
            .tokens()
            .iter()
            .zip(ids)
            .any(|(token, id)| token.marker() != BlockIdMarker::Block || token.id() != id.as_str())
    {
        return Err(IdentityTextPlanError::InvalidResult);
    }
    Ok(())
}

fn line_at(text: &str, char_offset: usize) -> usize {
    text.chars()
        .take(char_offset)
        .filter(|ch| *ch == '\n')
        .count()
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
    let line_start = if line_index == 0 {
        0
    } else {
        text.char_indices()
            .filter(|(_, ch)| *ch == '\n')
            .nth(line_index - 1)
            .map_or(text.len(), |(byte, _)| byte + 1)
    };
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |offset| line_start + offset);
    if line_index > text.chars().filter(|ch| *ch == '\n').count() {
        return Err(IdentityTextPlanError::LineOutOfBounds(line_index));
    }
    let content_end = if line_end > line_start && text.as_bytes()[line_end - 1] == b'\r' {
        line_end - 1
    } else {
        line_end
    };
    let prefix = if content_end == line_start { "" } else { " " };
    let insertion = format!("{prefix}^{}", id.as_str());
    let mut result = text.to_owned();
    result.insert_str(content_end, &insertion);
    Ok(result)
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
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        append_block_id, block_ids_in_text, commit_identity_edit, commit_planned_identity_edit,
        plan_delete, plan_delete_block_text, plan_discard_register, plan_join, plan_join_edit,
        plan_join_text, plan_paste, plan_paste_block_text, plan_split, plan_split_edit,
        plan_split_text, BlockId, BlockIdRegistry, BlockRegister, IdentityDecision,
        IdentityEditPlanError, IdentityPlanError, IdentityState, PageId, RegisterIdentity,
        RegisterOperation, RegistryChange,
    };
    use bloom_text::Doc;

    const ADR3_IDENTITY_ORACLE: &str =
        include_str!("../../../../testdata/adr3_vim_identity_oracle.v1");

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct FakeRegistry {
        live: BTreeMap<BlockId, PageId>,
        detached: BTreeMap<BlockId, PageId>,
        retired: BTreeSet<BlockId>,
        fail_next_apply: bool,
    }

    impl FakeRegistry {
        fn seed(&mut self, id: BlockId, page: PageId) {
            self.live.insert(id, page);
        }
    }

    impl BlockIdRegistry for FakeRegistry {
        type Error = &'static str;

        fn state(&self, id: &BlockId) -> Result<IdentityState, Self::Error> {
            Ok(if let Some(page) = self.live.get(id) {
                IdentityState::Live(page.clone())
            } else if let Some(page) = self.detached.get(id) {
                IdentityState::Detached(page.clone())
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
                    RegistryChange::Reserve { id, page } => {
                        if staged.state(id)? != IdentityState::Available {
                            return Err("identity is not available");
                        }
                        staged.live.insert(id.clone(), page.clone());
                    }
                    RegistryChange::Detach { id, page } => {
                        if staged.state(id)? != IdentityState::Live(page.clone()) {
                            return Err("identity is not live on page");
                        }
                        staged.live.remove(id);
                        staged.detached.insert(id.clone(), page.clone());
                    }
                    RegistryChange::MoveDetached {
                        id,
                        source_page,
                        target_page,
                    } => {
                        if staged.state(id)? != IdentityState::Detached(source_page.clone()) {
                            return Err("identity is not detached from page");
                        }
                        staged.detached.remove(id);
                        staged.live.insert(id.clone(), target_page.clone());
                    }
                    RegistryChange::RetireDetached { id, page } => {
                        if staged.state(id)? != IdentityState::Detached(page.clone()) {
                            return Err("identity is not detached from page");
                        }
                        staged.detached.remove(id);
                        staged.retired.insert(id.clone());
                    }
                    RegistryChange::Retire { id } => {
                        if staged.live.remove(id).is_none() {
                            return Err("identity is not live");
                        }
                        staged.retired.insert(id.clone());
                    }
                }
            }
            *self = staged;
            Ok(())
        }
    }

    fn page(value: &str) -> PageId {
        PageId::new(value).unwrap()
    }

    fn id(value: &str) -> BlockId {
        BlockId::new(value).unwrap()
    }

    fn register(value: &str, operation: RegisterOperation, live_at_capture: bool) -> BlockRegister {
        BlockRegister {
            visible_text: "block text".to_owned(),
            identity: Some(RegisterIdentity {
                id: id(value),
                source_page: page("source"),
                operation,
                live_at_capture,
            }),
        }
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
            Ok(IdentityState::Detached(page("source")))
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
        assert_eq!(
            registry.state(&id("move01")),
            Ok(IdentityState::Live(page("target")))
        );
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
        assert_eq!(
            registry.state(&moved_id),
            Ok(IdentityState::Live(target_page))
        );
    }

    #[test]
    fn discarding_yank_register_does_not_change_live_identity_state() {
        let original_id = id("copy01");
        let mut registry = FakeRegistry::default();
        registry.seed(original_id.clone(), page("source"));

        let plan = plan_discard_register(
            &register("copy01", RegisterOperation::Yank, true),
            &registry,
        )
        .unwrap();

        assert_eq!(
            plan.decision,
            IdentityDecision::DiscardRegister { retired_id: None }
        );
        assert!(plan.changes.is_empty());
        assert_eq!(
            registry.state(&original_id),
            Ok(IdentityState::Live(page("source")))
        );
    }

    #[test]
    fn discarding_a_stale_move_register_rejects_another_pages_detached_id() {
        let move_id = id("move01");
        let mut registry = FakeRegistry::default();
        registry.detached.insert(move_id.clone(), page("other"));

        assert_eq!(
            plan_discard_register(
                &register("move01", RegisterOperation::Delete, true),
                &registry
            ),
            Err(IdentityPlanError::DetachedRegisterPageMismatch(move_id))
        );
    }

    #[test]
    fn yank_then_paste_allocates_a_fresh_identity_while_source_is_live() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("copy01"), page("source"));
        let copied = register("copy01", RegisterOperation::Yank, true);
        let plan = plan_paste(&copied, &page("target"), Some(id("copy02")), &registry).unwrap();
        assert_eq!(
            plan.decision,
            IdentityDecision::Paste {
                assigned_id: Some(id("copy02")),
                reused_source_id: false,
            }
        );
        registry.apply(&plan.changes).unwrap();
        assert_eq!(
            registry.state(&id("copy01")),
            Ok(IdentityState::Live(page("source")))
        );
        assert_eq!(
            registry.state(&id("copy02")),
            Ok(IdentityState::Live(page("target")))
        );
    }

    #[test]
    fn stale_yank_cannot_claim_an_identity_detached_for_a_move_register() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("copy01"), page("source"));
        let copied = register("copy01", RegisterOperation::Yank, true);
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
            Ok(IdentityState::Detached(page("source")))
        );
    }

    #[test]
    fn retired_or_colliding_ids_are_never_reused_by_paste() {
        let mut registry = FakeRegistry::default();
        registry.retired.insert(id("old001"));
        registry.seed(id("taken1"), page("other"));
        let copied = register("old001", RegisterOperation::Delete, false);

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
        let copied = register("source1", RegisterOperation::Yank, true);

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
        assert_eq!(
            registry.state(&id("left01")),
            Ok(IdentityState::Live(page("page")))
        );
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
        assert_eq!(
            registry.state(&id("split1")),
            Ok(IdentityState::Live(page("page")))
        );
        assert_eq!(
            registry.state(&id("split2")),
            Ok(IdentityState::Live(page("page")))
        );
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
        let plan = plan_join_edit(
            &page("page"),
            &canonical,
            start..end,
            &replacement,
            &retained,
            &retired,
            &registry,
        )
        .unwrap();
        let mut actual_text = canonical;

        commit_planned_identity_edit(&mut registry, &mut actual_text, plan).unwrap();

        assert_eq!(actual_text, "left right ^left01\n");
        assert_eq!(
            Doc::new(&actual_text).visible_projection().text(),
            decode_oracle_text(fields[5])
        );
        assert_eq!(
            block_ids_in_text(&actual_text).as_slice(),
            std::slice::from_ref(&retained)
        );
        assert_eq!(
            registry.state(&retained),
            Ok(IdentityState::Live(page("page")))
        );
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
        let plan = plan_split_edit(
            &page("page"),
            &canonical,
            start,
            &original,
            allocated.clone(),
            &registry,
        )
        .unwrap();
        let mut actual_text = canonical;

        commit_planned_identity_edit(&mut registry, &mut actual_text, plan).unwrap();

        assert_eq!(actual_text, "alpha ^split1\n beta ^split2\n");
        assert_eq!(
            Doc::new(&actual_text).visible_projection().text(),
            decode_oracle_text(fields[5])
        );
        assert_eq!(
            block_ids_in_text(&actual_text),
            [original.clone(), allocated.clone()]
        );
        assert_eq!(
            registry.state(&original),
            Ok(IdentityState::Live(page("page")))
        );
        assert_eq!(
            registry.state(&allocated),
            Ok(IdentityState::Live(page("page")))
        );
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
    fn structural_text_plans_reject_additional_tokens() {
        assert_eq!(
            plan_join_text(
                "left ^left01\nright ^right1\nother ^third\n",
                4..5,
                " ",
                &id("left01"),
                &id("right1")
            ),
            Err(super::IdentityTextPlanError::UnsupportedTokenSet)
        );
        assert_eq!(
            plan_split_text(
                "alpha beta ^split1\nother ^third\n",
                5,
                &id("split1"),
                &id("split2")
            ),
            Err(super::IdentityTextPlanError::UnsupportedTokenSet)
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
        let canonical = "a `bc` ^split1\n";
        let original = id("split1");
        let mut registry = FakeRegistry::default();
        registry.seed(original.clone(), page("page"));
        let before_registry = registry.clone();

        assert_eq!(
            plan_split_edit(
                &page("page"),
                canonical,
                4,
                &original,
                id("split2"),
                &registry
            ),
            Err(IdentityEditPlanError::Text(
                super::IdentityTextPlanError::InvalidResult
            ))
        );
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
        registry.seed(destination_id.clone(), target_page.clone());
        let mut source_text = "one ^blk01\nnext ^blk02\n".to_owned();

        let deletion = plan_delete_block_text(&source_text, &moved_id).unwrap();
        assert_eq!(deletion.visible_text, "one");
        let (register, detach_plan) = plan_delete(
            &source_page,
            &moved_id,
            deletion.visible_text.clone(),
            &registry,
        )
        .unwrap();
        commit_identity_edit(
            &mut registry,
            &mut source_text,
            deletion.canonical_text,
            &detach_plan,
        )
        .unwrap();
        assert_eq!(source_text, "next ^blk02\n");

        let mut target_text = "target ^dest01\n".to_owned();
        let target_projection = Doc::new(&target_text).visible_projection();
        let paste_plan = plan_paste(&register, &target_page, None, &registry).unwrap();
        let planned_target = plan_paste_block_text(
            &target_text,
            target_projection.text().chars().count(),
            &register,
            &paste_plan,
        )
        .unwrap();
        commit_identity_edit(&mut registry, &mut target_text, planned_target, &paste_plan).unwrap();

        assert_eq!(target_text, "target ^dest01\none ^blk01\n");
        assert_eq!(
            registry.state(&moved_id),
            Ok(IdentityState::Live(target_page))
        );
        assert_eq!(Doc::new(&source_text).visible_projection().text(), "next\n");
        assert_eq!(
            Doc::new(&target_text).visible_projection().text(),
            "target\none\n"
        );
    }

    #[test]
    fn canonical_yank_paste_adds_a_fresh_identity_without_changing_the_source() {
        let source_page = page("page");
        let original_id = id("blk01");
        let fresh_id = id("blk02");
        let mut registry = FakeRegistry::default();
        registry.seed(original_id.clone(), source_page.clone());
        let mut canonical_text = "one ^blk01\r\nnext\r\n".to_owned();
        let register = BlockRegister {
            visible_text: "one".to_owned(),
            identity: Some(RegisterIdentity {
                id: original_id.clone(),
                source_page: source_page.clone(),
                operation: RegisterOperation::Yank,
                live_at_capture: true,
            }),
        };
        let insertion_at = Doc::new(&canonical_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        let plan = plan_paste(&register, &source_page, Some(fresh_id.clone()), &registry).unwrap();
        let planned_text =
            plan_paste_block_text(&canonical_text, insertion_at, &register, &plan).unwrap();

        commit_identity_edit(&mut registry, &mut canonical_text, planned_text, &plan).unwrap();

        assert_eq!(canonical_text, "one ^blk01\r\nnext\r\none ^blk02\r\n");
        assert_eq!(
            block_ids_in_text(&canonical_text),
            [original_id.clone(), fresh_id.clone()]
        );
        assert_eq!(
            registry.state(&original_id),
            Ok(IdentityState::Live(source_page.clone()))
        );
        assert_eq!(
            registry.state(&fresh_id),
            Ok(IdentityState::Live(source_page))
        );
    }

    #[test]
    fn failed_canonical_paste_keeps_destination_text_and_registry_unchanged() {
        let target_page = page("target");
        let original_id = id("blk01");
        let fresh_id = id("blk02");
        let register = BlockRegister {
            visible_text: "one".to_owned(),
            identity: Some(RegisterIdentity {
                id: original_id.clone(),
                source_page: page("source"),
                operation: RegisterOperation::Yank,
                live_at_capture: true,
            }),
        };
        let mut registry = FakeRegistry::default();
        registry.seed(original_id, page("source"));
        let target_text = "target ^dest01\n".to_owned();
        let insertion_at = Doc::new(&target_text)
            .visible_projection()
            .text()
            .chars()
            .count();
        let plan = plan_paste(&register, &target_page, Some(fresh_id), &registry).unwrap();
        let planned_text =
            plan_paste_block_text(&target_text, insertion_at, &register, &plan).unwrap();
        let mut actual_text = target_text.clone();
        let before_registry = registry.clone();
        registry.fail_next_apply = true;

        assert_eq!(
            commit_identity_edit(&mut registry, &mut actual_text, planned_text, &plan),
            Err("registry transaction failed")
        );
        assert_eq!(actual_text, target_text);
        assert_eq!(registry.live, before_registry.live);
        assert_eq!(registry.detached, before_registry.detached);
        assert_eq!(registry.retired, before_registry.retired);
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
        let join = plan_join_edit(
            &page("page"),
            &text,
            4..5,
            " ",
            &id("left01"),
            &id("right1"),
            &registry,
        )
        .unwrap();
        registry.fail_next_apply = true;
        let before_registry = registry.clone();

        assert_eq!(
            commit_planned_identity_edit(&mut registry, &mut text, join),
            Err("registry transaction failed")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
        assert_eq!(registry, before_registry);

        let split_before = "left ^left01\n".to_owned();
        let mut split_text = split_before.clone();
        let split = plan_split_edit(
            &page("page"),
            &split_text,
            4,
            &id("left01"),
            id("new001"),
            &registry,
        )
        .unwrap();
        assert_eq!(
            commit_planned_identity_edit(&mut registry, &mut split_text, split),
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
                RegistryChange::Retire { id: id("missing") },
            ]),
            Err("identity is not live")
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
