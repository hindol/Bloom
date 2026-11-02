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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityTextPlanError {
    Projection(ProjectionError),
    UnsupportedTokenSet,
    IdentityMismatch,
    InvalidJoinEdit,
    InvalidSplitEdit,
    InvalidResult,
    LineOutOfBounds(usize),
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
        append_block_id, block_ids_in_text, commit_identity_edit, plan_delete, plan_join,
        plan_join_text, plan_paste, plan_split, plan_split_text, BlockId, BlockIdRegistry,
        BlockRegister, IdentityDecision, IdentityPlanError, IdentityState, PageId,
        RegisterIdentity, RegisterOperation, RegistryChange,
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
        let plan = plan_join(&page("page"), &retained, &retired, &registry).unwrap();
        let planned_text =
            plan_join_text(&canonical, start..end, &replacement, &retained, &retired).unwrap();
        let mut actual_text = canonical;

        commit_identity_edit(&mut registry, &mut actual_text, planned_text, &plan).unwrap();

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
        let plan = plan_split(&page("page"), &original, allocated.clone(), &registry).unwrap();
        let planned_text = plan_split_text(&canonical, start, &original, &allocated).unwrap();
        let mut actual_text = canonical;

        commit_identity_edit(&mut registry, &mut actual_text, planned_text, &plan).unwrap();

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
    fn reservation_and_retirement_failures_leave_canonical_text_unchanged() {
        let mut registry = FakeRegistry::default();
        registry.seed(id("left01"), page("page"));
        registry.seed(id("right1"), page("page"));
        let join = plan_join(&page("page"), &id("left01"), &id("right1"), &registry).unwrap();
        let mut text = "left ^left01\nright ^right1\n".to_owned();
        registry.fail_next_apply = true;
        let before_registry = registry.clone();

        assert_eq!(
            commit_identity_edit(
                &mut registry,
                &mut text,
                "left right ^left01\n".to_owned(),
                &join
            ),
            Err("registry transaction failed")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
        assert_eq!(registry, before_registry);

        let split = plan_split(&page("page"), &id("left01"), id("new001"), &registry).unwrap();
        assert_eq!(
            commit_identity_edit(
                &mut registry,
                &mut text,
                "left\nright ^left01\n".to_owned(),
                &split
            ),
            Err("registry transaction failed")
        );
        assert_eq!(text, "left ^left01\nright ^right1\n");
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
