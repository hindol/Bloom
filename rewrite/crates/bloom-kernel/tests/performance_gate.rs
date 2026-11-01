#![cfg(feature = "simulation")]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerformanceGateResult {
    WithinLimit,
    Regression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerformanceGateError {
    ZeroBaseline,
    SampleOverflow,
}

fn compare_nanoseconds(
    baseline: u128,
    candidate: u128,
) -> Result<PerformanceGateResult, PerformanceGateError> {
    if baseline == 0 {
        return Err(PerformanceGateError::ZeroBaseline);
    }
    let candidate_scaled = candidate
        .checked_mul(100)
        .ok_or(PerformanceGateError::SampleOverflow)?;
    let allowed_scaled = baseline
        .checked_mul(110)
        .ok_or(PerformanceGateError::SampleOverflow)?;
    Ok(if candidate_scaled > allowed_scaled {
        PerformanceGateResult::Regression
    } else {
        PerformanceGateResult::WithinLimit
    })
}

#[test]
fn ten_percent_boundary_is_allowed_and_larger_regressions_fail() {
    assert_eq!(
        compare_nanoseconds(1_000, 1_100),
        Ok(PerformanceGateResult::WithinLimit)
    );
    assert_eq!(
        compare_nanoseconds(1_000, 1_101),
        Ok(PerformanceGateResult::Regression)
    );
    assert_eq!(
        compare_nanoseconds(1_000, 900),
        Ok(PerformanceGateResult::WithinLimit)
    );
}

#[test]
fn invalid_baselines_and_overflow_fail_explicitly() {
    assert_eq!(
        compare_nanoseconds(0, 100),
        Err(PerformanceGateError::ZeroBaseline)
    );
    assert_eq!(
        compare_nanoseconds(u128::MAX, 1),
        Err(PerformanceGateError::SampleOverflow)
    );
}
