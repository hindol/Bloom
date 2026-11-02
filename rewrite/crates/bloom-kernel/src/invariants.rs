use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InvariantId {
    I1,
    I2,
    I3,
    I4,
    I5,
    I6,
    I7,
    I8,
    I9,
    I10,
    I11,
    I12,
}

impl InvariantId {
    pub const ALL: [Self; 12] = [
        Self::I1,
        Self::I2,
        Self::I3,
        Self::I4,
        Self::I5,
        Self::I6,
        Self::I7,
        Self::I8,
        Self::I9,
        Self::I10,
        Self::I11,
        Self::I12,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I1 => "I1",
            Self::I2 => "I2",
            Self::I3 => "I3",
            Self::I4 => "I4",
            Self::I5 => "I5",
            Self::I6 => "I6",
            Self::I7 => "I7",
            Self::I8 => "I8",
            Self::I9 => "I9",
            Self::I10 => "I10",
            Self::I11 => "I11",
            Self::I12 => "I12",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvariantCoverage {
    pub id: InvariantId,
    pub invariant: &'static str,
    pub owner: &'static str,
    pub checker: &'static str,
    pub test: &'static str,
    pub activation: &'static str,
}

pub const INVARIANT_COVERAGE: [InvariantCoverage; 12] = [
    InvariantCoverage {
        id: InvariantId::I1,
        invariant: "Round-trip valid unedited text",
        owner: "bloom-text::TextProps",
        checker: "check_round_trip",
        test: "text_props_round_trip",
        activation: "M2",
    },
    InvariantCoverage {
        id: InvariantId::I2,
        invariant: "Bounded crash loss and untorn files",
        owner: "bloom-state::TxnJournal",
        checker: "check_bounded_loss",
        test: "crash_recovery_simulation",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I3,
        invariant: "Page ID and path key are bijective",
        owner: "bloom-vault::VaultStore",
        checker: "check_page_path_bijection",
        test: "page_path_bijection",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I4,
        invariant: "Live block IDs are vault-wide unique",
        owner: "bloom-text::BlockId",
        checker: "check_unique_live_block_ids",
        test: "unique_live_block_ids",
        activation: "M2",
    },
    InvariantCoverage {
        id: InvariantId::I5,
        invariant: "Clean page matches observed disk base",
        owner: "bloom-vault::PageSync",
        checker: "check_clean_page_matches_base",
        test: "page_sync_external_write",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I6,
        invariant: "Positions are valid grapheme boundaries",
        owner: "bloom-text::Doc",
        checker: "check_cursor_grapheme_boundary",
        test: "active_grapheme_detector_catches_injected_cursor_faults",
        activation: "M1 raw cursor; M2 projected positions",
    },
    InvariantCoverage {
        id: InvariantId::I7,
        invariant: "Exactly one component owns input focus",
        owner: "bloom-kernel::Ui",
        checker: "check_exactly_one_focus_owner",
        test: "focus_stack_ownership",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I8,
        invariant: "Quiescent index equals latest projection",
        owner: "bloom-index::Projection",
        checker: "check_index_matches_versions",
        test: "index_full_rebuild_differential",
        activation: "M5",
    },
    InvariantCoverage {
        id: InvariantId::I9,
        invariant: "Writes preserve unmerged observed disk data",
        owner: "bloom-vault::CasWrite",
        checker: "check_write_respects_observed_base",
        test: "external_writer_cas",
        activation: "M4",
    },
    InvariantCoverage {
        id: InvariantId::I10,
        invariant: "No armed timer is due after update",
        owner: "bloom-kernel::TimerWheel",
        checker: "check_no_due_armed_timers",
        test: "due_timer_is_consumed",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I11,
        invariant: "View is pure for the same model",
        owner: "bloom-kernel::View",
        checker: "check_view_purity",
        test: "repeated_view_is_identical",
        activation: "M1",
    },
    InvariantCoverage {
        id: InvariantId::I12,
        invariant: "Undo then redo restores identical text and IDs",
        owner: "bloom-text::Undo",
        checker: "check_undo_redo_identity",
        test: "undo_redo_identity",
        activation: "M2",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub invariant: InvariantId,
    pub message: String,
}

impl Violation {
    fn new(invariant: InvariantId, message: String) -> Self {
        Self { invariant, message }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateRegistration(InvariantId),
    DuplicateApplicableInvariant(InvariantId),
    MissingCheckers(Vec<InvariantId>),
}

type Checker<'a> = Box<dyn Fn() -> Result<(), String> + 'a>;

#[derive(Default)]
pub struct CheckerRegistry<'a> {
    checkers: BTreeMap<InvariantId, Checker<'a>>,
}

impl<'a> CheckerRegistry<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        invariant: InvariantId,
        checker: impl Fn() -> Result<(), String> + 'a,
    ) -> Result<(), RegistryError> {
        if self.checkers.contains_key(&invariant) {
            return Err(RegistryError::DuplicateRegistration(invariant));
        }

        self.checkers.insert(invariant, Box::new(checker));
        Ok(())
    }

    pub fn run_applicable(
        &self,
        applicable: &[InvariantId],
    ) -> Result<Vec<Violation>, RegistryError> {
        let mut ordered = applicable.to_vec();
        ordered.sort_unstable();
        for pair in ordered.windows(2) {
            if pair[0] == pair[1] {
                return Err(RegistryError::DuplicateApplicableInvariant(pair[0]));
            }
        }

        let missing: Vec<_> = ordered
            .iter()
            .copied()
            .filter(|id| !self.checkers.contains_key(id))
            .collect();
        if !missing.is_empty() {
            return Err(RegistryError::MissingCheckers(missing));
        }

        Ok(ordered
            .into_iter()
            .filter_map(|id| {
                self.checkers
                    .get(&id)
                    .and_then(|checker| checker().err())
                    .map(|message| Violation::new(id, message))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COVERAGE_DOC: &str = include_str!("../../../../docs/planning/M0_INVARIANT_FRAMEWORK.md");

    #[test]
    fn synthetic_violation_is_reported_with_registered_invariant() {
        let mut registry = CheckerRegistry::new();
        registry
            .register(InvariantId::I10, || {
                Err("synthetic due timer remains armed".to_owned())
            })
            .unwrap();

        assert_eq!(
            registry.run_applicable(&[InvariantId::I10]).unwrap(),
            vec![Violation::new(
                InvariantId::I10,
                "synthetic due timer remains armed".to_owned()
            )]
        );
    }

    #[test]
    fn all_applicable_checkers_run_in_stable_order() {
        let mut registry = CheckerRegistry::new();
        registry
            .register(InvariantId::I2, || Err("second violation".to_owned()))
            .unwrap();
        registry
            .register(InvariantId::I1, || Err("first violation".to_owned()))
            .unwrap();

        assert_eq!(
            registry
                .run_applicable(&[InvariantId::I2, InvariantId::I1])
                .unwrap(),
            vec![
                Violation::new(InvariantId::I1, "first violation".to_owned()),
                Violation::new(InvariantId::I2, "second violation".to_owned()),
            ]
        );
    }

    #[test]
    fn missing_applicable_checker_is_an_error() {
        let registry = CheckerRegistry::new();

        assert_eq!(
            registry.run_applicable(&[InvariantId::I1]),
            Err(RegistryError::MissingCheckers(vec![InvariantId::I1]))
        );
    }

    #[test]
    fn duplicate_registration_and_applicability_are_rejected() {
        let mut registry = CheckerRegistry::new();
        registry.register(InvariantId::I1, || Ok(())).unwrap();

        assert_eq!(
            registry.register(InvariantId::I1, || Ok(())),
            Err(RegistryError::DuplicateRegistration(InvariantId::I1))
        );
        assert_eq!(
            registry.run_applicable(&[InvariantId::I1, InvariantId::I1]),
            Err(RegistryError::DuplicateApplicableInvariant(InvariantId::I1))
        );
    }

    #[test]
    fn coverage_metadata_maps_every_invariant_once() {
        let ids: Vec<_> = INVARIANT_COVERAGE.iter().map(|row| row.id).collect();
        assert_eq!(ids, InvariantId::ALL);
        assert!(INVARIANT_COVERAGE.iter().all(|row| {
            !row.invariant.is_empty()
                && !row.owner.is_empty()
                && !row.checker.is_empty()
                && !row.test.is_empty()
                && !row.activation.is_empty()
        }));
    }

    #[test]
    fn documentation_coverage_matches_executable_metadata() {
        let doc_rows: Vec<_> = COVERAGE_DOC
            .lines()
            .filter(|line| {
                InvariantId::ALL.iter().any(|id| {
                    line.split('|')
                        .nth(1)
                        .is_some_and(|cell| cell.trim() == id.as_str())
                })
            })
            .map(|line| {
                line.split('|')
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(|cell| cell.trim_matches('`').to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();

        assert_eq!(doc_rows.len(), INVARIANT_COVERAGE.len());
        for (doc, code) in doc_rows.iter().zip(INVARIANT_COVERAGE) {
            assert_eq!(doc.len(), 6);
            assert_eq!(doc[0], code.id.as_str());
            assert_eq!(doc[1], code.invariant);
            assert_eq!(doc[2], code.owner);
            assert_eq!(doc[3], code.checker);
            assert_eq!(doc[4], code.test);
            assert_eq!(doc[5], code.activation);
        }
    }
}
