#![cfg(feature = "simulation")]

use std::time::Duration;

use bloom_kernel::{
    simulation::{run_replay, ReplayCase, ReplayResult},
    timer::{check_no_due_armed_timers, TimerId, TimerWheel},
};

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
    LeaveDueTimerArmed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Detector {
    ReplayDeterminismComparison,
    CheckIndexMatchesVersions,
    CheckUniqueLiveBlockIds,
    CheckNoDueArmedTimers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FaultCase {
    fault: Fault,
    detector: Detector,
    status: FaultStatus,
}

const FAULT_CORPUS: [FaultCase; 4] = [
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

    assert_eq!(staged.len(), 2);
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
                )
            )
    }));
}

fn inject(fault: Fault, candidate: &mut ReplayResult) {
    match fault {
        Fault::CorruptSyntheticReplayOutput => {
            candidate.files[0].bytes = b"injected detector fault".to_vec();
        }
        Fault::LeaveDueTimerArmed => {
            panic!("timer fault must use the dedicated TimerWheel detector path")
        }
        Fault::AcceptStaleIndexResult | Fault::SkipBlockIdUniquenessCheck => {
            panic!("staged fault {fault:?} must not be injected before its milestone")
        }
    }
}

fn detect(detector: Detector, expected: &ReplayResult, candidate: &ReplayResult) -> bool {
    match detector {
        Detector::ReplayDeterminismComparison => expected != candidate,
        Detector::CheckIndexMatchesVersions | Detector::CheckUniqueLiveBlockIds => {
            panic!("product detector {detector:?} must remain staged until its milestone")
        }
        Detector::CheckNoDueArmedTimers => {
            panic!("timer detector uses the dedicated TimerWheel fault path")
        }
    }
}
