use bloom_text::{BlockIdMarker, Doc};

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
        block_ids_in_text, commit_identity_edit, plan_delete, plan_join, plan_paste, plan_split,
        BlockId, BlockIdRegistry, BlockRegister, IdentityDecision, IdentityPlanError,
        IdentityState, PageId, RegisterIdentity, RegisterOperation, RegistryChange,
    };

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
