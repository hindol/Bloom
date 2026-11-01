use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdout, Command, ExitStatus, Stdio},
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

const PROTOCOL_PREFIX: &str = "BLOOM_WATCHDOG/1 ";
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug)]
pub struct WatchdogLimits {
    pub timeout: Duration,
    pub max_steps: u64,
    pub max_queue_depth: usize,
    pub max_retained_documents: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchdogOutcome {
    Succeeded { steps: u64 },
    Failed { reason: String },
    Blocked { reason: String },
    TimedOut { steps: u64 },
    StepLimitExceeded { observed: u64 },
    QueueLimitExceeded { observed: usize, limit: usize },
    RetainedDocumentsLimitExceeded { observed: usize, limit: usize },
    DueTimerNotConsumed,
    ProtocolFailure(String),
    ExitedWithoutOutcome { exit_code: Option<i32> },
    TerminalButNonzeroExit { exit_code: Option<i32> },
    LaunchFailure(String),
    InvalidLimits,
}

#[derive(Debug)]
enum Message {
    Progress {
        queue_depth: usize,
        retained_documents: usize,
        due_timer_pending: bool,
    },
    Succeeded,
    Failed(String),
    Blocked(String),
    ProtocolFailure(String),
    ReaderClosed,
}

#[derive(Debug)]
enum Terminal {
    Succeeded,
    Failed(String),
    Blocked(String),
}

// This effectful test observer intentionally lives outside the pure kernel.
#[allow(clippy::disallowed_methods)]
pub fn run_watchdog(mut command: Command, limits: WatchdogLimits) -> WatchdogOutcome {
    if limits.timeout.is_zero() || limits.max_steps == 0 {
        return WatchdogOutcome::InvalidLimits;
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return WatchdogOutcome::LaunchFailure(error.to_string()),
    };
    let Some(stdout) = child.stdout.take() else {
        terminate_and_reap(&mut child);
        return WatchdogOutcome::ProtocolFailure("child stdout was not captured".to_owned());
    };

    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || read_protocol(stdout, sender));
    let deadline = Instant::now() + limits.timeout;
    let mut steps = 0;
    let mut terminal = None;
    let mut reader_closed = false;
    let mut exit_status = None;

