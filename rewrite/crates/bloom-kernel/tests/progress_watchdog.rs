mod support;

use std::process::Command;

use support::{run_watchdog, WatchdogLimits, WatchdogOutcome};

fn child(scenario: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("watchdog_child")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("BLOOM_WATCHDOG_SCENARIO", scenario);
    command
}

fn limits() -> WatchdogLimits {
    WatchdogLimits {
        timeout: std::time::Duration::from_secs(3),
        max_steps: 2,
        max_queue_depth: 4,
        max_retained_documents: 3,
    }
}

#[test]
fn watchdog_child() {
    let Ok(scenario) = std::env::var("BLOOM_WATCHDOG_SCENARIO") else {
        return;
    };

    match scenario.as_str() {
        "success" => {
            emit("BLOOM_WATCHDOG/1 PROGRESS queue=1 retained=1 due=0");
            emit("BLOOM_WATCHDOG/1 SUCCEEDED");
        }
        "failure" => emit("BLOOM_WATCHDOG/1 FAILED synthetic-failure"),
        "blocked" => emit("BLOOM_WATCHDOG/1 BLOCKED waiting-for-explicit-input"),
        "exit" => {}
        "nonzero-exit" => std::process::exit(7),
        "terminal-nonzero-exit" => {
            emit("BLOOM_WATCHDOG/1 SUCCEEDED");
            std::process::exit(7);
        }
        "malformed" => emit("BLOOM_WATCHDOG/1 PROGRESS queue=x retained=0 due=0"),
        "stall" => {
            emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=0 due=0");
            loop {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        "queue-limit" => emit("BLOOM_WATCHDOG/1 PROGRESS queue=5 retained=0 due=0"),
        "retained-limit" => emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=4 due=0"),
        "due-timer" => emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=0 due=1"),
        "step-limit" => {
            emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=0 due=0");
            emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=0 due=0");
            emit("BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=0 due=0");
        }
        _ => panic!("unknown watchdog test scenario {scenario}"),
    }
}

#[test]
fn watchdog_observes_terminal_states_and_unreported_exit() {
    assert_eq!(
        run_watchdog(child("success"), limits()),
        WatchdogOutcome::Succeeded { steps: 1 }
    );
    assert_eq!(
        run_watchdog(child("failure"), limits()),
        WatchdogOutcome::Failed {
            reason: "synthetic-failure".to_owned()
        }
    );
    assert_eq!(
        run_watchdog(child("blocked"), limits()),
        WatchdogOutcome::Blocked {
            reason: "waiting-for-explicit-input".to_owned()
        }
    );
    assert!(matches!(
        run_watchdog(child("exit"), limits()),
        WatchdogOutcome::ExitedWithoutOutcome { exit_code: Some(0) }
    ));
    assert_eq!(
        run_watchdog(child("nonzero-exit"), limits()),
        WatchdogOutcome::ExitedWithoutOutcome { exit_code: Some(7) }
    );
    assert_eq!(
        run_watchdog(child("terminal-nonzero-exit"), limits()),
        WatchdogOutcome::TerminalButNonzeroExit { exit_code: Some(7) }
    );
}

#[test]
fn watchdog_enforces_external_step_resource_and_timer_bounds() {
    assert!(matches!(
        run_watchdog(child("step-limit"), limits()),
        WatchdogOutcome::StepLimitExceeded { observed: 3 }
    ));
    assert_eq!(
        run_watchdog(child("queue-limit"), limits()),
        WatchdogOutcome::QueueLimitExceeded {
            observed: 5,
            limit: 4
        }
    );
    assert_eq!(
        run_watchdog(child("retained-limit"), limits()),
        WatchdogOutcome::RetainedDocumentsLimitExceeded {
            observed: 4,
            limit: 3
        }
    );
    assert_eq!(
        run_watchdog(child("due-timer"), limits()),
        WatchdogOutcome::DueTimerNotConsumed
    );
    assert!(matches!(
        run_watchdog(child("malformed"), limits()),
        WatchdogOutcome::ProtocolFailure(_)
    ));
}

#[test]
#[allow(clippy::disallowed_methods)]
fn watchdog_kills_a_stalled_child_with_its_own_deadline() {
    let mut watchdog_limits = limits();
    watchdog_limits.timeout = std::time::Duration::from_secs(2);
    let started = std::time::Instant::now();
    assert!(matches!(
        run_watchdog(child("stall"), watchdog_limits),
        WatchdogOutcome::TimedOut { steps: 1 }
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

fn emit(message: &str) {
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "{message}").unwrap();
    output.flush().unwrap();
}
