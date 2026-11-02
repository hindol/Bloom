#![cfg(feature = "simulation")]

use std::time::Duration;

use bloom_kernel::{
    invariants::{CheckerRegistry, InvariantId, RegistryError, Violation},
    simulation::{run_replay, ReplayCase, ReplayResult},
    timer::{check_no_due_armed_timers, TimerId, TimerWheel},
};
use bloom_text::{coordinates::RawCharIdx, Doc, DocError, DocSnapshot};

const REPLAY_FIXTURE: &str = include_str!("../../../testdata/replays/empty-kernel-smoke.sim");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultStatus {
    Active,
    Staged(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    CorruptSyntheticReplayOutput,
    AcceptStaleIndexResult,
    SkipBlockIdUniquenessCheck,
    PlaceCursorInsideGrapheme,
    LeaveDueTimerArmed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Detector {
    ReplayDeterminismComparison,
    CheckIndexMatchesVersions,
    CheckUniqueLiveBlockIds,
    CheckPositionsAreValid,
    CheckNoDueArmedTimers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FaultCase {
    fault: Fault,
    detector: Detector,
    status: FaultStatus,
}

const FAULT_CORPUS: [FaultCase; 5] = [
    FaultCase {
        fault: Fault::CorruptSyntheticReplayOutput,
        detector: Detector::ReplayDeterminismComparison,
        status: FaultStatus::Active,
    },
    FaultCase {
        fault: Fault::AcceptStaleIndexResult,
        detector: Detector::CheckIndexMatchesVersions,
        status: FaultStatus::Staged("M5"),
    },
    FaultCase {
        fault: Fault::SkipBlockIdUniquenessCheck,
        detector: Detector::CheckUniqueLiveBlockIds,
        status: FaultStatus::Staged("M2"),
    },
    FaultCase {
        fault: Fault::PlaceCursorInsideGrapheme,
        detector: Detector::CheckPositionsAreValid,
        status: FaultStatus::Staged("M2"),
    },
    FaultCase {
        fault: Fault::LeaveDueTimerArmed,
        detector: Detector::CheckNoDueArmedTimers,
        status: FaultStatus::Active,
    },
];

#[test]
fn active_replay_fault_is_caught_by_its_named_detector() {
    let active: Vec<_> = FAULT_CORPUS
        .iter()
        .filter(|case| {
            case.status == FaultStatus::Active && case.fault == Fault::CorruptSyntheticReplayOutput
        })
        .collect();
    assert_eq!(active.len(), 1);

    for case in active {
        let input = ReplayCase::parse(REPLAY_FIXTURE).unwrap();
        let reference_run = run_replay(&input).unwrap();
        let mut faulted_run = run_replay(&input).unwrap();
        inject(case.fault, &mut faulted_run);

        assert!(
            detect(case.detector, &reference_run, &faulted_run),
            "detector {:?} missed fault {:?}",
            case.detector,
            case.fault
        );
    }
}

#[test]
fn active_due_timer_fault_is_caught_by_its_named_detector() {
    let case = FAULT_CORPUS
        .iter()
        .find(|case| case.fault == Fault::LeaveDueTimerArmed)
        .unwrap();
    assert_eq!(case.status, FaultStatus::Active);

    let now = Duration::from_secs(2);
    let mut faulted_wheel = TimerWheel::default();
    faulted_wheel.schedule(TimerId::new(1), now);

    assert!(
        check_no_due_armed_timers(&faulted_wheel, now).is_err(),
        "detector {:?} missed fault {:?}",
        case.detector,
        case.fault
    );
}

#[test]
fn future_product_faults_are_registered_as_staged_not_passing() {
    let staged: Vec<_> = FAULT_CORPUS
        .iter()
        .filter_map(|case| match case.status {
            FaultStatus::Active => None,
            FaultStatus::Staged(milestone) => Some((case, milestone)),
        })
        .collect();

    assert_eq!(staged.len(), 3);
    assert!(staged.iter().all(|(case, milestone)| {
        !milestone.is_empty()
            && matches!(
                (case.fault, case.detector),
                (
                    Fault::AcceptStaleIndexResult,
                    Detector::CheckIndexMatchesVersions
                ) | (
                    Fault::SkipBlockIdUniquenessCheck,
                    Detector::CheckUniqueLiveBlockIds
                ) | (
                    Fault::PlaceCursorInsideGrapheme,
                    Detector::CheckPositionsAreValid
                )
            )
    }));
}

#[derive(Debug, Eq, PartialEq)]
enum PlainCursorProbeError {
    HiddenTokensUnsupported,
    InvalidRawPosition(DocError),
    Registry(RegistryError),
}

fn probe_plain_cursor_grapheme_boundary(
    snapshot: &DocSnapshot,
    cursor: RawCharIdx,
) -> Result<Vec<Violation>, PlainCursorProbeError> {
    let projection = snapshot.visible_projection();
    if !projection.tokens().is_empty() {
        return Err(PlainCursorProbeError::HiddenTokensUnsupported);
    }
    let offset = snapshot
        .raw_char_offset(cursor)
        .map_err(PlainCursorProbeError::InvalidRawPosition)?;
    let mut registry = CheckerRegistry::new();
    registry
        .register(InvariantId::I6, || {
            projection
                .range(offset, offset)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        })
        .map_err(PlainCursorProbeError::Registry)?;
    registry
        .run_applicable(&[InvariantId::I6])
        .map_err(PlainCursorProbeError::Registry)
}

#[test]
fn staged_grapheme_probe_catches_injected_cursor_faults() {
    let case = FAULT_CORPUS
        .iter()
        .find(|case| case.fault == Fault::PlaceCursorInsideGrapheme)
        .unwrap();
    assert_eq!(case.detector, Detector::CheckPositionsAreValid);
    assert_eq!(case.status, FaultStatus::Staged("M2"));

    let cases: [(&str, &[usize], &[usize]); 5] = [
        ("", &[0], &[]),
        ("e\u{301}A", &[0, 2, 3], &[1]),
        ("👩\u{200d}💻A", &[0, 3, 4], &[1, 2]),
        ("🇮🇳A", &[0, 2, 3], &[1]),
        ("\r\nA", &[0, 2, 3], &[1]),
    ];
    for (text, valid_offsets, injected_offsets) in cases {
        let snapshot = Doc::new(text).snapshot();
        for &offset in valid_offsets {
            let cursor = snapshot.raw_char_idx(offset).unwrap();
            assert!(
                probe_plain_cursor_grapheme_boundary(&snapshot, cursor)
                    .unwrap()
                    .is_empty(),
                "valid boundary {offset} in {text:?}"
            );
        }
        for &offset in injected_offsets {
            let cursor = snapshot.raw_char_idx(offset).unwrap();
            let violations = probe_plain_cursor_grapheme_boundary(&snapshot, cursor).unwrap();
            assert_eq!(violations.len(), 1, "{text:?} at {offset}");
            assert_eq!(violations[0].invariant, InvariantId::I6);
            assert_eq!(
                violations[0].message,
                format!("SplitsGrapheme {{ offset: {offset} }}"),
                "{text:?}"
            );
        }
    }
}

#[test]
fn staged_grapheme_probe_rejects_unsupported_inputs() {
    let hidden = Doc::new("text ^id").snapshot();
    assert_eq!(
        probe_plain_cursor_grapheme_boundary(&hidden, hidden.raw_char_idx(0).unwrap()),
        Err(PlainCursorProbeError::HiddenTokensUnsupported)
    );

    let short = Doc::new("x").snapshot();
    let foreign_cursor = Doc::new("hello").raw_char_idx(5).unwrap();
    assert_eq!(
        probe_plain_cursor_grapheme_boundary(&short, foreign_cursor),
        Err(PlainCursorProbeError::InvalidRawPosition(
            DocError::PositionOutOfBounds {
                position: 5,
                length: 1
            }
        ))
    );
}

fn inject(fault: Fault, candidate: &mut ReplayResult) {
    match fault {
        Fault::CorruptSyntheticReplayOutput => {
            candidate.files[0].bytes = b"injected detector fault".to_vec();
        }
        Fault::LeaveDueTimerArmed => {
            panic!("timer fault must use the dedicated TimerWheel detector path")
        }
        Fault::AcceptStaleIndexResult
        | Fault::SkipBlockIdUniquenessCheck
        | Fault::PlaceCursorInsideGrapheme => {
            panic!("staged fault {fault:?} must not be injected before its milestone")
        }
    }
}

fn detect(detector: Detector, expected: &ReplayResult, candidate: &ReplayResult) -> bool {
    match detector {
        Detector::ReplayDeterminismComparison => expected != candidate,
        Detector::CheckIndexMatchesVersions
        | Detector::CheckUniqueLiveBlockIds
        | Detector::CheckPositionsAreValid => {
            panic!("product detector {detector:?} must remain staged until its milestone")
        }
        Detector::CheckNoDueArmedTimers => {
            panic!("timer detector uses the dedicated TimerWheel fault path")
        }
    }
}
