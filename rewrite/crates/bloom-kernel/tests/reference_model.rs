#![cfg(feature = "simulation")]

use bloom_kernel::simulation::{
    run_replay, FailureCode, ReplayCase, ReplayResult, ReplayStep, SimFile, SimFsError,
    StepObservation,
};

const REPLAY_FIXTURE: &str = include_str!("../../../testdata/replays/empty-kernel-smoke.sim");

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReferenceError {
    Missing(String),
    InjectedRead,
    InjectedWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationKind {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExpectedOperation {
    Read(Result<Vec<u8>, ReferenceError>),
    Write(Result<(), ReferenceError>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedFile {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct ReferenceFileSystem {
    files: Vec<ExpectedFile>,
    queued_failures: Vec<OperationKind>,
    operations: Vec<(usize, ExpectedOperation)>,
}

impl ReferenceFileSystem {
    fn from_initial_files(files: &[SimFile]) -> Self {
        Self {
            files: files
                .iter()
                .map(|file| ExpectedFile {
                    path: file.path.clone(),
                    bytes: file.bytes.clone(),
                })
                .collect(),
            ..Self::default()
        }
    }

    fn model(case: &ReplayCase) -> Self {
        let mut reference = Self::from_initial_files(&case.files);
        for (index, step) in case.steps.iter().enumerate() {
            match step {
                ReplayStep::FailNextFsRead => {
                    reference.queued_failures.push(OperationKind::Read);
                }
                ReplayStep::FailNextFsWrite => {
                    reference.queued_failures.push(OperationKind::Write);
                }
                ReplayStep::Read { path } => {
                    let result = if reference.consume_failure(OperationKind::Read) {
                        Err(ReferenceError::InjectedRead)
                    } else {
                        reference
                            .files
                            .iter()
                            .find(|file| file.path == *path)
                            .map(|file| Ok(file.bytes.clone()))
                            .unwrap_or_else(|| Err(ReferenceError::Missing(path.clone())))
                    };
                    reference
                        .operations
                        .push((index, ExpectedOperation::Read(result)));
                }
                ReplayStep::Write { path, bytes } => {
                    let result = if reference.consume_failure(OperationKind::Write) {
                        Err(ReferenceError::InjectedWrite)
                    } else {
                        match reference.files.iter().position(|file| file.path == *path) {
                            Some(existing) => reference.files[existing].bytes.clone_from(bytes),
                            None => reference.files.push(ExpectedFile {
                                path: path.clone(),
                                bytes: bytes.clone(),
                            }),
                        }
                        Ok(())
                    };
                    reference
                        .operations
                        .push((index, ExpectedOperation::Write(result)));
                }
                _ => {}
            }
        }
        reference
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        reference
    }

    fn consume_failure(&mut self, operation: OperationKind) -> bool {
        if self.queued_failures.first() == Some(&operation) {
            self.queued_failures.remove(0);
            true
        } else {
            false
        }
    }
}

fn compare_candidate_with_reference(
    candidate: &ReplayResult,
    reference: &ReferenceFileSystem,
) -> Result<(), Vec<String>> {
    let mut mismatches = Vec::new();

    for (step, expected) in &reference.operations {
        let actual = candidate.observations.get(*step);
        if !matches_expected(expected, actual) {
            mismatches.push(format!(
                "filesystem observation differs at step {step}: expected {expected:?}, got {actual:?}"
            ));
        }
    }

    if candidate.files.len() != reference.files.len() {
        mismatches.push(format!(
            "final file count differs: expected {}, got {}",
            reference.files.len(),
            candidate.files.len()
        ));
    }
    for (index, expected) in reference.files.iter().enumerate() {
        match candidate.files.get(index) {
            Some(actual) if actual.path == expected.path && actual.bytes == expected.bytes => {}
            Some(actual) => mismatches.push(format!(
                "final file differs at position {index}: expected {expected:?}, got {actual:?}"
            )),
            None => break,
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

fn matches_expected(expected: &ExpectedOperation, actual: Option<&StepObservation>) -> bool {
    match (expected, actual) {
        (ExpectedOperation::Read(expected), Some(StepObservation::FsRead(actual))) => {
            match (expected, actual) {
                (Ok(expected), Ok(actual)) => expected == actual,
                (Err(ReferenceError::Missing(expected)), Err(SimFsError::NotFound(actual))) => {
                    expected == actual
                }
                (
                    Err(ReferenceError::InjectedRead),
                    Err(SimFsError::Injected(bloom_kernel::simulation::FsOperation::Read)),
                ) => true,
                _ => false,
            }
        }
        (ExpectedOperation::Write(expected), Some(StepObservation::FsWrite(actual))) => {
            matches!(
                (expected, actual),
                (Ok(()), Ok(()))
                    | (
                        Err(ReferenceError::InjectedWrite),
                        Err(SimFsError::Injected(
                            bloom_kernel::simulation::FsOperation::Write
                        )),
                    )
            )
        }
        _ => false,
    }
}

#[test]
fn synthetic_replay_matches_independent_linear_filesystem_model() {
    let case = ReplayCase::parse(REPLAY_FIXTURE).unwrap();
    let expected = ReferenceFileSystem::model(&case);
    let candidate = run_replay(&case).unwrap();

    assert!(expected.operations.iter().any(|(_, operation)| {
        matches!(
            operation,
            ExpectedOperation::Write(Err(ReferenceError::InjectedWrite))
        )
    }));
    assert!(candidate
        .failures
        .iter()
        .any(|failure| { failure.code == FailureCode::FsWriteInjected }));
    compare_candidate_with_reference(&candidate, &expected).unwrap();

    let mut perturbed_candidate = candidate;
    perturbed_candidate.files[0].bytes = b"deliberately wrong".to_vec();
    assert!(compare_candidate_with_reference(&perturbed_candidate, &expected).is_err());
}