    loop {
        if exit_status.is_none() {
            match child.try_wait() {
                Ok(status) => exit_status = status,
                Err(error) => {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::ProtocolFailure(format!(
                        "could not observe child process: {error}"
                    ));
                }
            }
        }

        if reader_closed {
            if let Some(status) = exit_status {
                let _ = reader.join();
                return finish(terminal, status, steps);
            }
            thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
        }

        let now = Instant::now();
        if now >= deadline {
            terminate_and_reap(&mut child);
            let _ = reader.join();
            return WatchdogOutcome::TimedOut { steps };
        }

        let wait_for = (deadline - now).min(POLL_INTERVAL);
        match receiver.recv_timeout(wait_for) {
            Ok(Message::Progress {
                queue_depth,
                retained_documents,
                due_timer_pending,
            }) => {
                if terminal.is_some() {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::ProtocolFailure(
                        "progress received after terminal outcome".to_owned(),
                    );
                }
                steps += 1;
                if steps > limits.max_steps {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::StepLimitExceeded { observed: steps };
                }
                if queue_depth > limits.max_queue_depth {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::QueueLimitExceeded {
                        observed: queue_depth,
                        limit: limits.max_queue_depth,
                    };
                }
                if retained_documents > limits.max_retained_documents {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::RetainedDocumentsLimitExceeded {
                        observed: retained_documents,
                        limit: limits.max_retained_documents,
                    };
                }
                if due_timer_pending {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::DueTimerNotConsumed;
                }
            }
            Ok(Message::Succeeded) => {
                if !set_terminal(&mut terminal, Terminal::Succeeded) {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::ProtocolFailure(
                        "multiple terminal outcomes received".to_owned(),
                    );
                }
            }
            Ok(Message::Failed(reason)) => {
                if !set_terminal(&mut terminal, Terminal::Failed(reason)) {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::ProtocolFailure(
                        "multiple terminal outcomes received".to_owned(),
                    );
                }
            }
            Ok(Message::Blocked(reason)) => {
                if !set_terminal(&mut terminal, Terminal::Blocked(reason)) {
                    terminate_and_reap(&mut child);
                    let _ = reader.join();
                    return WatchdogOutcome::ProtocolFailure(
                        "multiple terminal outcomes received".to_owned(),
                    );
                }
            }
            Ok(Message::ProtocolFailure(error)) => {
                terminate_and_reap(&mut child);
                let _ = reader.join();
                return WatchdogOutcome::ProtocolFailure(error);
            }
            Ok(Message::ReaderClosed) => reader_closed = true,
            Err(RecvTimeoutError::Disconnected) => reader_closed = true,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn finish(terminal: Option<Terminal>, status: ExitStatus, steps: u64) -> WatchdogOutcome {
    if !status.success() {
        return WatchdogOutcome::TerminalButNonzeroExit {
            exit_code: status.code(),
        };
    }
    match terminal {
        Some(Terminal::Succeeded) => WatchdogOutcome::Succeeded { steps },
        Some(Terminal::Failed(reason)) => WatchdogOutcome::Failed { reason },
        Some(Terminal::Blocked(reason)) => WatchdogOutcome::Blocked { reason },
        None => WatchdogOutcome::ExitedWithoutOutcome {
            exit_code: status.code(),
        },
    }
}

fn set_terminal(current: &mut Option<Terminal>, next: Terminal) -> bool {
    if current.is_some() {
        false
    } else {
        *current = Some(next);
        true
    }
}

fn terminate_and_reap(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn read_protocol(stdout: ChildStdout, sender: Sender<Message>) {
    for (line_number, line) in BufReader::new(stdout).lines().enumerate() {
        match line {
            Ok(line) => {
                if let Some(protocol_start) = line.find(PROTOCOL_PREFIX) {
                    let message = parse_message(
                        &line[protocol_start + PROTOCOL_PREFIX.len()..],
                        line_number + 1,
                    );
                    let failed = matches!(message, Message::ProtocolFailure(_));
                    if sender.send(message).is_err() || failed {
                        return;
                    }
                }
            }
            Err(error) => {
                let _ = sender.send(Message::ProtocolFailure(format!(
                    "could not read child output: {error}"
                )));
                return;
            }
        }
    }
    let _ = sender.send(Message::ReaderClosed);
}

fn parse_message(line: &str, line_number: usize) -> Message {
    if line == "SUCCEEDED" {
        return Message::Succeeded;
    }
    if let Some(reason) = line.strip_prefix("FAILED ") {
        return Message::Failed(reason.to_owned());
    }
    if let Some(reason) = line.strip_prefix("BLOCKED ") {
        return Message::Blocked(reason.to_owned());
    }
    if let Some(progress) = line.strip_prefix("PROGRESS ") {
        let fields: Vec<_> = progress.split_whitespace().collect();
        if fields.len() != 3 {
            return protocol_error(
                line_number,
                "progress requires queue, retained, and due fields",
            );
        }
        let Some(queue_depth) = fields[0].strip_prefix("queue=").and_then(parse_number) else {
            return protocol_error(line_number, "invalid queue depth");
        };
        let Some(retained_documents) = fields[1].strip_prefix("retained=").and_then(parse_number)
        else {
            return protocol_error(line_number, "invalid retained-document count");
        };
        let due_timer_pending = match fields[2].strip_prefix("due=") {
            Some("0") => false,
            Some("1") => true,
            _ => return protocol_error(line_number, "due must be 0 or 1"),
        };
        return Message::Progress {
            queue_depth,
            retained_documents,
            due_timer_pending,
        };
    }
    protocol_error(line_number, "unknown watchdog message")
}

fn parse_number(value: &str) -> Option<usize> {
    let parsed = value.parse::<usize>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn protocol_error(line_number: usize, reason: &str) -> Message {
    Message::ProtocolFailure(format!(
        "invalid watchdog protocol at child output line {line_number}: {reason}"
    ))
}
